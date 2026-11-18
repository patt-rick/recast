use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::process::Child;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, STILL_ACTIVE};
use windows::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetFileAttributesW, MoveFileExW, SetFileAttributesW,
    FILE_ATTRIBUTE_HIDDEN, FILE_FLAGS_AND_ATTRIBUTES, INVALID_FILE_ATTRIBUTES, MOVE_FILE_FLAGS,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// `CREATE_NO_WINDOW`: engine consoles never flash on screen.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// NUL-terminated UTF-16 path. Absolute paths of 248+ chars get the `\?\`
/// (or `\?\UNC\`) prefix so these Win32 calls work past MAX_PATH.
fn wide(p: &Path) -> Vec<u16> {
    const BS: u16 = b'\\' as u16;
    let raw: Vec<u16> = p.as_os_str().encode_wide().map(|c| if c == b'/' as u16 { BS } else { c }).collect();
    let verbatim = raw.starts_with(&[BS, BS, b'?' as u16, BS]);
    let mut out: Vec<u16> = if raw.len() >= 248 && !verbatim && p.is_absolute() {
        if raw.starts_with(&[BS, BS]) {
            r"\\?\UNC".encode_utf16().chain(raw[1..].iter().copied()).collect()
        } else {
            r"\\?\".encode_utf16().chain(raw.iter().copied()).collect()
        }
    } else {
        raw
    };
    out.push(0);
    out
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
            Ok(Self { raw: job.0 as usize })
        }
    }

    fn handle(&self) -> HANDLE {
        HANDLE(self.raw as *mut c_void)
    }

    /// Assign right after spawn. Engines don't spawn children in their first
    /// instructions, so the window before assignment is harmless in practice.
    pub fn assign(&self, child: &Child) -> io::Result<()> {
        unsafe { AssignProcessToJobObject(self.handle(), HANDLE(child.as_raw_handle())).map_err(to_io) }
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

/// Bytes available to the current user on the volume holding `path`.
pub fn free_space(path: &Path) -> Option<u64> {
    let w = wide(path);
    let mut avail = 0u64;
    unsafe { GetDiskFreeSpaceExW(PCWSTR(w.as_ptr()), Some(&mut avail as *mut u64), None, None).ok()? };
    Some(avail)
}

pub fn set_hidden(path: &Path) -> io::Result<()> {
    let w = wide(path);
    unsafe {
        let attrs = GetFileAttributesW(PCWSTR(w.as_ptr()));
        if attrs == INVALID_FILE_ATTRIBUTES {
            return Err(io::Error::last_os_error());
        }
        SetFileAttributesW(PCWSTR(w.as_ptr()), FILE_FLAGS_AND_ATTRIBUTES(attrs | FILE_ATTRIBUTE_HIDDEN.0))
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
}
