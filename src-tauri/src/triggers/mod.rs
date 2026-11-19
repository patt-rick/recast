//! The ways into the pad (spec §4.1, §6.7): edge handle, desk pad and shake.
pub mod geometry;
pub mod shake;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tauri::{AppHandle, DragDropEvent, Emitter, Manager, PhysicalPosition, PhysicalSize};

use crate::app_state::AppState;
use crate::lockutil::lock;
use crate::pad::{self, PadMode};
use crate::platform;
use crate::settings::Settings;
use geometry::{Edge, Rect};

pub const HANDLE: &str = "handle";
pub const DESK: &str = "deskpad";

#[derive(Default)]
pub struct TriggerState {
    inner: Mutex<Inner>,
    desk_save: AtomicU64,
}

#[derive(Default)]
struct Inner {
    /// What the handle window was last set to (None: never).
    handle_shown: Option<bool>,
    /// The primary monitor's foreground window is fullscreen.
    fullscreen: bool,
    hook: Option<platform::MouseHook>,
    /// The handle is enlarged with its "Drag a file here" label (first run, L9).
    highlighted: bool,
    /// The primary work area and scale the handle was last placed for (B12).
    handle_layout: Option<(Rect, f64)>,
    /// The work area and scale the desk pad was last placed for (B12).
    desk_layout: Option<(Rect, f64)>,
    /// The position `place_desk` last set, so its own `Moved` event is not saved (L8).
    desk_placed: Option<(i32, i32)>,
}

fn settings(app: &AppHandle) -> Settings {
    app.state::<AppState>().settings_now()
}

/// Startup: create the trigger state, apply the settings, start the foreground watcher.
pub fn init(app: &AppHandle) {
    app.manage(TriggerState::default());
    // The handle must be thinner than a captioned window may be (see `strip_caption`).
    if let Some(h) = app.get_webview_window(HANDLE).and_then(|w| w.hwnd().ok()) {
        platform::strip_caption(h.0 as isize);
    }
    sync(app);
    let h = app.clone();
    if let Err(e) = platform::watch_foreground(Box::new(move || tick(&h))) {
        log::warn!("triggers: foreground watcher: {e}");
    }
}

/// Apply the current trigger settings: place and show or hide the widgets; start or
/// stop the hook. Reads the settings itself, at the moment it runs, so quick successive
/// changes land in order (B11).
pub fn sync(app: &AppHandle) {
    let _ = app.run_on_main_thread({
        let app = app.clone();
        move || {
            let s = settings(&app);
            place_handle(&app, &s);
            refresh_handle(&app, &s, true);
            place_desk(&app, &s);
            set_visible(&app, DESK, s.triggers.desk_pad);
        }
    });
    set_shake(app);
}

fn rect_of(m: &tauri::Monitor) -> (Rect, f64) {
    let wa = m.work_area();
    (
        Rect {
            x: wa.position.x,
            y: wa.position.y,
            w: wa.size.width as i32,
            h: wa.size.height as i32,
        },
        m.scale_factor(),
    )
}

fn primary_work(app: &AppHandle) -> Option<(Rect, f64)> {
    let m = app.primary_monitor().ok()??;
    Some(rect_of(&m))
}

/// The work area the desk pad belongs on: the monitor holding the saved spot, else the
/// primary (B9). "Unset" is exactly (-1, -1).
fn desk_work(app: &AppHandle, s: &Settings) -> Option<(Rect, f64)> {
    let (sx, sy) = (s.triggers.desk_x, s.triggers.desk_y);
    let saved = if sx == -1.0 && sy == -1.0 {
        None
    } else {
        app.monitor_from_point(sx, sy).ok().flatten()
    };
    saved.map(|m| rect_of(&m)).or_else(|| primary_work(app))
}

fn place_handle(app: &AppHandle, s: &Settings) {
    static LOGGED: AtomicBool = AtomicBool::new(false);
    let (Some(w), Some((work, scale))) = (app.get_webview_window(HANDLE), primary_work(app)) else {
        return;
    };
    let st = app.state::<TriggerState>();
    let grown = {
        let mut i = lock(&st.inner);
        i.handle_layout = Some((work, scale));
        i.highlighted
    };
    let edge = Edge::parse(&s.triggers.edge);
    let r = if grown {
        geometry::handle_rect_highlighted(edge, s.triggers.edge_pos, work, scale)
    } else {
        geometry::handle_rect(edge, s.triggers.edge_pos, work, scale)
    };
    let _ = w.set_position(PhysicalPosition::new(r.x, r.y));
    let _ = w.set_size(PhysicalSize::new(r.w as u32, r.h as u32));
    if !LOGGED.swap(true, Ordering::Relaxed) {
        log::info!(
            "triggers: handle placed at ({}, {}) asked {}x{}, window is {:?} at scale {scale}",
            r.x,
            r.y,
            r.w,
            r.h,
            w.outer_size().ok().map(|s| (s.width, s.height)),
        );
    }
}

