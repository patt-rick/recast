//! File drags onto Recast's windows.
//!
//! wry delivers file drags only for the WebView2 child windows that existed when each
//! webview was created. The transparent windows (pad, handle, desk pad) get their input
//! window later, so without help no drag ever reaches them (`platform::cover_drop_targets`
//! explains). `cover` registers Recast's own target on such windows; its events come back
//! here and are turned into exactly what wry's would have produced: the window event that
//! `dispatch` handles, and the `tauri://drag-*` events the pages listen to.
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, DragDropEvent, Emitter, EventTarget, Manager, PhysicalPosition};

use crate::lockutil::lock;
use crate::platform::{self, DropEvent};
use crate::{pad, triggers};

/// The windows covered. Not the companion: wry covers its input windows (they exist when
/// its webview is created), and mixing wry's events with these in one window could
/// deliver an Enter and a Leave out of order.
const LABELS: [&str; 3] = [pad::LABEL, triggers::HANDLE, triggers::DESK];

/// Events from the drop targets, in the order OLE raised them, to the forwarder thread.
static EVENTS: OnceLock<Mutex<mpsc::Sender<(String, DropEvent)>>> = OnceLock::new();

/// A drag event for a window, from wry or from Recast's own targets: what Recast does
/// with it. Runs on the main thread.
pub fn dispatch(window: &tauri::Window, ev: &DragDropEvent) {
    match ev {
        // A real file drag reaching the pad confirms a shake probe; a drop ends the wait
        // for one (B1). Both arrive in event order.
        _ if matches!(window.label(), triggers::HANDLE | triggers::DESK) => {
            triggers::on_drag(window, ev)
        }
        DragDropEvent::Enter { paths, .. } if window.label() == pad::LABEL && !paths.is_empty() => {
            pad::drag_entered(window.app_handle())
        }
        DragDropEvent::Drop { .. } if window.label() == pad::LABEL => {
            pad::dropped(window.app_handle())
        }
        _ => {}
    }
}

/// Start the forwarder. Call once in setup, before the first `cover`.
///
/// The targets' COM callbacks run on the main thread in the middle of the drag, and from
/// the main thread `run_on_main_thread` runs its task at once. So, like wry, a target only
/// posts its event; this thread hands each one back to the main thread as a separate task,
/// in order.
pub fn init(app: &AppHandle) {
    let (tx, rx) = mpsc::channel::<(String, DropEvent)>();
    if EVENTS.set(Mutex::new(tx)).is_err() {
        return;
    }
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("recast-drops".into())
        .spawn(move || {
            for (label, ev) in rx {
                let app2 = app.clone();
                if let Err(e) = app.run_on_main_thread(move || deliver(&app2, &label, ev)) {
                    log::warn!("drops: couldn't reach the main thread: {e}");
                }
            }
        });
    if let Err(e) = spawned {
        log::warn!("drops: forwarder thread: {e}");
    }
}

#[derive(Serialize, Clone)]
struct Payload<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    paths: Option<&'a Vec<PathBuf>>,
    position: PhysicalPosition<f64>,
}

/// Convert a platform event to Tauri's (wry's values, as tauri-runtime-wry converts them).
fn to_tauri(ev: DropEvent) -> DragDropEvent {
    match ev {
        DropEvent::Enter { paths, x, y } => DragDropEvent::Enter {
            paths,
            position: PhysicalPosition::new(x, y),
        },
        DropEvent::Over { x, y } => DragDropEvent::Over {
            position: PhysicalPosition::new(x, y),
        },
        DropEvent::Drop { paths, x, y } => DragDropEvent::Drop {
            paths,
            position: PhysicalPosition::new(x, y),
        },
        DropEvent::Leave => DragDropEvent::Leave,
    }
}

