//! Run one engine process (spec §6.4): Job Object, both pipes drained, watchdog.
use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::progress::Tracker;
use crate::platform;
use crate::presets::ProgressKind;

pub const TAIL_LINES: usize = 30;

#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

pub struct RunSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub progress: ProgressKind,
    pub duration_s: Option<f64>,
    /// No forward progress for this long → Stalled (engines that report progress).
    pub stall: Duration,
    /// Total runtime cap for engines that report no progress.
    pub hard_cap: Duration,
    /// Extra environment variables (e.g. MAGICK_CONFIGURE_PATH for ImageMagick).
    pub env: Vec<(OsString, OsString)>,
}

#[derive(Debug, PartialEq)]
pub enum RunOutcome {
    Exited(i32),
    Stalled,
    Cancelled,
}

pub struct RunResult {
    pub outcome: RunOutcome,
    /// Last TAIL_LINES non-progress lines from stdout and stderr.
    pub tail: Vec<String>,
}

enum Line {
    Out(String),
    Err(String),
}

fn spawn_reader<R: Read + Send + 'static>(
    r: R,
    tx: Sender<Line>,
    wrap: fn(String) -> Line,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(r);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let s = String::from_utf8_lossy(&buf);
                    for part in s.split('\r') {
                        let t = part.trim_end_matches('\n');
                        if !t.trim().is_empty() && tx.send(wrap(t.to_string())).is_err() {
                            return;
                        }
                    }
                }
            }
        }
    })
}

fn absorb(
    line: Line,
    tracker: &mut Tracker,
    tail: &mut VecDeque<String>,
    last_advance: &mut Instant,
    on_pct: &mut dyn FnMut(f32),
) {
    let text = match line {
        Line::Out(s) => {
            let f = tracker.feed(&s);
            if f.advanced {
                *last_advance = Instant::now();
            }
            if let Some(p) = f.pct {
                on_pct(p);
            }
            if f.consumed {
                return;
            }
            s
        }
        Line::Err(s) => s,
    };
    if tail.len() == TAIL_LINES {
        tail.pop_front();
    }
    tail.push_back(text);
}