fn place_desk(app: &AppHandle, s: &Settings) {
    let Some(w) = app.get_webview_window(DESK) else {
        return;
    };
    let Some((work, scale)) = desk_work(app, s) else {
        return;
    };
    let (x, y) = geometry::desk_position(s.triggers.desk_x, s.triggers.desk_y, work, scale);
    let size = (geometry::DESK_SIZE * scale).round() as u32;
    {
        let st = app.state::<TriggerState>();
        let mut i = lock(&st.inner);
        i.desk_layout = Some((work, scale));
        i.desk_placed = Some((x, y));
    }
    let _ = w.set_position(PhysicalPosition::new(x, y));
    let _ = w.set_size(PhysicalSize::new(size, size));
}

/// Show without activating, or hide, a widget window. Like the pad (§6.7), shown behind
/// tao's back, so it is hidden through the OS as well.
fn set_visible(app: &AppHandle, label: &str, on: bool) {
    let Some(w) = app.get_webview_window(label) else {
        return;
    };
    let Ok(h) = w.hwnd() else { return };
    let h = h.0 as isize;
    if on {
        platform::show_no_activate(h);
    } else {
        let _ = w.hide();
        platform::hide_window(h);
    }
}

/// Handle visible = setting on and no fullscreen app in front. `force` re-applies.
fn refresh_handle(app: &AppHandle, s: &Settings, force: bool) {
    let st = app.state::<TriggerState>();
    let mut i = lock(&st.inner);
    let want = s.triggers.edge_handle && !i.fullscreen;
    if force || i.handle_shown != Some(want) {
        i.handle_shown = Some(want);
        drop(i);
        set_visible(app, HANDLE, want);
    }
}

fn fullscreen_on(primary_only: bool) -> bool {
    platform::foreground_info().is_some_and(|f| {
        (!primary_only || f.primary)
            && geometry::is_fullscreen(
                Rect::ltrb(f.window[0], f.window[1], f.window[2], f.window[3]),
                Rect::ltrb(f.monitor[0], f.monitor[1], f.monitor[2], f.monitor[3]),
                &f.class,
                f.zoomed,
            )
    })
}

/// Foreground change or the 1 s timer (watcher thread): read the foreground window here,
/// then do the rest on the main thread: auto-hide the handle over fullscreen apps, and
/// re-place the widgets when the screen layout or scale changed (B12).
fn tick(app: &AppHandle) {
    let full = fullscreen_on(true);
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let app = app2;
        let s = settings(&app);
        let (hide_changed, handle_stale, desk_stale) = {
            let st = app.state::<TriggerState>();
            let mut i = lock(&st.inner);
            let changed = i.fullscreen != full;
            i.fullscreen = full;
            (
                changed,
                primary_work(&app).is_some_and(|l| i.handle_layout != Some(l)),
                s.triggers.desk_pad
                    && desk_work(&app, &s).is_some_and(|l| i.desk_layout != Some(l)),
            )
        };
        if handle_stale {
            log::info!("triggers: screen layout changed; re-placing the handle");
            place_handle(&app, &s);
        }
        if desk_stale {
            log::info!("triggers: screen layout changed; re-placing the desk pad");
            place_desk(&app, &s);
        }
        if hide_changed {
            refresh_handle(&app, &s, false);
        }
    });
}

/// Start or stop the mouse hook to match the current setting. The setting is read under
/// the state lock, so concurrent calls end in the right state (B11).
fn set_shake(app: &AppHandle) {
    let st = app.state::<TriggerState>();
    let stopped = {
        let mut i = lock(&st.inner);
        let on = settings(app).triggers.shake;
        if on == i.hook.is_some() {
            return;
        }
        if !on {
            i.hook.take()
        } else {
            let app = app.clone();
            let mut det = shake::Detector::default();
            let hook = platform::MouseHook::start(Box::new(move |e| {
                let input = match e.input {
                    platform::MouseInput::LeftDown => shake::Input::Down,
                    platform::MouseInput::LeftUp => shake::Input::Up,
                    platform::MouseInput::Move => shake::Input::Move,
                };
                let sample = shake::Sample {
                    t_ms: e.t_ms as u64,
                    x: e.x,
                    y: e.y,
                    input,
                };
                if det.feed(sample) == shake::Shake::Armed {
                    on_shake(&app);
                }
            }));
            match hook {
                Ok(h) => i.hook = Some(h),
                Err(e) => log::warn!("shake: mouse hook failed: {e}"),
            }
            None
        }
    };
    // Dropping unhooks and joins the hook and worker threads: not under the lock.
    drop(stopped);
}

