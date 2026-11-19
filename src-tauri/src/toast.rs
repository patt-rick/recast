//! Notifications with buttons (spec §4.3, §5.7, §7). The pure part maps a target and an
//! action to what Recast does; `init` and `show` wire it to tauri-plugin-notification.
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::{Action, ActionType, NotificationExt};
use tauri_plugin_opener::OpenerExt;

use crate::app_state::AppState;
pub use crate::jobs::notify::Target;
use crate::lockutil::lock;

pub const DONE: &str = "recast-done";
pub const FAILED: &str = "recast-failed";
pub const DEST: &str = "recast-dest";
pub const LOCKED: &str = "recast-locked";
pub const ENGINE: &str = "recast-engine";

pub fn action_type(t: &Target) -> Option<&'static str> {
    match t {
        Target::Done(_) => Some(DONE),
        Target::Failed(_) => Some(FAILED),
        Target::DestFailed(_) => Some(DEST),
        Target::Locked(_) => Some(LOCKED),
        Target::Engine(_) => Some(ENGINE),
        Target::Activity => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    /// Open the companion at a section, optionally expanding one job.
    Navigate {
        section: &'static str,
        job_id: Option<String>,
    },
    Reveal(String),
    Fallback(String),
    Retry(String),
    InstallEngine(String),
}

/// `action` is the button id, or `tap` for a click on the notification itself.
pub fn resolve(action: &str, t: &Target) -> Option<Act> {
    let nav = |section, job: Option<&String>| Act::Navigate {
        section,
        job_id: job.cloned(),
    };
    match (action, t) {
        ("show", Target::Done(id)) => Some(Act::Reveal(id.clone())),
        ("fallback", Target::DestFailed(id)) => Some(Act::Fallback(id.clone())),
        ("retry", Target::Locked(id)) => Some(Act::Retry(id.clone())),
        ("retry", Target::Engine(id)) => Some(Act::InstallEngine(id.clone())),
        ("details" | "tap", Target::Failed(id) | Target::DestFailed(id) | Target::Locked(id)) => {
            Some(nav("activity", Some(id)))
        }
        ("tap", Target::Engine(_)) => Some(nav("library", None)),
        ("tap", Target::Done(id)) => Some(nav("activity", Some(id))),
        ("tap", Target::Activity) => Some(nav("activity", None)),
        _ => None,
    }
}

const KEEP: usize = 200;

/// Notification id → target, for the last 200 notifications.
pub struct Targets {
    next: i32,
    order: VecDeque<i32>,
    map: HashMap<i32, Target>,
}

impl Default for Targets {
    fn default() -> Self {
        Self {
            next: 1,
            order: VecDeque::new(),
            map: HashMap::new(),
        }
    }
}

