use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::process::Child;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError, mpsc};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_FILE_NOT_FOUND, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, RECT,
    STILL_ACTIVE, WAIT_OBJECT_0, WIN32_ERROR, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, GetMonitorInfoW,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_HIDDEN, FILE_FLAGS_AND_ATTRIBUTES, GetDiskFreeSpaceExW, GetFileAttributesW,
    INVALID_FILE_ATTRIBUTES, MOVE_FILE_FLAGS, MoveFileExW, SetFileAttributesW,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegDeleteTreeW,
    RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::System::Threading::{
    GetCurrentThreadId, GetExitCodeProcess, OpenProcess, OpenThread,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, ResumeThread, THREAD_SUSPEND_RESUME,
    WaitForSingleObject,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
};
use windows::Win32::UI::Shell::{ABM_GETAUTOHIDEBAREX, APPBARDATA, SHAppBarMessage};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, EVENT_SYSTEM_FOREGROUND, GWL_STYLE, GetClassNameW, GetDesktopWindow,
    GetForegroundWindow, GetMessageW, GetShellWindow, GetWindowLongPtrW, GetWindowRect, IsZoomed,
    MONITORINFOF_PRIMARY, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
    SW_HIDE, SW_SHOWNOACTIVATE, SetTimer, SetWindowDisplayAffinity, SetWindowsHookExW, ShowWindow,
    UnhookWindowsHookEx, WDA_EXCLUDEFROMCAPTURE, WH_MOUSE_LL, WINEVENT_OUTOFCONTEXT,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_QUIT, WM_TIMER,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW,
    SetWindowPos, WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU,
};
use windows::core::PCWSTR;

/// `CREATE_NO_WINDOW`: engine consoles never flash on screen.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// `CREATE_SUSPENDED`: the main thread waits for `ResumeThread`.
pub const CREATE_SUSPENDED: u32 = 0x0000_0004;

/// NUL-terminated UTF-16 path. Absolute paths of 248+ chars get the `\?\`
/// (or `\?\UNC\`) prefix so these Win32 calls work past MAX_PATH. The verbatim
/// prefix switches off Win32's `.`/`..` handling, so long paths are normalised first.
fn wide(p: &Path) -> Vec<u16> {
    let plain = p.as_os_str().len() < 248 || p.as_os_str().encode_wide().take(4).eq(VERBATIM);
    let normal = if plain {
        None
    } else {
        std::path::absolute(p).ok()
    };
    let p = normal.as_deref().unwrap_or(p);
    let mut out = verbatim_wide(p.as_os_str(), p.is_absolute());
    out.push(0);
    out
}

const BS: u16 = b'\\' as u16;
const VERBATIM: [u16; 4] = [BS, BS, b'?' as u16, BS];

/// The pure string transform behind `wide` (no NUL): forward slashes become
/// backslashes and long absolute paths get the verbatim prefix.
fn verbatim_wide(raw: &std::ffi::OsStr, absolute: bool) -> Vec<u16> {
    let raw: Vec<u16> = raw
        .encode_wide()
        .map(|c| if c == b'/' as u16 { BS } else { c })
        .collect();
    if raw.len() >= 248 && !raw.starts_with(&VERBATIM) && absolute {
        if raw.starts_with(&[BS, BS]) {
            r"\\?\UNC"
                .encode_utf16()
                .chain(raw[1..].iter().copied())
                .collect()
        } else {
            r"\\?\".encode_utf16().chain(raw.iter().copied()).collect()
        }
    } else {
        raw
    }
}

fn to_io(e: windows::core::Error) -> io::Error {
    io::Error::other(e.to_string())
}

/// A Windows Job Object with KILL_ON_JOB_CLOSE: killing it (or dropping it,
/// or Recast crashing) kills every process assigned to it and their children.
pub struct KillGroup {
    raw: usize,
}

impl KillGroup {
    pub fn new() -> io::Result<Self> {
        unsafe {
            let job = CreateJobObjectW(None, PCWSTR::null()).map_err(to_io)?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let set = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if let Err(e) = set {
                let _ = CloseHandle(job);
                return Err(to_io(e));
            }
            Ok(Self {
                raw: job.0 as usize,
            })
        }
    }

