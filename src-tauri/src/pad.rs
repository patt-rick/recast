//! The pad window (spec §4.4, §6.7): created hidden at startup, then positioned,
//! shown and hidden for every open. Plan 3's triggers call `open` too.
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

use crate::lockutil::lock;
use crate::platform;

pub const LABEL: &str = "pad";
pub const SIZE: f64 = 368.0;

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
    /// A shake probe: drawn only once a file drag enters it (deviation 1).
    pub probe: bool,
    /// The open came from a trigger with an anchor (handle, desk pad): leave-to-dismiss
    /// is armed from the start, since the cursor begins outside the pad (B2).
    pub armed: bool,
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
    /// Mode of the open pad; None while closed.
    mode: Option<PadMode>,
    /// Paths of the current open.
    paths: Vec<String>,
    /// A shake probe that no file drag has entered yet.
    probe: bool,
    /// Something was dropped on the current open (B1).
    dropped: bool,
}

impl Inner {
    /// Start a new open: bump the generation and either hand the request back for
    /// immediate delivery (page ready) or park it until `pad_ready`.
    fn begin(&mut self, mut req: PadOpen) -> Option<PadOpen> {
        self.generation += 1;
        req.generation = self.generation;
        self.mode = Some(req.mode);
        self.paths = req.paths.clone();
        self.probe = req.probe;
        self.dropped = false;
        if self.ready {
            self.pending = None;
            Some(req)
        } else {
            self.pending = Some(req);
            None
        }
    }

    /// The pad page started (re)loading: its listeners are gone until `pad_ready`.
    /// Returns true when an open that was already delivered to the old page is now
    /// orphaned (no parked request will replace it), so the caller must close it.
    fn page_loading(&mut self) -> bool {
        let was_ready = std::mem::replace(&mut self.ready, false);
        was_ready && self.mode.is_some() && self.pending.is_none()
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }

    /// Invalidate the current open (and any parked request).
    fn end(&mut self) {
        self.generation += 1;
        self.pending = None;
        self.mode = None;
        self.paths.clear();
        self.probe = false;
        self.dropped = false;
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

    /// `pad_show(generation)`: allowed only for the current open; the pad is now a
    /// focused click-mode pad.
    fn shown_focused(&mut self, generation: u64) -> bool {
        if !self.is_current(generation) || self.mode.is_none() {
            return false;
        }
        self.mode = Some(PadMode::Click);
        self.probe = false;
        true
    }

    fn is_open_with(&self, paths: &[String]) -> bool {
        self.mode.is_some() && self.paths == paths
    }

    /// Drag mode, nothing dropped yet: the pad is still waiting for the user to let go.
    fn dragging(&self, generation: u64) -> bool {
        self.is_current(generation) && self.mode == Some(PadMode::Drag) && !self.dropped
    }

    fn waiting_for_drop(&self) -> bool {
        self.dragging(self.generation)
    }

    fn unconfirmed_probe(&self, generation: u64) -> bool {
        self.is_current(generation) && self.probe
    }

    fn confirm(&mut self, generation: u64) -> bool {
        if self.unconfirmed_probe(generation) {
            self.probe = false;
            true
        } else {
            false
        }
    }

    /// A drop landed on the current open.
    fn dropped(&mut self) {
        self.dropped = true;
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

/// Exclude the pad from capture once, at startup. `PadState` is managed on the Builder.
pub fn init(app: &AppHandle) {
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

pub const PROBE_MS: u64 = 300;
pub const RELEASE_GRACE_MS: u64 = 1500;

/// Open at the cursor. Drag mode shows it at once (without focus) once the page is
/// ready; click mode waits for `pad_show` once the page has drawn.
pub fn open(app: &AppHandle, paths: Vec<String>, mode: PadMode) {
    open_at(app, paths, mode, None);
}

/// Open centred on `anchor` (physical px), or on the cursor when None.
pub fn open_at(app: &AppHandle, paths: Vec<String>, mode: PadMode, anchor: Option<(i32, i32)>) {
    log::info!("pad: open ({mode:?}, {} files)", paths.len());
    on_main(app, move |app| {
        open_now(app, paths, mode, anchor, false);
    });
}

/// A shake was detected: show an invisible drag-mode pad under the cursor. It is drawn
/// only if a file drag enters it (`drag_entered`), and hidden after PROBE_MS otherwise.
pub fn open_probe(app: &AppHandle) {
    on_main(app, |app| {
        let Some(g) = open_now(app, vec![], PadMode::Drag, None, true) else {
            return;
        };
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(PROBE_MS));
            on_main(&app, move |app| {
                let unconfirmed = lock(&app.state::<PadState>().inner).unconfirmed_probe(g);
                if unconfirmed {
                    dismiss_and_close(app, g);
                }
            });
        });
    });
}

/// Tell the page, then close this open (B10): the page must not keep a session for a
/// pad Rust has hidden.
fn dismiss_and_close(app: &AppHandle, generation: u64) {
    let _ = app.emit_to(LABEL, "pad://dismiss", ());
    close_now(app, Some(generation));
}

/// A file drag (one that carries paths) entered the pad window: a shake probe is real
/// (B1). Called from `on_window_event`, on the main thread.
pub fn drag_entered(app: &AppHandle) {
    let confirmed = {
        let st = app.state::<PadState>();
        let mut inner = lock(&st.inner);
        let g = inner.generation;
        inner.confirm(g)
    };
    if confirmed {
        log::info!("pad: drag entered");
    }
}

/// Something was dropped on the pad window (B1). Called on the main thread.
pub fn dropped(app: &AppHandle) {
    lock(&app.state::<PadState>().inner).dropped();
}

/// The pad is open (any mode). Cheap; callable from any thread.
pub fn is_open(app: &AppHandle) -> bool {
    app.try_state::<PadState>()
        .is_some_and(|st| lock(&st.inner).mode.is_some())
}

/// The pad is open with exactly these files.
pub fn is_open_with(app: &AppHandle, paths: &[String]) -> bool {
    app.try_state::<PadState>()
        .is_some_and(|st| lock(&st.inner).is_open_with(paths))
}

/// The current pad is a drag-mode pad that nothing has been dropped on (B14).
pub fn is_waiting_for_drop(app: &AppHandle) -> bool {
    app.try_state::<PadState>()
        .is_some_and(|st| lock(&st.inner).waiting_for_drop())
}

/// The button has been up for the grace period (§4.1, B2).
pub fn grace_due(up_since_ms: Option<u64>, now_ms: u64) -> bool {
    up_since_ms.is_some_and(|t| now_ms.saturating_sub(t) >= RELEASE_GRACE_MS)
}

/// Leave-to-dismiss is armed from the start when the open has an anchor (B2).
fn armed_for(anchor: Option<(i32, i32)>) -> bool {
    anchor.is_some()
}

fn open_now(
    app: &AppHandle,
    paths: Vec<String>,
    mode: PadMode,
    anchor: Option<(i32, i32)>,
    probe: bool,
) -> Option<u64> {
    let Some(w) = app.get_webview_window(LABEL) else {
        log::warn!("pad window missing");
        return None;
    };
    let cursor = match app.cursor_position() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("pad: cursor position unavailable, using the origin: {e}");
            PhysicalPosition::new(0.0, 0.0)
        }
    };
    let at = anchor
        .map(|(x, y)| PhysicalPosition::new(x as f64, y as f64))
        .unwrap_or(cursor);
    let anchor_px = (at.x.round() as i32, at.y.round() as i32);
    let armed = armed_for(anchor);
    match app.monitor_from_point(at.x, at.y) {
        Ok(Some(m)) => {
            let wa = m.work_area();
            let px = (SIZE * m.scale_factor()).round() as u32;
            let (x, y) = place(
                anchor_px,
                px,
                (wa.position.x, wa.position.y, wa.size.width, wa.size.height),
            );
            // Position first: the size is then interpreted at the target monitor's DPI.
            let _ = w.set_position(PhysicalPosition::new(x, y));
            let _ = w.set_size(tauri::PhysicalSize::new(px, px));
        }
        Ok(None) => log::warn!("pad: no monitor at {anchor_px:?}; leaving the pad where it is"),
        Err(e) => log::warn!("pad: monitor lookup failed ({e}); leaving the pad where it is"),
    }
    let req = PadOpen {
        paths,
        mode,
        reduced_transparency: !platform::transparency_enabled(),
        generation: 0,
        probe,
        armed,
    };
    // One guard from the generation bump to the show, so a close or newer open can't
    // slip in between. Emitting and acting under it relies on there being no Rust-side
    // listeners that take this lock.
    let st = app.state::<PadState>();
    let mut inner = lock(&st.inner);
    if let Some(req) = inner.begin(req) {
        let _ = app.emit_to(LABEL, "pad://open", &req);
        if mode == PadMode::Drag {
            show_drag(app, &w, req.generation);
        } else {
            // A click-mode open retires any poller left over from an earlier drag-mode open.
            track_cursor(app, false, 0);
        }
    }
    // Page not ready: `ready` delivers the parked open and, in drag mode, shows it.
    Some(inner.generation)
}