impl Targets {
    pub fn add(&mut self, t: Target) -> i32 {
        let id = self.next;
        self.next = self.next.checked_add(1).unwrap_or(1);
        self.map.insert(id, t);
        self.order.push_back(id);
        while self.order.len() > KEEP {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        id
    }

    pub fn get(&self, id: i32) -> Option<&Target> {
        self.map.get(&id)
    }
}

pub struct Toasts(Mutex<Targets>);

/// Register the button sets and the action handler. Call in setup, before any
/// notification: the plugin shows buttons only once a handler exists.
pub fn init(app: &AppHandle) {
    app.manage(Toasts(Mutex::new(Targets::default())));
    let a = |id: &str, title: &str| Action::builder(id, title).build();
    let t = |id: &str, actions: Vec<Action>| ActionType::builder(id).actions(actions).build();
    let n = app.notification();
    if let Err(e) = n.register_action_types(vec![
        t(DONE, vec![a("show", "Show in folder")]),
        t(FAILED, vec![a("details", "Details")]),
        t(
            DEST,
            vec![
                a("fallback", "Save to fallback folder"),
                a("details", "Details"),
            ],
        ),
        t(LOCKED, vec![a("retry", "Retry"), a("details", "Details")]),
        t(ENGINE, vec![a("retry", "Retry")]),
    ]) {
        log::warn!("notifications: registering buttons: {e}");
    }
    let h = app.clone();
    let _ = n.on_action(move |p| {
        let Some(id) = p.notification().map(|n| n.id()) else {
            return;
        };
        let target = lock(&h.state::<Toasts>().0).get(id).cloned();
        if let Some(act) = target.and_then(|t| resolve(p.action_id(), &t)) {
            perform(&h, act);
        }
    });
}

/// Show a notification; `target` decides its buttons and what a click does.
pub fn show(app: &AppHandle, title: &str, body: &str, target: Option<Target>) {
    let mut b = app.notification().builder().title(title).body(body);
    if let Some(t) = target {
        if let Some(at) = action_type(&t) {
            b = b.action_type_id(at);
        }
        if let Some(st) = app.try_state::<Toasts>() {
            b = b.id(lock(&st.0).add(t));
        }
    }
    if let Err(e) = b.show() {
        log::warn!("notification: {e}");
    }
}

/// The plain toast for Retry on an engine a conversion is using.
fn busy_title(name: &str) -> String {
    format!("{name} is in use. Try again when the conversion finishes.")
}

#[derive(serde::Serialize, Clone)]
struct Navigate {
    section: &'static str,
    job_id: Option<String>,
}

fn perform(app: &AppHandle, act: Act) {
    match act {
        Act::Navigate { section, job_id } => {
            crate::tray::show_companion(app);
            let _ = app.emit_to(
                "companion",
                "companion://navigate",
                Navigate { section, job_id },
            );
        }
        Act::Reveal(id) => {
            // The history guard drops at the end of this statement, before Explorer opens.
            let out = {
                let st = app.state::<AppState>();
                crate::ipc::job_output(&lock(&st.history), &id)
            };
            match out {
                Ok(out) => {
                    let app = app.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = app.opener().reveal_item_in_dir(out) {
                            log::warn!("show in folder: {e}");
                        }
                    });
                }
                Err(e) => log::warn!("show in folder: {e}"),
            }
        }
        Act::Fallback(id) => {
            let jobs = app.state::<AppState>().jobs.clone();
            std::thread::spawn(move || {
                if let Err(e) = jobs.retry(&id, true) {
                    log::warn!("save to fallback folder: {e}");
                }
            });
        }
        Act::Retry(id) => {
            let jobs = app.state::<AppState>().jobs.clone();
            std::thread::spawn(move || {
                if let Err(e) = jobs.retry(&id, false) {
                    log::warn!("retry: {e}");
                }
            });
        }
        Act::InstallEngine(id) => {
            let st = app.state::<AppState>();
            if st.engines.in_use(&id) {
                let name = st
                    .engines
                    .list()
                    .into_iter()
                    .find(|r| r.id == id)
                    .map_or_else(|| id.clone(), |r| r.name);
                show(app, &busy_title(&name), "", None);
                return;
            }
            // A retry that fails again must notify again (B15 L7).
            st.reset_engine_notice(&id);
            let engines = st.engines.clone();
            std::thread::spawn(move || {
                let _ = engines.install(&id, &mut |_| {});
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_map_to_actions() {
        let j = || "j1".to_string();
        assert_eq!(resolve("show", &Target::Done(j())), Some(Act::Reveal(j())));
        assert_eq!(
            resolve("fallback", &Target::DestFailed(j())),
            Some(Act::Fallback(j()))
        );
        assert_eq!(
            resolve("retry", &Target::Engine("ffmpeg".into())),
            Some(Act::InstallEngine("ffmpeg".into()))
        );
        assert_eq!(
            resolve("details", &Target::Failed(j())),
            Some(Act::Navigate {
                section: "activity",
                job_id: Some(j())
            })
        );
        assert_eq!(
            resolve("tap", &Target::Engine("x".into())),
            Some(Act::Navigate {
                section: "library",
                job_id: None
            })
        );
        assert_eq!(
            resolve("show", &Target::Failed(j())),
            None,
            "no such button"
        );
        assert_eq!(
            resolve("retry", &Target::Locked(j())),
            Some(Act::Retry(j()))
        );
        assert_eq!(resolve("fallback", &Target::Locked(j())), None);
        assert_eq!(
            resolve("details", &Target::Locked(j())),
            Some(Act::Navigate {
                section: "activity",
                job_id: Some(j())
            })
        );
        assert_eq!(action_type(&Target::Locked(j())), Some(LOCKED));
        assert_eq!(
            busy_title("FFmpeg"),
            "FFmpeg is in use. Try again when the conversion finishes."
        );
        assert_eq!(action_type(&Target::Activity), None);
        assert_eq!(action_type(&Target::DestFailed(j())), Some(DEST));
    }

    #[test]
    fn targets_keep_the_last_200() {
        let mut t = Targets::default();
        let first = t.add(Target::Activity);
        for _ in 0..200 {
            t.add(Target::Activity);
        }
        assert!(t.get(first).is_none());
        assert!(t.get(first + 200).is_some());
    }
}
