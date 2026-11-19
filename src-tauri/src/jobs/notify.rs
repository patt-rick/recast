//! Done/failed notifications; one combined notification for batches of 3+ (spec §4.3).
use std::collections::HashMap;

use super::{ErrorCode, JobEvent};

/// What a notification is about, so its buttons and a click can act on it (spec §4.3).
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// A finished job: "Show in folder".
    Done(String),
    /// A failed job: "Details".
    Failed(String),
    /// A job that couldn't write its output: "Save to fallback folder" and "Details".
    DestFailed(String),
    /// A job whose destination file was locked: "Retry" and "Details" (spec §7).
    Locked(String),
    /// A batch or a skip: a click opens Activity.
    Activity,
    /// An engine download that failed: "Retry".
    Engine(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub target: Option<Target>,
}

#[derive(Default)]
struct Tally {
    finished: usize,
    ok: usize,
    skipped: usize,
    failed: usize,
}

#[derive(Default)]
pub struct Notifier {
    batches: HashMap<String, Tally>,
}

/// The Activity row already shows a result while the Recast window is open and in
/// front, so a toast on top of it is noise. `on_event` still runs for every event
/// (batch tallies must stay correct); only the toast is dropped.
pub fn unless_watching(
    n: Option<Notification>,
    window_visible_and_focused: bool,
) -> Option<Notification> {
    n.filter(|_| !window_visible_and_focused)
}

fn base(p: &str) -> String {
    p.rsplit(['\\', '/']).next().unwrap_or(p).to_string()
}