    fn handle(&self) -> HANDLE {
        HANDLE(self.raw as *mut c_void)
    }

    /// Prefer `spawn_contained`, which assigns before the process runs any code.
    pub fn assign(&self, child: &Child) -> io::Result<()> {
        unsafe {
            AssignProcessToJobObject(self.handle(), HANDLE(child.as_raw_handle())).map_err(to_io)
        }
    }

    pub fn kill(&self) {
        unsafe {
            let _ = TerminateJobObject(self.handle(), 1);
        }
    }
}

impl Drop for KillGroup {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle());
        }
    }
}

/// Spawn `cmd` suspended (and windowless) and put it in `group`. It has executed no
/// code yet: call `resume_process_main_thread` to let it run. Overrides any creation
/// flags on `cmd`.
pub fn spawn_suspended_in_group(
    cmd: &mut std::process::Command,
    group: &KillGroup,
) -> io::Result<Child> {
    use std::os::windows::process::CommandExt;
    let mut child = cmd
        .creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED)
        .spawn()?;
    if let Err(e) = group.assign(&child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    Ok(child)
}

/// Spawn `cmd` suspended, put it in `group`, then let it run, so no child it
/// starts can escape the group.
pub fn spawn_contained(cmd: &mut std::process::Command, group: &KillGroup) -> io::Result<Child> {
    let mut child = spawn_suspended_in_group(cmd, group)?;
    if let Err(e) = resume_process_main_thread(child.id()) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    Ok(child)
}

/// Resume every thread of a process created with CREATE_SUSPENDED (only the main
/// thread exists then).
pub fn resume_process_main_thread(pid: u32) -> io::Result<()> {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0).map_err(to_io)?;
        let mut entry = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut resumed = false;
        let mut more = Thread32First(snap, &mut entry).is_ok();
        while more {
            if entry.th32OwnerProcessID == pid
                && let Ok(t) = OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID)
            {
                resumed |= ResumeThread(t) != u32::MAX;
                let _ = CloseHandle(t);
            }
            more = Thread32Next(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
        if resumed {
            Ok(())
        } else {
            Err(io::Error::other(format!("couldn't resume process {pid}")))
        }
    }
}

/// Bytes available to the current user on the volume holding `path`.
pub fn free_space(path: &Path) -> Option<u64> {
    let w = wide(path);
    let mut avail = 0u64;
    unsafe {
        GetDiskFreeSpaceExW(PCWSTR(w.as_ptr()), Some(&mut avail as *mut u64), None, None).ok()?
    };
    Some(avail)
}

pub fn set_hidden(path: &Path) -> io::Result<()> {
    let w = wide(path);
    unsafe {
        let attrs = GetFileAttributesW(PCWSTR(w.as_ptr()));
        if attrs == INVALID_FILE_ATTRIBUTES {
            return Err(io::Error::last_os_error());
        }
        SetFileAttributesW(
            PCWSTR(w.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(attrs | FILE_ATTRIBUTE_HIDDEN.0),
        )
        .map_err(to_io)
    }
}

pub fn is_hidden(path: &Path) -> bool {
    let w = wide(path);
    let attrs = unsafe { GetFileAttributesW(PCWSTR(w.as_ptr())) };
    attrs != INVALID_FILE_ATTRIBUTES && attrs & FILE_ATTRIBUTE_HIDDEN.0 != 0
}

/// Rename a file or folder on the same volume, failing if `to` exists
/// (std::fs::rename would silently replace an existing file on Windows).
pub fn move_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    let (f, t) = (wide(from), wide(to));
    unsafe { MoveFileExW(PCWSTR(f.as_ptr()), PCWSTR(t.as_ptr()), MOVE_FILE_FLAGS(0)) }
        .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))
}

/// ERROR_FILE_EXISTS (80) or ERROR_ALREADY_EXISTS (183).
pub fn is_already_exists(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(80) | Some(183))
}

