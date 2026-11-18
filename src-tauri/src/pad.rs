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

/// Position the pad over the cursor and hand it these files. Drag mode shows it at
/// once (without focus); click mode waits for `pad_show` once the page has drawn.
pub fn open(app: &AppHandle, paths: Vec<String>, mode: PadMode) {
    let Some(w) = app.get_webview_window(LABEL) else {
        log::warn!("pad window missing");
        return;
    };
    let cursor = app
        .cursor_position()
        .unwrap_or(tauri::PhysicalPosition::new(0.0, 0.0));
    let anchor = (cursor.x.round() as i32, cursor.y.round() as i32);
    if let Ok(Some(m)) = app.monitor_from_point(cursor.x, cursor.y) {
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
    let req = PadOpen {
        paths,
        mode,
        reduced_transparency: !platform::transparency_enabled(),
        generation: 0,
    };
    {
        let st = app.state::<PadState>();
        let mut inner = lock(&st.inner);
        if let Some(req) = inner.begin(req) {
            let _ = app.emit_to(LABEL, "pad://open", &req);
        }
    }
    if mode == PadMode::Drag {
        match raw_hwnd(&w) {
            Some(h) => platform::show_no_activate(h),
            None => {
                let _ = w.show();
            }
        }
        track_cursor(app, true);
    }
}

/// The pad page is listening: deliver an open that arrived earlier.
pub fn ready(app: &AppHandle) {
    let st = app.state::<PadState>();
    let mut inner = lock(&st.inner);
    inner.ready = true;
    if let Some(req) = inner.pending.take() {
        let _ = app.emit_to(LABEL, "pad://open", &req);
    }
}

/// Show and focus the pad for a click-mode open. A no-op if a newer open or a close
/// has happened since.
pub fn show(app: &AppHandle, generation: u64) {
    let st = app.state::<PadState>();
    if !lock(&st.inner).is_current(generation) {
        return;
    }
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Hide the pad, whatever open it belongs to, and invalidate that open.
pub fn close(app: &AppHandle) {
    lock(&app.state::<PadState>().inner).end();
    track_cursor(app, false);
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
}

/// Hide the pad only if `generation` is still the current open.
pub fn close_if_current(app: &AppHandle, generation: u64) {
    let current = lock(&app.state::<PadState>().inner).is_current(generation);
    if current {
        close(app);
    }
}

/// While a drag is involved, emit the cursor relative to the pad (logical px) at 60 Hz:
/// Tauri sends no positions after a drag leaves the window (spec §4.3).
pub fn track_cursor(app: &AppHandle, on: bool) {
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
