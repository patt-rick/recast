//! The ways into the pad (spec §4.1, §6.7): edge handle, desk pad and shake.
pub mod geometry;
pub mod shake;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

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
    desk_save: Mutex<DeskSave>,
}

/// The debounced save of the desk pad's dragged position: one thread waits, however many
/// `Moved` events arrive (same pattern as `shell::forward`).
#[derive(Default)]
struct DeskSave {
    waiting: bool,
    due: Option<Instant>,
    pos: (i32, i32),
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
    /// The taskbar theme the tray glyph was last set for (None: not read yet).
    light_taskbar: Option<bool>,
}

fn settings(app: &AppHandle) -> Settings {
    app.state::<AppState>().settings_now()
}

/// Startup: create the trigger state, apply the settings, start the foreground watcher.
pub fn init(app: &AppHandle) {
    app.manage(TriggerState::default());
    // The handle must be thinner than a captioned window may be (see `strip_caption`).
    for label in [HANDLE, DESK] {
        if let Some(h) = app.get_webview_window(label).and_then(|w| w.hwnd().ok()) {
            platform::strip_caption(h.0 as isize);
        }
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

fn abe(edge: Edge) -> u32 {
    match edge {
        Edge::Left => 0,
        Edge::Top => 1,
        Edge::Right => 2,
        Edge::Bottom => 3,
    }
}

/// Stand the handle off `edge` when an auto-hide taskbar lives there (it would otherwise
/// sit on the bar's reveal strip).
fn handle_inset(app: &AppHandle, edge: Edge) -> i32 {
    let Some(m) = app.primary_monitor().ok().flatten() else {
        return 0;
    };
    let (p, z) = (m.position(), m.size());
    let ltrb = [p.x, p.y, p.x + z.width as i32, p.y + z.height as i32];
    if platform::autohide_bar_on(abe(edge), ltrb) {
        geometry::AUTOHIDE_INSET
    } else {
        0
    }
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
    let inset = handle_inset(app, edge);
    let r = geometry::handle_window(edge, s.triggers.edge_pos, work, scale, grown, inset);
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
    static LOGGED: AtomicBool = AtomicBool::new(false);
    if !LOGGED.swap(true, Ordering::Relaxed) {
        log::info!(
            "triggers: desk pad placed at ({x}, {y}) asked {size}x{size}, window is {:?} at scale {scale}",
            w.outer_size().ok().map(|s| (s.width, s.height)),
        );
    }
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

/// A widget needs re-placing: it is in use, and the work area or scale it was placed
/// for differs from the current one (B12). `now` None (no monitor info) changes nothing.
fn layout_changed(
    enabled: bool,
    placed_for: Option<(Rect, f64)>,
    now: Option<(Rect, f64)>,
) -> bool {
    enabled && now.is_some_and(|l| placed_for != Some(l))
}

/// Foreground change or the 1 s timer (watcher thread): read the foreground window here,
/// follow the taskbar theme, then do the rest on the main thread: auto-hide the handle over fullscreen apps, and
/// re-place the widgets when the screen layout or scale changed (B12).
fn tick(app: &AppHandle) {
    let full = fullscreen_on(true);
    // The registry read stays on the watcher thread; only the icon swap needs the main one.
    let light = platform::taskbar_is_light();
    let theme_changed = {
        let st = app.state::<TriggerState>();
        let mut i = lock(&st.inner);
        let changed = i.light_taskbar.is_some_and(|l| l != light);
        i.light_taskbar = Some(light);
        changed
    };
    if theme_changed {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || crate::tray::set_theme(&app2, light));
    }
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
                layout_changed(true, i.handle_layout, primary_work(&app)),
                layout_changed(s.triggers.desk_pad, i.desk_layout, desk_work(&app, &s)),
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
    {
        let mut i = lock(&st.inner);
        let on = settings(app).triggers.shake;
        if on == i.hook.is_some() {
            return;
        }
        if !on {
            // Dropping unhooks and joins the hook and worker threads. Under the lock on
            // purpose: a concurrent start must not begin until the old hook is gone.
            // Nothing on the hook or worker path takes this lock, and the worker only
            // queues work for the main thread, so the join cannot deadlock.
            i.hook = None;
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
        }
    }
}

/// Runs on the hook's worker thread, never in the hook callback itself.
fn on_shake(app: &AppHandle) {
    // A shake while a pad is still waiting for its drop is the same drag; any other open
    // pad is replaced by the probe (B14, spec §4.3).
    // The button went up while the event was queued: not a drag any more.
    if !platform::any_button_down() {
        return;
    }
    if pad::is_waiting_for_drop(app) || fullscreen_on(false) {
        return;
    }
    log::info!("shake: armed");
    pad::open_probe(app);
}

/// Where the pad goes for a drag over a widget: beside the handle's tab (not its grown
/// window), centred on the desk pad.
fn anchor_for(w: &tauri::Window) -> Option<(i32, i32)> {
    let app = w.app_handle();
    let scale = w.scale_factor().ok()?;
    if w.label() == HANDLE {
        let (work, _) = primary_work(app)?;
        let s = settings(app);
        let edge = Edge::parse(&s.triggers.edge);
        let pad_px = (pad::SIZE * scale).round() as i32;
        Some(geometry::handle_pad_anchor(
            edge,
            s.triggers.edge_pos,
            work,
            scale,
            handle_inset(app, edge),
            pad_px,
        ))
    } else {
        let (pos, size) = (w.outer_position().ok()?, w.outer_size().ok()?);
        Some(
            Rect {
                x: pos.x,
                y: pos.y,
                w: size.width as i32,
                h: size.height as i32,
            }
            .centre(),
        )
    }
}

fn strings(paths: &[std::path::PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// The mode of the pad a drag event should open, or None for no pad: entering opens a
/// drag-mode pad unless one is already open for these files; dropping opens click mode.
/// Text, links and virtual items carry no paths and open nothing (§4.1).
fn drag_pad_mode(entering: bool, paths: &[String], already_open: bool) -> Option<PadMode> {
    if paths.is_empty() {
        None
    } else if entering {
        (!already_open).then_some(PadMode::Drag)
    } else {
        Some(PadMode::Click)
    }
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
            let open = pad::is_open_with(app, &paths);
            if let Some(mode) = drag_pad_mode(true, &paths, open) {
                pad::open_at(app, paths, mode, anchor_for(w));
            }
        }
        DragDropEvent::Drop { paths, .. } => {
            let _ = app.emit_to(&label, "widget://hover", false);
            let paths = strings(paths);
            if let Some(mode) = drag_pad_mode(false, &paths, false) {
                pad::open_at(app, paths, mode, anchor_for(w));
            }
        }
        DragDropEvent::Leave => {
            let _ = app.emit_to(&label, "widget://hover", false);
        }
        _ => {}
    }
}

/// A `Moved` event is the user dragging the desk pad only if it is not the position
/// `place_desk` just set (L8) and a mouse button is down; Windows moving the window itself
/// (monitor unplugged, DPI change) must not be saved.
fn is_user_drag(placed: Option<(i32, i32)>, pos: (i32, i32), button_down: bool) -> bool {
    button_down && placed != Some(pos)
}

/// The desk pad was dragged: remember where, 500 ms after it stops.
pub fn desk_moved(app: &AppHandle, pos: PhysicalPosition<i32>) {
    let st = app.state::<TriggerState>();
    if !is_user_drag(
        lock(&st.inner).desk_placed,
        (pos.x, pos.y),
        platform::any_button_down(),
    ) {
        return;
    }
    let mut g = lock(&st.desk_save);
    g.pos = (pos.x, pos.y);
    g.due = Some(Instant::now() + Duration::from_millis(500));
    if g.waiting {
        return; // the waiting thread will see the new deadline
    }
    g.waiting = true;
    drop(g);
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            let st = app.state::<TriggerState>();
            let mut g = lock(&st.desk_save);
            let now = Instant::now();
            match g.due {
                Some(d) if d > now => {
                    drop(g);
                    std::thread::sleep(d - now);
                }
                Some(_) => {
                    let pos = g.pos;
                    g.due = None;
                    g.waiting = false;
                    drop(g);
                    save_desk(&app, pos);
                    return;
                }
                None => {
                    g.waiting = false;
                    return;
                }
            }
        }
    });
}

