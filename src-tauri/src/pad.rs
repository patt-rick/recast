//! The pad window (spec §4.4, §6.7): created hidden at startup, then positioned,
//! shown and hidden for every open. Plan 3's triggers call `open` too.
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

use crate::lockutil::lock;
use crate::platform;

pub const LABEL: &str = "pad";
pub const SIZE: f64 = 440.0;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PadMode {
    /// Opened with files already chosen: segments are clicked; the pad takes focus.
    Click,
    /// Opened mid-drag: segments are dropped on; the pad must not take focus.
    Drag,
}

#[derive(Serialize, Clone, Debug)]
pub struct PadOpen {
    pub paths: Vec<String>,
    pub mode: PadMode,
    pub reduced_transparency: bool,
    /// Identifies this open; `pad_show` and `pad_close` from an older open are ignored.
    pub generation: u64,
}

#[derive(Serialize, Clone, Copy)]
struct Cursor {
    x: f64,
    y: f64,
}

/// Top-left of a `size`×`size` window centred on `anchor`, kept inside `work`
/// (x, y, w, h), all in physical pixels.
pub fn place(anchor: (i32, i32), size: u32, work: (i32, i32, u32, u32)) -> (i32, i32) {
    let s = size as i32;
    let axis = |a: i32, lo: i32, len: u32| {
        let hi = lo + len as i32 - s;
        if hi < lo {
            lo
        } else {
            (a - s / 2).clamp(lo, hi)
        }
    };
    (
        axis(anchor.0, work.0, work.2),
        axis(anchor.1, work.1, work.3),
    )
}

/// Everything that must change together, behind one lock.
#[derive(Default)]
struct Inner {
    /// The pad page has registered its listeners (`pad_ready`).
    ready: bool,
    /// An open requested before the page was ready.
    pending: Option<PadOpen>,
    /// Bumped by every open and every close.
    generation: u64,
}

impl Inner {
    /// Start a new open: bump the generation and either hand the request back for
    /// immediate delivery (page ready) or park it until `pad_ready`.
    fn begin(&mut self, mut req: PadOpen) -> Option<PadOpen> {
        self.generation += 1;
        req.generation = self.generation;
        if self.ready {
            self.pending = None;
            Some(req)
        } else {
            self.pending = Some(req);
            None
        }
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }

    /// Invalidate the current open (and any parked request).
    fn end(&mut self) {
        self.generation += 1;
        self.pending = None;
    }

    /// Should a close go ahead? `None` closes whatever is open; `Some(g)` only open `g`.
    /// When it should, the open is invalidated here, so check and effect are one step.
    fn begin_close(&mut self, generation: Option<u64>) -> bool {
        if generation.is_some_and(|g| !self.is_current(g)) {
            return false;
        }
        self.end();
        true
    }

    /// Should `pad_show(generation)` go ahead? Not after a newer open or a close.
    fn allows_show(&self, generation: u64) -> bool {
        self.is_current(generation)
    }
}

/// Pad window state, managed by Tauri.
#[derive(Default)]
pub struct PadState {
    inner: Mutex<Inner>,
    /// Cursor-poll epoch: `track_cursor` bumps it; a poller runs only while the
    /// counter still equals the value it started with, and never writes it.
    poll: Arc<AtomicU64>,
}

fn raw_hwnd(w: &tauri::WebviewWindow) -> Option<isize> {
    w.hwnd().ok().map(|h| h.0 as isize)
}

/// Exclude the pad from capture once, at startup.
pub fn init(app: &AppHandle) {
    app.manage(PadState::default());
    if let Some(w) = app.get_webview_window(LABEL)
        && let Some(h) = raw_hwnd(&w)
        && let Err(e) = platform::exclude_from_capture(h)
    {
        log::warn!("pad: capture exclusion failed: {e}");
    }
}

/// Run `f` on the main thread, in order with every other pad call. Triggers (Plan 3's
/// hook threads) and IPC commands call from other threads; window calls and the
/// generation bookkeeping both want one thread so they cannot interleave.
fn on_main(app: &AppHandle, f: impl FnOnce(&AppHandle) + Send + 'static) {
    let handle = app.clone();
    if let Err(e) = app.run_on_main_thread(move || f(&handle)) {
        log::warn!("pad: couldn't reach the main thread: {e}");
    }
}

