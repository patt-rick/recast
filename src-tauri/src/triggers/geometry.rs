//! Where the edge handle, the desk pad and the pad go (spec §4.1), and what counts as
//! fullscreen (§6.7). Physical pixels throughout; `scale` is the monitor's DPI scale.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn centre(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
    /// From left, top, right, bottom.
    pub fn ltrb(l: i32, t: i32, r: i32, b: i32) -> Rect {
        Rect {
            x: l,
            y: t,
            w: r - l,
            h: b - t,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    pub fn parse(s: &str) -> Edge {
        match s {
            "right" => Edge::Right,
            "top" => Edge::Top,
            "bottom" => Edge::Bottom,
            _ => Edge::Left,
        }
    }
}

pub const HANDLE_LONG: f64 = 80.0;
pub const HANDLE_SHORT: f64 = 6.0;
pub const PAD_GAP: f64 = 12.0;
pub const DESK_SIZE: f64 = 96.0;
pub const DESK_MARGIN: f64 = 24.0;

fn px(v: f64, scale: f64) -> i32 {
    (v * scale).round().max(1.0) as i32
}

/// The handle on `edge` of `work`, `pos` (0..1) along it.
pub fn handle_rect(edge: Edge, pos: f64, work: Rect, scale: f64) -> Rect {
    let (long, short) = (px(HANDLE_LONG, scale), px(HANDLE_SHORT, scale));
    let along = |start: i32, len: i32| {
        start + ((len - long).max(0) as f64 * pos.clamp(0.0, 1.0)).round() as i32
    };
    match edge {
        Edge::Left => Rect {
            x: work.x,
            y: along(work.y, work.h),
            w: short,
            h: long,
        },
        Edge::Right => Rect {
            x: work.right() - short,
            y: along(work.y, work.h),
            w: short,
            h: long,
        },
        Edge::Top => Rect {
            x: along(work.x, work.w),
            y: work.y,
            w: long,
            h: short,
        },
        Edge::Bottom => Rect {
            x: along(work.x, work.w),
            y: work.bottom() - short,
            w: long,
            h: short,
        },
    }
}

pub const HANDLE_GROWN_W: f64 = 180.0;
pub const HANDLE_GROWN_H: f64 = 80.0;

/// The handle window while highlighted (L9): 180 × 80 logical px, anchored on its edge
/// and growing toward the screen centre, so a "Drag a file here" label fits beside the
/// tab. The tab is still `HANDLE_SHORT` thick, flush with the edge.
pub fn handle_rect_highlighted(edge: Edge, pos: f64, work: Rect, scale: f64) -> Rect {
    let (w, h) = (px(HANDLE_GROWN_W, scale), px(HANDLE_GROWN_H, scale));
    let base = handle_rect(edge, pos, work, scale);
    match edge {
        Edge::Left => Rect { w, h, ..base },
        Edge::Right => Rect {
            x: work.right() - w,
            w,
            h,
            ..base
        },
        Edge::Top | Edge::Bottom => {
            let x = base.x.clamp(work.x, (work.right() - w).max(work.x));
            let y = if edge == Edge::Top {
                work.y
            } else {
                work.bottom() - h
            };
            Rect { x, y, w, h }
        }
    }
}

/// Centre for a `pad_px` pad beside the handle, so the handle stays free for a drop.
/// `pad::place` then clamps it into the work area.
pub fn pad_anchor_beside(edge: Edge, handle: Rect, pad_px: i32, scale: f64) -> (i32, i32) {
    let (gap, half) = (px(PAD_GAP, scale), pad_px / 2);
    let (cx, cy) = handle.centre();
    match edge {
        Edge::Left => (handle.right() + gap + half, cy),
        Edge::Right => (handle.x - gap - half, cy),
        Edge::Top => (cx, handle.bottom() + gap + half),
        Edge::Bottom => (cx, handle.y - gap - half),
    }
}

/// The desk pad's top-left: the saved spot kept inside `work`, or bottom-right if unset.
/// "Unset" is exactly (-1, -1); any other negative value is a real spot on a monitor
/// left of or above the primary (B9).
pub fn desk_position(saved_x: f64, saved_y: f64, work: Rect, scale: f64) -> (i32, i32) {
    let s = px(DESK_SIZE, scale);
    if saved_x == -1.0 && saved_y == -1.0 {
        let m = px(DESK_MARGIN, scale);
        return (work.right() - s - m, work.bottom() - s - m);
    }
    let clamp = |v: f64, lo: i32, len: i32| (v.round() as i32).clamp(lo, (lo + len - s).max(lo));
    (
        clamp(saved_x, work.x, work.w),
        clamp(saved_y, work.y, work.h),
    )
}

const SHELL_CLASSES: [&str; 4] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
];