/// Access denied (5), sharing violation (32), lock violation (33): what
/// antivirus, OneDrive and Dropbox cause while they briefly hold a new file.
pub fn is_transient_lock(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(5) | Some(32) | Some(33))
}

/// Diagnostics for tests: is a process with this pid still running?
pub fn is_process_alive(pid: u32) -> bool {
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => {
                let mut code = 0u32;
                let ok = GetExitCodeProcess(h, &mut code).is_ok();
                let _ = CloseHandle(h);
                ok && code == STILL_ACTIVE.0 as u32
            }
            Err(_) => false,
        }
    }
}

/// A handle that becomes signalled when the process it was opened for exits.
pub struct ProcessWaiter(HANDLE);

// A process handle may be used from any thread.
unsafe impl Send for ProcessWaiter {}

impl ProcessWaiter {
    /// True if the process has exited (or already had) within `timeout`.
    pub fn wait(&self, timeout: std::time::Duration) -> bool {
        let ms = timeout.as_millis().min(u32::MAX as u128 - 1) as u32;
        unsafe { WaitForSingleObject(self.0, ms) == WAIT_OBJECT_0 }
    }
}

impl Drop for ProcessWaiter {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Open `pid` so its exit can be waited on; None if it can't be opened (e.g. already gone).
pub fn open_for_wait(pid: u32) -> Option<ProcessWaiter> {
    unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            pid,
        )
    }
    .ok()
    .map(ProcessWaiter)
}

/// A screen rectangle in physical pixels (virtual-screen coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// Copy what is on screen under `r` as top-down BGRA. Windows marked
/// `exclude_from_capture` (the pad) are not in the image.
pub fn capture_region(r: ScreenRect) -> io::Result<Vec<u8>> {
    if r.w == 0 || r.h == 0 {
        return Err(io::Error::other("empty capture rect"));
    }
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err(io::Error::other("GetDC failed"));
        }
        let mem = CreateCompatibleDC(Some(screen));
        if mem.is_invalid() {
            ReleaseDC(None, screen);
            return Err(io::Error::other("CreateCompatibleDC failed"));
        }
        let bmp = CreateCompatibleBitmap(screen, r.w as i32, r.h as i32);
        if bmp.is_invalid() {
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            return Err(io::Error::other("CreateCompatibleBitmap failed"));
        }
        let old = SelectObject(mem, bmp.into());
        let copied = BitBlt(
            mem,
            0,
            0,
            r.w as i32,
            r.h as i32,
            Some(screen),
            r.x,
            r.y,
            SRCCOPY,
        );
        // GetDIBits needs the bitmap deselected from every DC.
        SelectObject(mem, old);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: r.w as i32,
                biHeight: -(r.h as i32), // negative: top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; r.w as usize * r.h as usize * 4];
        let lines = if copied.is_ok() {
            GetDIBits(
                screen,
                bmp,
                0,
                r.h,
                Some(buf.as_mut_ptr() as *mut c_void),
                &mut info,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        copied.map_err(to_io)?;
        if lines != r.h as i32 {
            return Err(io::Error::other("GetDIBits copied too few lines"));
        }
        Ok(buf)
    }
}

/// Keep a window out of every screen capture, including Recast's own (spec §6.8).
/// `hwnd` is the raw handle value (tauri's HWND type comes from another `windows` version).
pub fn exclude_from_capture(hwnd: isize) -> io::Result<()> {
    unsafe { SetWindowDisplayAffinity(HWND(hwnd as *mut c_void), WDA_EXCLUDEFROMCAPTURE) }
        .map_err(to_io)
}

/// Show a window without taking focus, so an OLE drag in progress keeps going (spec §6.7).
pub fn show_no_activate(hwnd: isize) {
    unsafe {
        let _ = ShowWindow(HWND(hwnd as *mut c_void), SW_SHOWNOACTIVATE);
    }
}

/// Hide a window that `show_no_activate` showed.
///
/// `show_no_activate` bypasses tao, so tao's cached VISIBLE flag stays false: a later
/// `WebviewWindow::hide()` sees "already hidden" and does nothing, and `set_focus` is
/// skipped too. Hiding through the OS directly keeps the real state correct; the
/// tao flag is resynced by the next tauri `show()`.
pub fn hide_window(hwnd: isize) {
    unsafe {
        let _ = ShowWindow(HWND(hwnd as *mut c_void), SW_HIDE);
    }
}