/// Position the pad over the cursor and hand it these files. Drag mode shows it at
/// once (without focus) once the page is ready; click mode waits for `pad_show` once
/// the page has drawn.
pub fn open(app: &AppHandle, paths: Vec<String>, mode: PadMode) {
    on_main(app, move |app| open_now(app, paths, mode));
}

fn open_now(app: &AppHandle, paths: Vec<String>, mode: PadMode) {
    let Some(w) = app.get_webview_window(LABEL) else {
        log::warn!("pad window missing");
        return;
    };
    let cursor = match app.cursor_position() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("pad: cursor position unavailable, using the origin: {e}");
            PhysicalPosition::new(0.0, 0.0)
        }
    };
    let anchor = (cursor.x.round() as i32, cursor.y.round() as i32);
    match app.monitor_from_point(cursor.x, cursor.y) {
        Ok(Some(m)) => {
            let wa = m.work_area();
            let px = (SIZE * m.scale_factor()).round() as u32;
            let (x, y) = place(
                anchor,
                px,
                (wa.position.x, wa.position.y, wa.size.width, wa.size.height),
            );
            // Position first: the size is then interpreted at the target monitor's DPI.
            let _ = w.set_position(PhysicalPosition::new(x, y));
            let _ = w.set_size(tauri::PhysicalSize::new(px, px));
        }
        Ok(None) => log::warn!("pad: no monitor at {anchor:?}; leaving the pad where it is"),
        Err(e) => log::warn!("pad: monitor lookup failed ({e}); leaving the pad where it is"),
    }
    let req = PadOpen {
        paths,
        mode,
        reduced_transparency: !platform::transparency_enabled(),
        generation: 0,
    };
    // One guard from the generation bump to the show, so a close or newer open can't
    // slip in between. Emitting and acting under it relies on there being no Rust-side
    // listeners that take this lock.
    let st = app.state::<PadState>();
    let mut inner = lock(&st.inner);
    if let Some(req) = inner.begin(req) {
        let _ = app.emit_to(LABEL, "pad://open", &req);
        if mode == PadMode::Drag {
            show_drag(app, &w);
        }
    }
    // Page not ready: `ready` delivers the parked open and, in drag mode, shows it.
}

/// Show without taking focus (an OLE drag is in progress) and start the cursor feed.
fn show_drag(app: &AppHandle, w: &tauri::WebviewWindow) {
    match raw_hwnd(w) {
        Some(h) => platform::show_no_activate(h),
        None => {
            let _ = w.show();
        }
    }
    track_cursor(app, true);
}

/// The pad page is listening: deliver an open that arrived earlier.
pub fn ready(app: &AppHandle) {
    on_main(app, |app| {
        let st = app.state::<PadState>();
        let mut inner = lock(&st.inner);
        inner.ready = true;
        if let Some(req) = inner.pending.take() {
            let _ = app.emit_to(LABEL, "pad://open", &req);
            if req.mode == PadMode::Drag
                && let Some(w) = app.get_webview_window(LABEL)
            {
                show_drag(app, &w);
            }
        }
    });
}

/// Show and focus the pad (click mode, or a drag-mode pad after a drop). A no-op if a
/// newer open or a close has happened since.
pub fn show(app: &AppHandle, generation: u64) {
    on_main(app, move |app| {
        let st = app.state::<PadState>();
        let inner = lock(&st.inner);
        if !inner.allows_show(generation) {
            return;
        }
        if let Some(w) = app.get_webview_window(LABEL) {
            let _ = w.show();
            let _ = w.set_focus();
        }
        drop(inner);
    });
}

/// Hide the pad, whatever open it belongs to, and invalidate that open.
pub fn close(app: &AppHandle) {
    on_main(app, |app| close_now(app, None));
}

/// Hide the pad only if `generation` is still the current open.
pub fn close_if_current(app: &AppHandle, generation: u64) {
    on_main(app, move |app| close_now(app, Some(generation)));
}

fn close_now(app: &AppHandle, generation: Option<u64>) {
    let st = app.state::<PadState>();
    // Check and act under one guard (see `open_now` on acting while locked).
    let mut inner = lock(&st.inner);
    if !inner.begin_close(generation) {
        return;
    }
    track_cursor(app, false);
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
        // A drag-mode pad was shown behind tao's back (`show_no_activate`), so tao still
        // believes it is hidden and `hide()` above may do nothing. Hide it for real.
        if let Some(h) = raw_hwnd(&w) {
            platform::hide_window(h);
        }
    }
}