fn save_desk(app: &AppHandle, pos: (i32, i32)) {
    let s = settings(app);
    if (s.triggers.desk_x - pos.0 as f64).abs() < 1.0
        && (s.triggers.desk_y - pos.1 as f64).abs() < 1.0
    {
        return;
    }
    let patch =
        serde_json::json!({ "triggers": { "desk_x": pos.0 as f64, "desk_y": pos.1 as f64 } });
    if let Err(e) = app.state::<AppState>().update_settings(patch) {
        log::warn!("desk pad: saving its position: {e}");
    }
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
    fn only_a_drag_with_a_button_down_away_from_the_placed_spot_is_saved() {
        assert!(is_user_drag(Some((10, 20)), (11, 20), true));
        assert!(is_user_drag(None, (10, 20), true));
        // place_desk's own Moved event.
        assert!(!is_user_drag(Some((10, 20)), (10, 20), true));
        // Windows moved it (monitor unplugged, DPI): no button is down.
        assert!(!is_user_drag(Some((10, 20)), (500, 20), false));
    }

    fn rect(w: i32) -> (Rect, f64) {
        (
            Rect {
                x: 0,
                y: 0,
                w,
                h: 1040,
            },
            1.0,
        )
    }

    #[test]
    fn a_widget_is_re_placed_only_when_in_use_and_the_layout_changed() {
        assert!(layout_changed(true, None, Some(rect(1920))));
        assert!(layout_changed(true, Some(rect(1920)), Some(rect(2560))));
        assert!(layout_changed(
            true,
            Some((rect(1920).0, 1.0)),
            Some((rect(1920).0, 1.5))
        ));
        assert!(!layout_changed(true, Some(rect(1920)), Some(rect(1920))));
        assert!(!layout_changed(true, Some(rect(1920)), None));
        assert!(!layout_changed(false, None, Some(rect(1920))));
    }

    #[test]
    fn drag_events_decide_the_pad_mode() {
        let one = vec!["a".to_string()];
        assert_eq!(drag_pad_mode(true, &one, false), Some(PadMode::Drag));
        assert_eq!(drag_pad_mode(true, &one, true), None);
        assert_eq!(drag_pad_mode(true, &[], false), None);
        assert_eq!(drag_pad_mode(false, &one, false), Some(PadMode::Click));
        // A drop re-opens in click mode even if a drag pad for the files is open.
        assert_eq!(drag_pad_mode(false, &one, true), Some(PadMode::Click));
        assert_eq!(drag_pad_mode(false, &[], false), None);
    }
}
