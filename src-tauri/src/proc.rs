//! Short helper processes with a timeout (never a shell).
use std::ffi::OsStr;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::platform;

/// Run `program args…`; return stdout if it exits 0 within `timeout`, else None.
pub fn run_capture<I, S>(program: &Path, args: I, timeout: Duration) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let group = platform::KillGroup::new().ok()?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(platform::CREATE_NO_WINDOW)
        .spawn()
        .ok()?;
    let _ = group.assign(&child);
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let start = Instant::now();
    let status = loop {
        if let Ok(Some(st)) = child.try_wait() {
            break Some(st);
        }
        if start.elapsed() > timeout {
            group.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    group.kill();
    let out = reader.join().ok()?;
    match status {
        Some(st) if st.success() => Some(String::from_utf8_lossy(&out).into_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_stdout() {
        let out = run_capture(Path::new("cmd"), ["/c", "echo hello"], Duration::from_secs(10)).unwrap();
        assert_eq!(out.trim(), "hello");
    }

    #[test]
    fn times_out() {
        let t = Instant::now();
        assert!(run_capture(Path::new("ping"), ["-n", "20", "127.0.0.1"], Duration::from_millis(300)).is_none());
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn nonzero_exit_is_none() {
        assert!(run_capture(Path::new("cmd"), ["/c", "exit 2"], Duration::from_secs(10)).is_none());
    }
}