pub fn run(
    spec: &RunSpec,
    cancel: &CancelToken,
    on_pct: &mut dyn FnMut(f32),
) -> io::Result<RunResult> {
    let group = platform::KillGroup::new()?;
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in &spec.env {
        cmd.env(k, v);
    }
    // Suspended until it is in the group, so no grandchild can escape the kill.
    let mut child = platform::spawn_contained(&mut cmd, &group)?;
    let (tx, rx) = mpsc::channel();
    let readers = [
        spawn_reader(
            child.stdout.take().expect("piped stdout"),
            tx.clone(),
            Line::Out,
        ),
        spawn_reader(child.stderr.take().expect("piped stderr"), tx, Line::Err),
    ];
    let mut tracker = Tracker::new(spec.progress, spec.duration_s);
    let mut tail = VecDeque::new();
    let start = Instant::now();
    let mut last_advance = start;

    let outcome = loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => absorb(line, &mut tracker, &mut tail, &mut last_advance, on_pct),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => thread::sleep(Duration::from_millis(20)),
        }
        if cancel.is_cancelled() {
            group.kill();
            break RunOutcome::Cancelled;
        }
        let stalled = match spec.progress {
            ProgressKind::None => start.elapsed() > spec.hard_cap,
            _ if tracker.stall_armed() => last_advance.elapsed() > spec.stall,
            // before the first output / while finishing: only the hard cap applies
            _ => last_advance.elapsed() > spec.hard_cap,
        };
        if stalled {
            group.kill();
            break RunOutcome::Stalled;
        }
        if let Some(status) = child.try_wait()? {
            break RunOutcome::Exited(status.code().unwrap_or(-1));
        }
    };
    let _ = child.wait();
    group.kill(); // anything the engine left behind would hold our pipes open
    // The group is dead, so the pipes close; don't hang on a reader regardless.
    let deadline = Instant::now() + Duration::from_secs(2);
    for r in readers {
        let _ = crate::proc::join_until(r, deadline);
    }
    for line in rx.try_iter() {
        absorb(line, &mut tracker, &mut tail, &mut last_advance, on_pct);
    }
    Ok(RunResult {
        outcome,
        tail: tail.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::fake_engine;

    fn spec(
        dir: &std::path::Path,
        args: &[&str],
        progress: ProgressKind,
        stall_ms: u64,
    ) -> RunSpec {
        RunSpec {
            program: fake_engine(),
            args: args.iter().map(OsString::from).collect(),
            cwd: dir.to_path_buf(),
            progress,
            duration_s: Some(1.0),
            stall: Duration::from_millis(stall_ms),
            hard_cap: Duration::from_millis(stall_ms),
            env: vec![],
        }
    }

    #[test]
    fn success_reports_progress_and_writes_output() {
        let d = tempfile::tempdir().unwrap();
        let mut pcts = Vec::new();
        let r = run(
            &spec(d.path(), &["ok", "out.txt"], ProgressKind::Ffmpeg, 5000),
            &CancelToken::default(),
            &mut |p| pcts.push(p),
        )
        .unwrap();
        assert_eq!(r.outcome, RunOutcome::Exited(0));
        assert_eq!(pcts.last(), Some(&1.0));
        assert!(pcts.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(
            std::fs::read_to_string(d.path().join("out.txt")).unwrap(),
            "converted"
        );
        assert!(
            r.tail.is_empty(),
            "progress lines are not kept: {:?}",
            r.tail
        );
    }

    #[test]
    fn failure_keeps_stderr_tail() {
        let d = tempfile::tempdir().unwrap();
        let r = run(
            &spec(d.path(), &["fail"], ProgressKind::None, 5000),
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(r.outcome, RunOutcome::Exited(3));
        assert!(r.tail.iter().any(|l| l.contains("Invalid data")));
    }

    #[test]
    fn stall_watchdog_kills_silent_engine() {
        let d = tempfile::tempdir().unwrap();
        let t = Instant::now();
        let r = run(
            &spec(
                d.path(),
                &["hang_after_progress"],
                ProgressKind::Ffmpeg,
                500,
            ),
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(r.outcome, RunOutcome::Stalled);
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn hard_cap_applies_when_engine_reports_no_progress() {
        let d = tempfile::tempdir().unwrap();
        let r = run(
            &spec(d.path(), &["hang"], ProgressKind::None, 500),
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(r.outcome, RunOutcome::Stalled);
    }

    #[test]
    fn cancel_kills_the_whole_tree() {
        let d = tempfile::tempdir().unwrap();
        let cancel = CancelToken::default();
        let c2 = cancel.clone();
        let pid_file = d.path().join("grandchild.pid");
        let pf = pid_file.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        // Cancel only once the grandchild exists and we hold a handle to wait on it.
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                if let Some(pid) = std::fs::read_to_string(&pf)
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok())
                {
                    if let Some(w) = crate::platform::open_for_wait(pid) {
                        let _ = tx.send(w);
                    }
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            c2.cancel();
        });
        let r = run(
            &spec(
                d.path(),
                &["spawn_child", pid_file.to_str().unwrap()],
                ProgressKind::None,
                60_000,
            ),
            &cancel,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(r.outcome, RunOutcome::Cancelled);
        let waiter = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("the grandchild was never opened");
        assert!(waiter.wait(Duration::from_secs(5)), "grandchild survived");
    }

    #[test]
    fn passes_extra_environment() {
        let d = tempfile::tempdir().unwrap();
        let mut s = spec(
            d.path(),
            &["print_env", "MAGICK_CONFIGURE_PATH"],
            ProgressKind::None,
            5000,
        );
        s.env = vec![(
            OsString::from("MAGICK_CONFIGURE_PATH"),
            OsString::from(r"C:\magick"),
        )];
        let r = run(&s, &CancelToken::default(), &mut |_| {}).unwrap();
        assert_eq!(r.outcome, RunOutcome::Exited(0));
        assert_eq!(r.tail, [r"C:\magick"]);
    }

    #[test]
    fn chatty_stderr_does_not_deadlock() {
        let d = tempfile::tempdir().unwrap();
        let t = Instant::now();
        let r = run(
            &spec(d.path(), &["chatty"], ProgressKind::None, 20_000),
            &CancelToken::default(),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(r.outcome, RunOutcome::Exited(0));
        assert_eq!(r.tail.len(), TAIL_LINES);
        assert!(t.elapsed() < Duration::from_secs(15));
    }
}
