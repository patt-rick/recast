use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::process::Child;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, STILL_ACTIVE, WAIT_OBJECT_0};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
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
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, OpenThread, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, ResumeThread, THREAD_SUSPEND_RESUME, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SW_HIDE, SW_SHOWNOACTIVATE, SetWindowDisplayAffinity, ShowWindow, WDA_EXCLUDEFROMCAPTURE,
};
use windows::core::{PCWSTR, w};

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

/// Windows "Transparency effects" (Settings → Personalisation → Colours). Missing value = on.
pub fn transparency_enabled() -> bool {
    let mut value = 1u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("EnableTransparency"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size as *mut u32),
        )
    };
    !r.is_ok() || value != 0
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
}