/// Show without taking focus (an OLE drag is in progress) and start the cursor feed.
fn show_drag(app: &AppHandle, w: &tauri::WebviewWindow, generation: u64) {
    match raw_hwnd(w) {
        Some(h) => platform::show_no_activate(h),
        None => {
            let _ = w.show();
        }
    }
    // A drag-mode pad (a shake probe above all) is only of use if the drag can reach it.
    crate::drops::cover_soon(app);
    track_cursor(app, true, generation);
}

/// The pad page started loading (a reload): it is not listening any more. An open the
/// old page already had would be an invisible topmost window, so close it.
pub fn page_loading(app: &AppHandle) {
    let Some(st) = app.try_state::<PadState>() else {
        return;
    };
    let orphaned = lock(&st.inner).page_loading();
    if orphaned {
        log::warn!("pad: page reloaded while open; closing");
        on_main(app, |app| close_now(app, None));
    }
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
                show_drag(app, &w, req.generation);
            } else {
                track_cursor(app, false, 0);
            }
        }
    });
}

/// Show and focus the pad (click mode, or a drag-mode pad after a drop). A no-op if a
/// newer open or a close has happened since.
pub fn show(app: &AppHandle, generation: u64) {
    on_main(app, move |app| {
        let st = app.state::<PadState>();
        let mut inner = lock(&st.inner);
        if !inner.shown_focused(generation) {
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
    track_cursor(app, false, 0);
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
        track_cursor(app, on, generation);
    }
    drop(inner);
}

/// While a drag is involved, emit the cursor relative to the pad (logical px) at 60 Hz:
/// Tauri sends no positions after a drag leaves the window (spec §4.3). The poller also
/// watches the mouse button: once it has been up for the grace period while the open
/// `generation` is still waiting for a drop, it dismisses the pad (§4.1, B2).
fn track_cursor(app: &AppHandle, on: bool, generation: u64) {
    let st = app.state::<PadState>();
    let epoch = st.poll.clone();
    // Any bump retires every running poller; only `on` starts a new one.
    let start = epoch.fetch_add(1, Ordering::SeqCst) + 1;
    if !on {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let began = Instant::now();
        let mut up_since: Option<u64> = None;
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
            let now = began.elapsed().as_millis() as u64;
            if platform::any_button_down() {
                up_since = None;
            } else {
                up_since.get_or_insert(now);
            }
            if grace_due(up_since, now) && lock(&app.state::<PadState>().inner).dragging(generation)
            {
                log::info!("pad: released with nothing dropped; closing");
                let _ = app.emit_to(LABEL, "pad://dismiss", ());
                close_if_current(&app, generation);
                break;
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
            probe: false,
            armed: false,
        }
    }

    fn probe() -> PadOpen {
        PadOpen {
            probe: true,
            mode: PadMode::Drag,
            ..req()
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
        assert!(s.shown_focused(g));
        assert!(s.begin_close(None));
        assert!(!s.shown_focused(g));
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

    #[test]
    fn mode_and_paths_follow_the_current_open() {
        let mut s = Inner {
            ready: true,
            ..Default::default()
        };
        assert!(!s.is_open_with(&[]));
        let g = s
            .begin(PadOpen {
                paths: vec!["a".into()],
                mode: PadMode::Drag,
                ..req()
            })
            .unwrap()
            .generation;
        assert!(s.is_open_with(&["a".to_string()]));
        assert!(s.dragging(g));
        assert!(
            s.shown_focused(g),
            "a drop turns the session into click mode"
        );
        assert!(!s.dragging(g));
        assert!(s.begin_close(Some(g)));
        assert!(!s.is_open_with(&["a".to_string()]));
        assert!(!s.shown_focused(g), "stale");
    }

    #[test]
    fn a_probe_needs_confirming() {
        let mut s = Inner {
            ready: true,
            ..Default::default()
        };
        let g = s.begin(probe()).unwrap().generation;
        assert!(s.unconfirmed_probe(g));
        assert!(s.confirm(g));
        assert!(!s.unconfirmed_probe(g));
        assert!(!s.confirm(g), "confirming twice is a no-op");
        let h = s
            .begin(PadOpen {
                mode: PadMode::Drag,
                ..req()
            })
            .unwrap()
            .generation;
        assert!(!s.unconfirmed_probe(h));
        assert!(!s.confirm(g), "stale generation");
    }

    #[test]
    fn a_drop_ends_the_drag_wait_and_a_new_open_or_close_clears_it() {
        let mut s = Inner {
            ready: true,
            ..Default::default()
        };
        let g = s.begin(probe()).unwrap().generation;
        assert!(s.dragging(g));
        assert!(s.waiting_for_drop());
        s.dropped();
        assert!(!s.dragging(g), "a dropped-on pad is no longer waiting");
        assert!(!s.waiting_for_drop());
        let h = s
            .begin(PadOpen {
                mode: PadMode::Drag,
                ..req()
            })
            .unwrap()
            .generation;
        assert!(s.dragging(h), "begin clears the drop flag");
        s.dropped();
        s.end();
        let k = s
            .begin(PadOpen {
                mode: PadMode::Drag,
                ..req()
            })
            .unwrap()
            .generation;
        assert!(s.dragging(k));
        assert!(s.begin_close(Some(k)));
        assert!(!s.waiting_for_drop(), "closed");
    }

    #[test]
    fn a_reload_unreadies_the_page_and_orphans_a_delivered_open() {
        let mut s = Inner {
            ready: true,
            ..Default::default()
        };
        assert!(s.begin(req()).is_some());
        assert!(s.page_loading(), "the delivered open is orphaned");
        assert!(!s.ready);
        assert!(s.begin(req()).is_none(), "the next open parks");
        assert!(s.pending.is_some());
        assert!(!s.page_loading(), "already not ready: nothing to close");
    }

    #[test]
    fn the_first_load_keeps_a_parked_open() {
        let mut s = Inner::default();
        assert!(s.begin(req()).is_none());
        assert!(!s.page_loading(), "not ready yet: nothing was delivered");
        assert!(s.pending.is_some());
        // Ready, closed, then a reload: nothing open, nothing to close.
        s.ready = true;
        s.end();
        assert!(!s.page_loading());
    }

    #[test]
    fn armed_follows_whether_the_open_has_an_anchor() {
        assert!(armed_for(Some((1, 2))));
        assert!(!armed_for(None));
    }

    #[test]
    fn release_grace_is_due_after_the_grace_period() {
        assert!(!grace_due(None, 5000));
        assert!(!grace_due(Some(1000), 1000 + RELEASE_GRACE_MS - 1));
        assert!(grace_due(Some(1000), 1000 + RELEASE_GRACE_MS));
    }
}