/// `pad_track_cursor` from the page: ignored unless `generation` is the current open,
/// so a late "off" from a dismissed pad cannot stop the feed of a newer one.
pub fn track_cursor_for(app: &AppHandle, on: bool, generation: u64) {
    let st = app.state::<PadState>();
    let inner = lock(&st.inner);
    if inner.is_current(generation) {
        track_cursor(app, on);
    }
    drop(inner);
}

/// While a drag is involved, emit the cursor relative to the pad (logical px) at 60 Hz:
/// Tauri sends no positions after a drag leaves the window (spec §4.3).
fn track_cursor(app: &AppHandle, on: bool) {
    let st = app.state::<PadState>();
    let epoch = st.poll.clone();
    // Any bump retires every running poller; only `on` starts a new one.
    let start = epoch.fetch_add(1, Ordering::SeqCst) + 1;
    if !on {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        while epoch.load(Ordering::SeqCst) == start {
            let Some(w) = app.get_webview_window(LABEL) else {
                break;
            };
            if !w.is_visible().unwrap_or(false) {
                break;
            }
            if let (Ok(c), Ok(pos), Ok(scale)) =
                (app.cursor_position(), w.outer_position(), w.scale_factor())
            {
                let p = Cursor {
                    x: (c.x - pos.x as f64) / scale,
                    y: (c.y - pos.y as f64) / scale,
                };
                let _ = app.emit_to(LABEL, "pad://cursor", p);
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centres_on_the_anchor_inside_the_work_area() {
        let work = (0, 0, 1920, 1040);
        assert_eq!(place((960, 520), 440, work), (740, 300));
        assert_eq!(place((10, 10), 440, work), (0, 0));
        assert_eq!(place((1915, 1035), 440, work), (1480, 600));
        // A second monitor to the left, at negative coordinates.
        assert_eq!(place((-100, 500), 880, (-2560, 0, 2560, 1400)), (-880, 60));
        // A work area smaller than the pad pins it to the top-left.
        assert_eq!(place((50, 50), 440, (0, 0, 300, 300)), (0, 0));
    }

    fn req() -> PadOpen {
        PadOpen {
            paths: vec![],
            mode: PadMode::Click,
            reduced_transparency: false,
            generation: 0,
        }
    }

    #[test]
    fn every_open_gets_a_new_generation_and_old_ones_go_stale() {
        let mut s = Inner::default();
        assert!(s.begin(req()).is_none()); // page not ready: parked
        assert_eq!(s.pending.as_ref().map(|r| r.generation), Some(1));
        assert!(s.is_current(1));
        s.ready = true;
        let second = s.begin(req()).unwrap();
        assert_eq!(second.generation, 2);
        assert!(s.pending.is_none(), "a ready page gets it at once");
        assert!(!s.is_current(1));
        assert!(s.is_current(2));
    }

    #[test]
    fn a_stale_close_leaves_the_newer_session_alone() {
        let mut s = Inner {
            ready: true,
            ..Default::default()
        };
        let first = s.begin(req()).unwrap().generation;
        let second = s.begin(req()).unwrap().generation;
        assert!(!s.begin_close(Some(first)), "stale close is refused");
        assert!(s.is_current(second), "and changes nothing");
        assert!(s.begin_close(Some(second)));
        assert!(!s.is_current(second));
    }

    #[test]
    fn show_after_close_is_a_no_op_and_an_unconditional_close_always_closes() {
        let mut s = Inner {
            ready: true,
            ..Default::default()
        };
        let g = s.begin(req()).unwrap().generation;
        assert!(s.allows_show(g));
        assert!(s.begin_close(None));
        assert!(!s.allows_show(g));
        assert!(s.begin_close(None), "closing an already closed pad is fine");
    }

    #[test]
    fn a_parked_open_is_replaced_by_the_next_one_and_dropped_by_a_close() {
        let mut s = Inner::default();
        s.begin(req());
        s.begin(req());
        assert_eq!(s.pending.as_ref().map(|r| r.generation), Some(2));
        s.end();
        assert!(s.pending.is_none());
        assert!(!s.is_current(2), "a close makes a late show/close a no-op");
    }
}