/// `style` without the caption and the buttons that go with it.
fn without_caption(style: u32) -> u32 {
    style & !(WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0)
}

/// Windows keeps a minimum width for any window with `WS_CAPTION`, which tao leaves set
/// on undecorated windows: a 6 px handle comes out 198 px wide. Dropping the caption
/// bits lets a widget window be as thin as asked.
pub fn strip_caption(hwnd: isize) {
    unsafe {
        let h = HWND(hwnd as *mut c_void);
        let style = GetWindowLongPtrW(h, GWL_STYLE);
        let new = without_caption(style as u32) as isize | (style & !0xFFFF_FFFF);
        if new != style {
            SetWindowLongPtrW(h, GWL_STYLE, new);
            let _ = SetWindowPos(
                h,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

/// Any mouse button is physically down right now (left, right or middle, by the physical
/// key code, so swapped buttons need no special case). Polled; works without the hook.
pub fn any_button_down() -> bool {
    [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON]
        .iter()
        .any(|vk| unsafe { (GetAsyncKeyState(vk.0 as i32) as u16) & 0x8000 != 0 })
}

/// An auto-hide appbar (taskbar) is docked on `edge` (`ABE_*`: left 0, top 1, right 2,
/// bottom 3) of the monitor `monitor` (left, top, right, bottom).
pub fn autohide_bar_on(edge: u32, monitor: [i32; 4]) -> bool {
    let mut data = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        uEdge: edge,
        rc: RECT {
            left: monitor[0],
            top: monitor[1],
            right: monitor[2],
            bottom: monitor[3],
        },
        ..Default::default()
    };
    unsafe { SHAppBarMessage(ABM_GETAUTOHIDEBAREX, &mut data) != 0 }
}

/// The foreground window and its monitor, as left, top, right, bottom.
#[derive(Debug, Clone)]
pub struct ForegroundInfo {
    pub window: [i32; 4],
    pub monitor: [i32; 4],
    pub primary: bool,
    pub class: String,
    /// The window is maximized (B8): not the same as fullscreen.
    pub zoomed: bool,
}

pub fn foreground_info() -> Option<ForegroundInfo> {
    unsafe {
        let h = GetForegroundWindow();
        if h.is_invalid() || h == GetDesktopWindow() || h == GetShellWindow() {
            return None;
        }
        let mut r = RECT::default();
        GetWindowRect(h, &mut r).ok()?;
        let m = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(m, &mut mi).as_bool() {
            return None;
        }
        let mut buf = [0u16; 128];
        let n = GetClassNameW(h, &mut buf).max(0) as usize;
        let mr = mi.rcMonitor;
        Some(ForegroundInfo {
            window: [r.left, r.top, r.right, r.bottom],
            monitor: [mr.left, mr.top, mr.right, mr.bottom],
            primary: mi.dwFlags & MONITORINFOF_PRIMARY != 0,
            class: String::from_utf16_lossy(&buf[..n]),
            zoomed: IsZoomed(h).as_bool(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseInput {
    LeftDown,
    LeftUp,
    Move,
}

#[derive(Debug, Clone, Copy)]
pub struct MouseEvent {
    /// GetTickCount-style milliseconds.
    pub t_ms: u32,
    pub x: i32,
    pub y: i32,
    pub input: MouseInput,
}

type MouseSink = Box<dyn FnMut(MouseEvent) + Send>;
/// The hook callback's only way out: a channel to the worker (B5), tagged with the id of
/// the hook that owns it so a stale hook's `Drop` cannot clear a newer hook's sender.
static MOUSE_TX: Mutex<Option<(u64, mpsc::Sender<MouseEvent>)>> = Mutex::new(None);
static NEXT_HOOK_ID: AtomicU64 = AtomicU64::new(1);

/// Hand an event to the current hook's worker. Never blocks.
fn forward_mouse(e: MouseEvent) {
    if let Ok(tx) = MOUSE_TX.try_lock()
        && let Some((_, tx)) = tx.as_ref()
    {
        let _ = tx.send(e);
    }
}

unsafe extern "system" fn ll_mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let input = match wparam.0 as u32 {
            WM_MOUSEMOVE => Some(MouseInput::Move),
            WM_LBUTTONDOWN => Some(MouseInput::LeftDown),
            WM_LBUTTONUP => Some(MouseInput::LeftUp),
            _ => None,
        };
        // Never block the system's mouse: try_lock and an unbounded send, nothing else.
        if let Some(input) = input {
            let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            forward_mouse(MouseEvent {
                t_ms: info.time,
                x: info.pt.x,
                y: info.pt.y,
                input,
            });
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// A global low-level mouse hook (spec §6.7). The hook thread only forwards events over
/// a channel; a worker thread runs the sink (B5). Dropping it unhooks and joins both.
/// One at a time: starting a second replaces the first's channel.
pub struct MouseHook {
    id: u64,
    thread_id: u32,
    hook_thread: Option<std::thread::JoinHandle<()>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl MouseHook {
    pub fn start(mut sink: MouseSink) -> io::Result<MouseHook> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
        let hook_thread = std::thread::Builder::new()
            .name("recast-mouse-hook".into())
            .spawn(move || unsafe {
                let mut msg = MSG::default();
                // Create this thread's message queue before anyone posts WM_QUIT to it.
                let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                let module = GetModuleHandleW(None).ok().map(|m| HINSTANCE(m.0));
                let hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(ll_mouse), module, 0) {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(GetCurrentThreadId()));
                while GetMessageW(&mut msg, None, 0, 0).0 > 0 {}
                let _ = UnhookWindowsHookEx(hook);
            })?;
        let thread_id = match ready_rx.recv() {
            Ok(Ok(id)) => id,
            Ok(Err(e)) => {
                let _ = hook_thread.join();
                return Err(io::Error::other(e));
            }
            Err(_) => return Err(io::Error::other("the mouse hook thread exited")),
        };
        let (tx, rx) = mpsc::channel::<MouseEvent>();
        let worker = std::thread::Builder::new()
            .name("recast-mouse-worker".into())
            .spawn(move || {
                while let Ok(e) = rx.recv() {
                    sink(e);
                }
            });
        let worker = match worker {
            Ok(w) => w,
            Err(e) => {
                post_quit(thread_id);
                let _ = hook_thread.join();
                return Err(e);
            }
        };
        let id = NEXT_HOOK_ID.fetch_add(1, Ordering::Relaxed);
        *MOUSE_TX.lock().unwrap_or_else(PoisonError::into_inner) = Some((id, tx));
        Ok(MouseHook {
            id,
            thread_id,
            hook_thread: Some(hook_thread),
            worker: Some(worker),
        })
    }
}

fn post_quit(thread_id: u32) {
    unsafe {
        let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
    }
}

impl Drop for MouseHook {
    fn drop(&mut self) {
        post_quit(self.thread_id);
        if let Some(j) = self.hook_thread.take() {
            let _ = j.join();
        }
        // Unhooked: no more sends. Dropping the sender ends the worker's loop. Only clear
        // the sender if it is still ours: a newer hook may have replaced it.
        {
            let mut g = MOUSE_TX.lock().unwrap_or_else(PoisonError::into_inner);
            if g.as_ref().is_some_and(|(id, _)| *id == self.id) {
                *g = None;
            }
        }
        if let Some(j) = self.worker.take() {
            let _ = j.join();
        }
    }
}

static FOREGROUND_SINK: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

unsafe extern "system" fn on_foreground(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    if let Some(f) = FOREGROUND_SINK.get() {
        f();
    }
}

/// Call `f` whenever the foreground window changes and once a second (spec §6.7), on a
/// dedicated thread, for the life of the process. Only the first call starts anything.
pub fn watch_foreground(f: Box<dyn Fn() + Send + Sync>) -> io::Result<()> {
    if FOREGROUND_SINK.set(f).is_err() {
        return Err(io::Error::other(
            "the foreground watcher is already running",
        ));
    }
    std::thread::Builder::new()
        .name("recast-foreground".into())
        .spawn(|| unsafe {
            let hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(on_foreground),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            let _timer = SetTimer(None, 0, 1000, None);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                if msg.message == WM_TIMER
                    && let Some(f) = FOREGROUND_SINK.get()
                {
                    f();
                }
            }
            let _ = UnhookWinEvent(hook);
        })?;
    Ok(())
}

/// Windows "Transparency effects" (Settings → Personalisation → Colours). Missing value = on.
pub fn transparency_enabled() -> bool {
    hkcu_dword(PERSONALIZE, "EnableTransparency") != Some(0)
}

fn wstr(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn reg_result(e: WIN32_ERROR) -> io::Result<()> {
    if e.is_ok() {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(e.0 as i32))
    }
}

/// Write a REG_SZ value under HKCU, creating the key. `name` None is the key's default value.
pub fn hkcu_set_string(key: &str, name: Option<&str>, data: &str) -> io::Result<()> {
    let (k, n, d) = (wstr(key), name.map(wstr), wstr(data));
    let n = n.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr()));
    let r = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(k.as_ptr()),
            n,
            REG_SZ.0,
            Some(d.as_ptr() as *const c_void),
            (d.len() * 2) as u32,
        )
    };
    reg_result(r)
}

/// A REG_SZ value under HKCU, or None if the key or value is missing.
pub fn hkcu_get_string(key: &str, name: Option<&str>) -> Option<String> {
    let (k, n) = (wstr(key), name.map(wstr));
    let n = n.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr()));
    let mut size = 0u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(k.as_ptr()),
            n,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    if r.is_err() || size == 0 {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(k.as_ptr()),
            n,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    };
    if r.is_err() {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

/// Delete an HKCU key with everything under it. A missing key is not an error.
pub fn hkcu_delete_tree(key: &str) -> io::Result<()> {
    let k = wstr(key);
    let r = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr())) };
    if r == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    reg_result(r)
}