impl Notifier {
    pub fn on_event(&mut self, ev: &JobEvent) -> Option<Notification> {
        let status = ev.status.as_str();
        if !matches!(status, "done" | "skipped" | "failed" | "cancelled") {
            return None;
        }
        if ev.batch_size >= 3 {
            let t = self.batches.entry(ev.batch_id.clone()).or_default();
            t.finished += 1;
            match status {
                "done" => t.ok += 1,
                "skipped" => t.skipped += 1,
                "failed" => t.failed += 1,
                _ => {}
            }
            if t.finished < ev.batch_size {
                return None;
            }
            let t = self.batches.remove(&ev.batch_id).expect("tally exists");
            if t.ok == 0 && t.failed == 0 && t.skipped == 0 {
                return None;
            }
            let mut title = if t.ok == 0 && t.failed == 0 {
                // Everything was skipped: say so once.
                return Some(Notification {
                    title: format!("Nothing to convert · {} files skipped", t.skipped),
                    body: "They're already in that format or as small as they get.".into(),
                    target: Some(Target::Activity),
                });
            } else {
                format!(
                    "{} file{} converted",
                    t.ok,
                    if t.ok == 1 { "" } else { "s" }
                )
            };
            if t.skipped > 0 {
                title.push_str(&format!(" · {} skipped", t.skipped));
            }
            if t.failed > 0 {
                title.push_str(&format!(" · {} failed", t.failed));
            }
            return Some(Notification {
                title,
                body: "Open Recast to see the results.".into(),
                target: Some(Target::Activity),
            });
        }
        let message = || {
            ev.error
                .as_ref()
                .map(|e| e.message.clone())
                .unwrap_or_default()
        };
        match status {
            "done" => Some(Notification {
                title: format!(
                    "{} ready",
                    ev.out_path.as_deref().map(base).unwrap_or_default()
                ),
                body: format!("{} · from {}", ev.preset_label, ev.file),
                target: Some(Target::Done(ev.job_id.clone())),
            }),
            "skipped" => Some(Notification {
                title: "Nothing to do".into(),
                body: message(),
                target: Some(Target::Activity),
            }),
            "failed" => Some(Notification {
                title: format!("Couldn't convert {}", ev.file),
                body: message(),
                target: Some(match ev.error.as_ref().map(|e| &e.code) {
                    Some(ErrorCode::DestUnwritable) => Target::DestFailed(ev.job_id.clone()),
                    Some(ErrorCode::DestLocked) => Target::Locked(ev.job_id.clone()),
                    _ => Target::Failed(ev.job_id.clone()),
                }),
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{ErrorCode, JobError};

    fn ev(batch: &str, size: usize, status: &str) -> JobEvent {
        JobEvent {
            job_id: "j".into(),
            batch_id: batch.into(),
            batch_size: size,
            preset_id: "video-mp3".into(),
            preset_label: "MP3".into(),
            file: "holiday.mov".into(),
            status: status.into(),
            pct: None,
            eta_s: None,
            out_path: (status == "done").then(|| r"C:\v\holiday.mp3".to_string()),
            error: (status == "failed").then(|| JobError {
                code: ErrorCode::EngineFailed,
                message: "The file looks damaged or incomplete.".into(),
            }),
        }
    }

    #[test]
    fn notifications_carry_what_their_buttons_act_on() {
        let mut n = Notifier::default();
        let mut done = ev("b", 1, "done");
        done.job_id = "j1".into();
        assert_eq!(
            n.on_event(&done).unwrap().target,
            Some(Target::Done("j1".into()))
        );
        assert_eq!(
            n.on_event(&ev("c", 1, "failed")).unwrap().target,
            Some(Target::Failed("j".into()))
        );
        let mut dest = ev("d", 1, "failed");
        dest.error = Some(JobError {
            code: ErrorCode::DestUnwritable,
            message: "x".into(),
        });
        assert_eq!(
            n.on_event(&dest).unwrap().target,
            Some(Target::DestFailed("j".into()))
        );
        let mut locked = ev("e", 1, "failed");
        locked.error = Some(JobError {
            code: ErrorCode::DestLocked,
            message: "x".into(),
        });
        assert_eq!(
            n.on_event(&locked).unwrap().target,
            Some(Target::Locked("j".into()))
        );
        for s in ["done", "done"] {
            let _ = n.on_event(&ev("f", 3, s));
        }
        assert_eq!(
            n.on_event(&ev("f", 3, "done")).unwrap().target,
            Some(Target::Activity)
        );
    }

    #[test]
    fn single_jobs() {
        let mut n = Notifier::default();
        assert_eq!(n.on_event(&ev("b", 1, "running")), None);
        assert_eq!(
            n.on_event(&ev("b", 1, "done")).unwrap().title,
            "holiday.mp3 ready"
        );
        let f = n.on_event(&ev("c", 1, "failed")).unwrap();
        assert_eq!(f.title, "Couldn't convert holiday.mov");
        assert_eq!(f.body, "The file looks damaged or incomplete.");
        assert_eq!(n.on_event(&ev("d", 1, "cancelled")), None);
    }

    #[test]
    fn no_toast_while_the_window_is_visible_and_focused() {
        let mut n = Notifier::default();
        let toast = n.on_event(&ev("b", 1, "done"));
        assert!(toast.is_some());
        assert_eq!(unless_watching(toast.clone(), true), None);
        assert_eq!(unless_watching(toast.clone(), false), toast);
        assert_eq!(unless_watching(None, false), None);
    }

    #[test]
    fn batches_of_three_or_more_notify_once() {
        let mut n = Notifier::default();
        assert_eq!(n.on_event(&ev("b", 3, "done")), None);
        assert_eq!(n.on_event(&ev("b", 3, "failed")), None);
        assert_eq!(
            n.on_event(&ev("b", 3, "done")).unwrap().title,
            "2 files converted · 1 failed"
        );
    }

    #[test]
    fn batch_toast_counts_skips_separately() {
        let mut n = Notifier::default();
        for s in ["done", "skipped", "skipped"] {
            let _ = n.on_event(&ev("b", 4, s));
        }
        assert_eq!(
            n.on_event(&ev("b", 4, "skipped")).unwrap().title,
            "1 file converted · 3 skipped"
        );
        for s in ["skipped", "skipped"] {
            assert_eq!(n.on_event(&ev("c", 3, s)), None);
        }
        assert_eq!(
            n.on_event(&ev("c", 3, "skipped")).unwrap().title,
            "Nothing to convert · 3 files skipped"
        );
        let _ = n.on_event(&ev("d", 3, "done"));
        let _ = n.on_event(&ev("d", 3, "skipped"));
        assert_eq!(
            n.on_event(&ev("d", 3, "failed")).unwrap().title,
            "1 file converted · 1 skipped · 1 failed"
        );
    }
}