/// A window covering its whole monitor (not just the work area), and not the desktop or
/// taskbar. A maximized window overhangs by its border but is not fullscreen (B8).
pub fn is_fullscreen(window: Rect, monitor: Rect, class: &str, zoomed: bool) -> bool {
    !zoomed
        && !SHELL_CLASSES.contains(&class)
        && window.x <= monitor.x
        && window.y <= monitor.y
        && window.right() >= monitor.right()
        && window.bottom() >= monitor.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };

    #[test]
    fn the_handle_sits_on_its_edge_at_its_position() {
        assert_eq!(
            handle_rect(Edge::Left, 0.5, WORK, 1.0),
            Rect {
                x: 0,
                y: 480,
                w: 6,
                h: 80
            }
        );
        assert_eq!(
            handle_rect(Edge::Right, 0.0, WORK, 1.0),
            Rect {
                x: 1914,
                y: 0,
                w: 6,
                h: 80
            }
        );
        assert_eq!(
            handle_rect(Edge::Top, 1.0, WORK, 1.0),
            Rect {
                x: 1840,
                y: 0,
                w: 80,
                h: 6
            }
        );
        assert_eq!(
            handle_rect(Edge::Bottom, 0.5, WORK, 1.5),
            Rect {
                x: 900,
                y: 1031,
                w: 120,
                h: 9
            }
        );
        let left_monitor = Rect {
            x: -2560,
            y: 0,
            w: 2560,
            h: 1400,
        };
        assert_eq!(handle_rect(Edge::Left, 0.5, left_monitor, 1.0).x, -2560);
    }

    #[test]
    fn the_pad_anchor_clears_the_handle() {
        let h = handle_rect(Edge::Left, 0.5, WORK, 1.0);
        assert_eq!(
            pad_anchor_beside(Edge::Left, h, 440, 1.0),
            (6 + 12 + 220, 520)
        );
        let h = handle_rect(Edge::Right, 0.5, WORK, 2.0);
        assert_eq!(
            pad_anchor_beside(Edge::Right, h, 880, 2.0),
            (h.x - 24 - 440, h.y + h.h / 2)
        );
        let h = handle_rect(Edge::Bottom, 0.5, WORK, 1.0);
        assert_eq!(
            pad_anchor_beside(Edge::Bottom, h, 440, 1.0),
            (960, h.y - 12 - 220)
        );
    }

    #[test]
    fn desk_pad_defaults_bottom_right_and_saved_positions_stay_on_screen() {
        assert_eq!(
            desk_position(-1.0, -1.0, WORK, 1.0),
            (1920 - 96 - 24, 1040 - 96 - 24)
        );
        assert_eq!(desk_position(100.0, 200.0, WORK, 1.0), (100, 200));
        assert_eq!(desk_position(5000.0, -300.0, WORK, 1.0), (1920 - 96, 0));
    }

    #[test]
    fn only_exactly_minus_one_minus_one_means_unset() {
        // A spot on a monitor to the left has a negative x and (maybe) negative y (B9).
        // The y here is clamped so the 96 px pad stays inside the work area.
        let left_up = Rect {
            x: -2560,
            y: -1440,
            w: 2560,
            h: 1400,
        };
        assert_eq!(desk_position(-1200.0, -40.0, left_up, 1.0), (-1200, -136));
        assert_eq!(desk_position(-1200.0, -200.0, left_up, 1.0), (-1200, -200));
        // One coordinate of -1 is a real position, not "unset".
        assert_eq!(desk_position(-1.0, 200.0, WORK, 1.0), (0, 200));
    }

    #[test]
    fn fullscreen_means_covering_the_monitor_and_never_the_shell() {
        let mon = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        assert!(is_fullscreen(mon, mon, "Chrome_WidgetWin_1", false));
        assert!(is_fullscreen(
            Rect {
                x: -8,
                y: -8,
                w: 1936,
                h: 1096
            },
            mon,
            "X",
            false
        ));
        assert!(
            !is_fullscreen(
                Rect {
                    x: -8,
                    y: -8,
                    w: 1936,
                    h: 1096
                },
                mon,
                "CabinetWClass",
                true
            ),
            "a maximized window overhangs by its border but is not fullscreen"
        );
        assert!(
            !is_fullscreen(
                Rect {
                    x: 0,
                    y: 0,
                    w: 1920,
                    h: 1040
                },
                mon,
                "X",
                false
            ),
            "maximized, taskbar visible"
        );
        assert!(!is_fullscreen(mon, mon, "Progman", false));
        assert!(!is_fullscreen(mon, mon, "WorkerW", false));
        assert!(!is_fullscreen(mon, mon, "Shell_TrayWnd", false));
    }

    #[test]
    fn the_highlighted_handle_grows_toward_the_screen_centre_with_its_tab_on_the_edge() {
        // Left: the tab's top-left stays, the window widens to the right.
        assert_eq!(
            handle_rect_highlighted(Edge::Left, 0.5, WORK, 1.0),
            Rect {
                x: 0,
                y: 480,
                w: 180,
                h: 80
            }
        );
        // Right: the window's right side stays on the edge.
        assert_eq!(
            handle_rect_highlighted(Edge::Right, 0.0, WORK, 1.0),
            Rect {
                x: 1740,
                y: 0,
                w: 180,
                h: 80
            }
        );
        // Top: 180 wide along the edge, 80 tall inward, starting where the handle starts.
        assert_eq!(
            handle_rect_highlighted(Edge::Top, 0.0, WORK, 1.0),
            Rect {
                x: 0,
                y: 0,
                w: 180,
                h: 80
            }
        );
        // Bottom, at the far end: kept inside the work area.
        assert_eq!(
            handle_rect_highlighted(Edge::Bottom, 1.0, WORK, 1.0),
            Rect {
                x: 1740,
                y: 960,
                w: 180,
                h: 80
            }
        );
        // Scaled.
        assert_eq!(
            handle_rect_highlighted(Edge::Left, 0.0, WORK, 2.0),
            Rect {
                x: 0,
                y: 0,
                w: 360,
                h: 160
            }
        );
    }

    #[test]
    fn edges_parse_with_left_as_the_fallback() {
        assert_eq!(Edge::parse("right"), Edge::Right);
        assert_eq!(Edge::parse("diagonal"), Edge::Left);
    }
}