/// Delete one HKCU value. A missing key or value is not an error.
pub fn hkcu_delete_value(key: &str, name: &str) -> io::Result<()> {
    let (k, n) = (wstr(key), wstr(name));
    let r =
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr()), PCWSTR(n.as_ptr())) };
    if r == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    reg_result(r)
}

/// A REG_DWORD value under HKCU, or None if missing.
pub fn hkcu_dword(key: &str, name: &str) -> Option<u32> {
    let (k, n) = (wstr(key), wstr(name));
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(k.as_ptr()),
            PCWSTR(n.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    r.is_ok().then_some(value)
}

const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

/// The taskbar (and tray) uses the light theme. A missing value means dark, as Windows 10 defaults.
pub fn taskbar_is_light() -> bool {
    hkcu_dword(PERSONALIZE, "SystemUsesLightTheme") == Some(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn kill_group_terminates_assigned_process() {
        let mut child = Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let group = KillGroup::new().unwrap();
        group.assign(&child).unwrap();
        let pid = child.id();
        assert!(is_process_alive(pid));
        group.kill();
        let status = child.wait().unwrap();
        assert!(!status.success());
        assert!(!is_process_alive(pid));
    }

    #[test]
    fn spawn_contained_runs_the_process_inside_the_group() {
        let group = KillGroup::new().unwrap();
        let mut child = spawn_contained(
            Command::new("cmd")
                .args(["/c", "exit 7"])
                .stdout(Stdio::null()),
            &group,
        )
        .unwrap();
        assert_eq!(
            child.wait().unwrap().code(),
            Some(7),
            "the suspended main thread was resumed"
        );
        let mut hung = spawn_contained(
            Command::new("ping")
                .args(["-n", "30", "127.0.0.1"])
                .stdout(Stdio::null()),
            &group,
        )
        .unwrap();
        group.kill();
        assert!(!hung.wait().unwrap().success());
    }

    #[test]
    fn a_contained_process_has_not_run_before_it_is_resumed() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("marker.txt");
        let group = KillGroup::new().unwrap();
        let mut cmd = Command::new(crate::testutil::fake_engine());
        cmd.arg("marker").arg(&marker).stdout(Stdio::null());
        let mut child = spawn_suspended_in_group(&mut cmd, &group).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(!marker.exists(), "it ran before being resumed");
        assert!(is_process_alive(child.id()));
        resume_process_main_thread(child.id()).unwrap();
        assert!(child.wait().unwrap().success());
        assert!(marker.exists(), "it never ran after the resume");
    }

    #[test]
    fn wide_prefixes_only_long_absolute_paths() {
        let w =
            |s: &str| String::from_utf16(&verbatim_wide(std::ffi::OsStr::new(s), true)).unwrap();
        assert_eq!(w(r"C:\short\a.txt"), r"C:\short\a.txt");
        assert_eq!(w("C:/short/a.txt"), r"C:\short\a.txt");
        let long = format!(r"C:\{}", "x".repeat(250));
        assert_eq!(w(&long), format!(r"\\?\{long}"));
        let unc = format!(r"\\server\share\{}", "x".repeat(250));
        assert_eq!(w(&unc), format!(r"\\?\UNC{}", &unc[1..]));
        let verbatim = format!(r"\\?\C:\{}", "x".repeat(250));
        assert_eq!(w(&verbatim), verbatim);
        // Relative paths are never prefixed.
        let rel = "y".repeat(300);
        assert_eq!(
            String::from_utf16(&verbatim_wide(std::ffi::OsStr::new(&rel), false)).unwrap(),
            rel
        );
    }

    #[test]
    fn long_paths_with_dot_dot_segments_are_normalised() {
        let dir = tempfile::tempdir().unwrap();
        let mut deep = dir.path().to_path_buf();
        while deep.as_os_str().len() < 300 {
            deep.push("a-fairly-long-folder-name-for-testing");
        }
        std::fs::create_dir_all(deep.join("detour")).unwrap();
        std::fs::write(deep.join("in.txt"), "x").unwrap();
        // The same folder, spelled by walking into a child and back out with "..".
        let via = deep.join("detour").join("..");
        move_no_replace(&via.join("in.txt"), &via.join("out.txt")).unwrap();
        assert!(deep.join("out.txt").exists());
        assert!(!deep.join("in.txt").exists());
        set_hidden(&via).unwrap();
        assert!(is_hidden(&deep));
        assert!(!is_hidden(&deep.join("detour")));
    }

    #[test]
    fn free_space_reports_bytes() {
        let dir = tempfile::tempdir().unwrap();
        assert!(free_space(dir.path()).unwrap() > 0);
    }

    #[test]
    fn set_hidden_marks_directory() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join(".recast-x");
        std::fs::create_dir(&sub).unwrap();
        set_hidden(&sub).unwrap();
        assert!(is_hidden(&sub));
    }

    #[test]
    fn move_no_replace_refuses_to_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        let err = move_no_replace(&a, &b).unwrap_err();
        assert!(is_already_exists(&err));
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "b");
        let c = dir.path().join("c.txt");
        move_no_replace(&a, &c).unwrap();
        assert_eq!(std::fs::read_to_string(&c).unwrap(), "a");
    }

    #[test]
    fn long_paths_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut deep = dir.path().to_path_buf();
        while deep.as_os_str().len() < 300 {
            deep.push("a-fairly-long-folder-name-for-testing");
        }
        std::fs::create_dir_all(&deep).unwrap(); // std handles long paths itself
        let from = deep.join("in.txt");
        std::fs::write(&from, "x").unwrap();
        let to = deep.join("out.txt");
        move_no_replace(&from, &to).unwrap();
        assert!(to.exists());
        set_hidden(&deep).unwrap();
        assert!(is_hidden(&deep));
        assert!(free_space(&deep).is_some());
    }

    #[test]
    fn captures_a_small_screen_region() {
        let px = capture_region(ScreenRect {
            x: 0,
            y: 0,
            w: 16,
            h: 8,
        })
        .unwrap();
        assert_eq!(px.len(), 16 * 8 * 4);
        assert!(
            capture_region(ScreenRect {
                x: 0,
                y: 0,
                w: 0,
                h: 8
            })
            .is_err()
        );
    }

    #[test]
    fn transparency_setting_reads_without_panicking() {
        let _ = transparency_enabled();
    }

    fn scratch_key() -> String {
        format!(r"Software\RecastTest-{}", uuid::Uuid::new_v4())
    }

    #[test]
    fn registry_strings_round_trip_and_trees_delete() {
        let key = scratch_key();
        let sub = format!(r"{key}\command");
        hkcu_set_string(&key, None, "Open with Recast").unwrap();
        hkcu_set_string(&key, Some("MultiSelectModel"), "Player").unwrap();
        hkcu_set_string(&sub, None, r#""C:\x\recast.exe" --pad "%1""#).unwrap();
        assert_eq!(
            hkcu_get_string(&key, None).as_deref(),
            Some("Open with Recast")
        );
        assert_eq!(
            hkcu_get_string(&key, Some("MultiSelectModel")).as_deref(),
            Some("Player")
        );
        assert_eq!(
            hkcu_get_string(&sub, None).as_deref(),
            Some(r#""C:\x\recast.exe" --pad "%1""#)
        );
        hkcu_delete_value(&key, "MultiSelectModel").unwrap();
        assert_eq!(hkcu_get_string(&key, Some("MultiSelectModel")), None);
        hkcu_delete_value(&key, "MultiSelectModel").unwrap(); // missing is fine
        hkcu_delete_tree(&key).unwrap();
        assert_eq!(hkcu_get_string(&key, None), None);
        assert_eq!(hkcu_get_string(&sub, None), None);
        hkcu_delete_tree(&key).unwrap(); // missing is fine
    }

    #[test]
    fn caption_bits_come_off_and_the_rest_stay() {
        // What tao leaves on an undecorated popup: caption, system menu, min/max, clip bits.
        assert_eq!(without_caption(0x14CB_0000), 0x1400_0000);
        assert_eq!(without_caption(0x0400_0000), 0x0400_0000);
    }

    #[test]
    fn dword_reads_and_theme_reads_without_panicking() {
        let key = scratch_key();
        assert_eq!(hkcu_dword(&key, "Nope"), None);
        let _ = taskbar_is_light();
        let _ = transparency_enabled();
    }

    /// The mouse hook is process-global; tests that start one must not overlap.
    static HOOK_TEST: Mutex<()> = Mutex::new(());

    #[test]
    fn the_mouse_hook_starts_and_stops_cleanly() {
        let _g = HOOK_TEST.lock().unwrap_or_else(PoisonError::into_inner);
        let hook = MouseHook::start(Box::new(|_e| {})).unwrap();
        drop(hook);
        // A second hook after the first is gone works too.
        drop(MouseHook::start(Box::new(|_e| {})).unwrap());
    }

    #[test]
    fn dropping_an_old_hook_does_not_cut_off_a_newer_one() {
        let _g = HOOK_TEST.lock().unwrap_or_else(PoisonError::into_inner);
        const MARK: i32 = 987_654;
        let a = MouseHook::start(Box::new(|_e| {})).unwrap();
        let (tx, rx) = mpsc::channel::<i32>();
        let b = MouseHook::start(Box::new(move |e| {
            let _ = tx.send(e.x);
        }))
        .unwrap();
        // The old hook goes away after the new one started.
        drop(a);
        forward_mouse(MouseEvent {
            t_ms: 0,
            x: MARK,
            y: 0,
            input: MouseInput::Move,
        });
        let got = std::iter::from_fn(|| rx.recv_timeout(std::time::Duration::from_secs(2)).ok())
            .any(|x| x == MARK);
        assert!(got, "the newer hook stopped receiving events");
        drop(b);
    }

    #[test]
    fn button_and_appbar_queries_do_not_panic() {
        let _ = any_button_down();
        let _ = autohide_bar_on(3, [0, 0, 1920, 1080]);
    }

    #[test]
    fn foreground_info_reads_without_panicking() {
        if let Some(f) = foreground_info() {
            assert!(f.monitor[2] > f.monitor[0] && f.monitor[3] > f.monitor[1]);
        }
    }
}