/// The page event Tauri emits for a window event (tauri 2.12.7 manager/window.rs
/// `on_window_event`): name and payload.
fn page_event(ev: &DragDropEvent) -> Option<(&'static str, Option<Payload<'_>>)> {
    Some(match ev {
        DragDropEvent::Enter { paths, position } => (
            "tauri://drag-enter",
            Some(Payload {
                paths: Some(paths),
                position: *position,
            }),
        ),
        DragDropEvent::Over { position } => (
            "tauri://drag-over",
            Some(Payload {
                paths: None,
                position: *position,
            }),
        ),
        DragDropEvent::Drop { paths, position } => (
            "tauri://drag-drop",
            Some(Payload {
                paths: Some(paths),
                position: *position,
            }),
        ),
        DragDropEvent::Leave => ("tauri://drag-leave", None),
        _ => return None,
    })
}

/// Main thread: act on one event from Recast's targets as if wry had delivered it.
fn deliver(app: &AppHandle, label: &str, ev: DropEvent) {
    let Some(w) = app.get_webview_window(label) else {
        return;
    };
    let ev = to_tauri(ev);
    dispatch(&w.as_ref().window(), &ev);
    let target = EventTarget::labeled(label);
    let sent = match page_event(&ev) {
        Some((name, Some(payload))) => app.emit_to(target, name, payload),
        Some((name, None)) => app.emit_to(target, name, ()),
        None => Ok(()),
    };
    if let Err(e) = sent {
        log::warn!("drops: emitting to {label}: {e}");
    }
}

/// Main thread: cover every uncovered input window of Recast's webview windows. Cheap
/// when there is nothing new; logs what it registers.
pub fn cover(app: &AppHandle) {
    let Some(events) = EVENTS.get() else {
        return;
    };
    for label in LABELS {
        let Some(h) = app.get_webview_window(label).and_then(|w| w.hwnd().ok()) else {
            continue;
        };
        let tx = lock(events).clone();
        let owned = label.to_string();
        let sink: platform::DropSink = Rc::new(move |ev| {
            let _ = tx.send((owned.clone(), ev));
        });
        let done = platform::cover_drop_targets(h.0 as isize, &sink);
        if !done.is_empty() {
            log::info!("drops: covered {label}: {}", done.join(", "));
        }
    }
}

/// Cover at once and again shortly after: WebView2 may create a window's input window
/// only once it is shown, and a shake probe is hit within 300 ms. Call right after
/// showing a window. Every pass is its own main-thread task, never part of the caller's:
/// callers hold the pad's lock, and registering on another process's window may wait
/// on COM, which may run other main-thread tasks that take that lock.
pub fn cover_soon(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut slept = 0;
        for at in [0, 30, 100, 250] {
            std::thread::sleep(Duration::from_millis(at - slept));
            slept = at;
            let a = app.clone();
            let _ = app.run_on_main_thread(move || cover(&a));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_events_match_what_tauri_emits() {
        let p = vec![PathBuf::from(r"C:\a.png")];
        let enter = to_tauri(DropEvent::Enter {
            paths: p.clone(),
            x: 3.0,
            y: 4.0,
        });
        let (name, payload) = page_event(&enter).unwrap();
        assert_eq!(name, "tauri://drag-enter");
        assert_eq!(
            serde_json::to_value(payload.unwrap()).unwrap(),
            serde_json::json!({ "paths": [r"C:\a.png"], "position": { "x": 3.0, "y": 4.0 } })
        );
        let over = to_tauri(DropEvent::Over { x: 1.0, y: 2.0 });
        let (name, payload) = page_event(&over).unwrap();
        assert_eq!(name, "tauri://drag-over");
        assert_eq!(
            serde_json::to_value(payload.unwrap()).unwrap(),
            serde_json::json!({ "position": { "x": 1.0, "y": 2.0 } }),
            "over carries no paths, as in Tauri"
        );
        let drop = to_tauri(DropEvent::Drop {
            paths: p,
            x: 5.0,
            y: 6.0,
        });
        assert_eq!(page_event(&drop).unwrap().0, "tauri://drag-drop");
        let leave = to_tauri(DropEvent::Leave);
        let (name, payload) = page_event(&leave).unwrap();
        assert_eq!((name, payload.is_none()), ("tauri://drag-leave", true));
    }
}
