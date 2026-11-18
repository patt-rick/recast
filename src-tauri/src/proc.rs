//! Short helper processes with a timeout (never a shell).
use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::platform;

/// What a finished helper process produced. `status` is None when it timed out.
pub struct Captured {
    pub status: Option<ExitStatus>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Join `h` if it finishes before `deadline`; otherwise leave it running (detached).
pub fn join_until<T>(h: JoinHandle<T>, deadline: Instant) -> Option<T> {
    while !h.is_finished() {
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    h.join().ok()
}

fn drain<R: Read + Send + 'static>(r: Option<R>) -> Option<JoinHandle<Vec<u8>>> {
    let mut r = r?;
    Some(std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = r.read_to_end(&mut buf);
        buf
    }))
}

/// Run `program args…` inside a kill group with extra environment variables,
/// draining both pipes; the whole tree is killed after `timeout`.
pub fn run_full<I, S>(
    program: &Path,
    args: I,
    env: &[(OsString, OsString)],
    timeout: Duration,
) -> io::Result<Captured>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let group = platform::KillGroup::new()?;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = platform::spawn_contained(&mut cmd, &group)?;
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let start = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait()? {
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
    let deadline = Instant::now() + Duration::from_secs(2);
    let stdout = out
        .and_then(|h| join_until(h, deadline))
        .unwrap_or_default();
    let stderr = err
        .and_then(|h| join_until(h, deadline))
        .unwrap_or_default();
    Ok(Captured {
        status,
        stdout,
        stderr,
    })
}

/// Run `program args…`; return stdout if it exits 0 within `timeout`, else None.
pub fn run_capture<I, S>(program: &Path, args: I, timeout: Duration) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_capture_env(program, args, &[], timeout)
}

/// `run_capture` with extra environment variables.
pub fn run_capture_env<I, S>(
    program: &Path,
    args: I,
    env: &[(OsString, OsString)],
    timeout: Duration,
) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let c = run_full(program, args, env, timeout).ok()?;
    match c.status {
        Some(st) if st.success() => Some(String::from_utf8_lossy(&c.stdout).into_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_stdout() {
        let out = run_capture(
            Path::new("cmd"),
            ["/c", "echo hello"],
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(out.trim(), "hello");
    }

    #[test]
    fn passes_environment() {
        let env = [(OsString::from("RECAST_TEST_VAR"), OsString::from("xyz"))];
        let out = run_capture_env(
            Path::new("cmd"),
            ["/c", "echo %RECAST_TEST_VAR%"],
            &env,
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(out.trim(), "xyz");
    }

    #[test]
    fn times_out() {
        let t = Instant::now();
        assert!(
            run_capture(
                Path::new("ping"),
                ["-n", "20", "127.0.0.1"],
                Duration::from_millis(300)
            )
            .is_none()
        );
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn nonzero_exit_is_none() {
        assert!(run_capture(Path::new("cmd"), ["/c", "exit 2"], Duration::from_secs(10)).is_none());
    }

    #[test]
    fn captures_stderr_and_status() {
        let c = run_full(
            Path::new("cmd"),
            ["/c", "echo oops 1>&2 & exit 4"],
            &[],
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(c.status.unwrap().code(), Some(4));
        assert!(String::from_utf8_lossy(&c.stderr).contains("oops"));
    }
}