/// Runs on the hook's worker thread, never in the hook callback itself.
fn on_shake(app: &AppHandle) {
    // A shake while a pad is still waiting for its drop is the same drag; any other open
    // pad is replaced by the probe (B14, spec §4.3).
    if pad::is_waiting_for_drop(app) || fullscreen_on(false) {
        return;
    }
    log::info!("shake: armed");
    pad::open_probe(app);
}

/// Where the pad goes for a drag over a widget: beside the handle, over the desk pad.
fn anchor_for(w: &tauri::Window) -> Option<(i32, i32)> {
    let (pos, size, scale) = (
        w.outer_position().ok()?,
        w.outer_size().ok()?,
        w.scale_factor().ok()?,
    );
    let r = Rect {
        x: pos.x,
        y: pos.y,
        w: size.width as i32,
        h: size.height as i32,
    };
    if w.label() == HANDLE {
        let edge = Edge::parse(&settings(w.app_handle()).triggers.edge);
        let pad_px = (pad::SIZE * scale).round() as i32;
        Some(geometry::pad_anchor_beside(edge, r, pad_px, scale))
    } else {
        Some(r.centre())
    }
}

fn strings(paths: &[std::path::PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// Drag-drop on the handle or desk pad (spec §4.1): entering opens a drag-mode pad for
/// these files; dropping on the widget itself opens a click-mode pad.
pub fn on_drag(w: &tauri::Window, ev: &DragDropEvent) {
    let app = w.app_handle();
    let label = w.label().to_string();
    match ev {
        DragDropEvent::Enter { paths, .. } => {
            let paths = strings(paths);
            if paths.is_empty() {
                return; // text, links, virtual items (§4.1)
            }
            let _ = app.emit_to(&label, "widget://hover", true);
            if !pad::is_open_with(app, &paths) {
                pad::open_at(app, paths, PadMode::Drag, anchor_for(w));
            }
        }
        DragDropEvent::Drop { paths, .. } => {
            let _ = app.emit_to(&label, "widget://hover", false);
            let paths = strings(paths);
            if !paths.is_empty() {
                pad::open_at(app, paths, PadMode::Click, anchor_for(w));
            }
        }
        DragDropEvent::Leave => {
            let _ = app.emit_to(&label, "widget://hover", false);
        }
        _ => {}
    }
}

/// A `Moved` event that merely reports where `place_desk` put the window is not the
/// user dragging it (L8).
fn is_programmatic_move(placed: Option<(i32, i32)>, pos: (i32, i32)) -> bool {
    placed == Some(pos)
}

/// The desk pad was dragged: remember where, 500 ms after it stops.
pub fn desk_moved(app: &AppHandle, pos: PhysicalPosition<i32>) {
    let st = app.state::<TriggerState>();
    if is_programmatic_move(lock(&st.inner).desk_placed, (pos.x, pos.y)) {
        return;
    }
    let mine = st.desk_save.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        if app.state::<TriggerState>().desk_save.load(Ordering::SeqCst) != mine {
            return;
        }
        let s = settings(&app);
        if (s.triggers.desk_x - pos.x as f64).abs() < 1.0
            && (s.triggers.desk_y - pos.y as f64).abs() < 1.0
        {
            return;
        }
        let patch =
            serde_json::json!({ "triggers": { "desk_x": pos.x as f64, "desk_y": pos.y as f64 } });
        if let Err(e) = app.state::<AppState>().update_settings(patch) {
            log::warn!("desk pad: saving its position: {e}");
        }
    });
}

/// First run (§4.6): make the handle pulse while the welcome points at it. The handle
/// window grows to show a "Drag a file here" label; turning it off puts the handle back (L9).
pub fn highlight(app: &AppHandle, on: bool) {
    let Some(st) = app.try_state::<TriggerState>() else {
        return;
    };
    lock(&st.inner).highlighted = on;
    let edge = settings(app).triggers.edge;
    let _ = app.emit_to(
        HANDLE,
        "widget://highlight",
        serde_json::json!({ "on": on, "edge": edge }),
    );
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || place_handle(&app2, &settings(&app2)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moved_event_at_the_placed_spot_is_not_a_user_drag() {
        assert!(is_programmatic_move(Some((10, 20)), (10, 20)));
        assert!(!is_programmatic_move(Some((10, 20)), (11, 20)));
        assert!(!is_programmatic_move(None, (10, 20)));
    }
}
