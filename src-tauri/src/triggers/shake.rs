//! Shake-while-dragging (spec §4.1): with the left button held, >=3 horizontal direction
//! reversals within 500 ms, each stroke >=40 px, arm the trigger. Pure; the mouse hook
//! feeds it (triggers/mod.rs). Strokes are measured in physical px (deviation 13).
use std::collections::VecDeque;

pub const MIN_STROKE_PX: i32 = 40;
pub const WINDOW_MS: u64 = 500;
pub const REVERSALS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Down,
    Move,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    pub t_ms: u64,
    pub x: i32,
    pub y: i32,
    pub input: Input,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shake {
    None,
    /// A shake: open the probe pad. At most once per press.
    Armed,
}

/// Milliseconds from `earlier` to `now`, both low 32 bits of the system tick (it wraps
/// every 49.7 days; a plain u64 difference would then see a huge or negative gap).
fn elapsed_ms(earlier: u64, now: u64) -> u64 {
    u64::from((now as u32).wrapping_sub(earlier as u32))
}

#[derive(Default, Debug)]
pub struct Detector {
    down: bool,
    fired: bool,
    /// -1 moving left, +1 right, 0 not yet moving far enough.
    dir: i32,
    /// x where the current stroke started.
    anchor: i32,
    /// Farthest x of the current stroke.
    extreme: i32,
    reversals: VecDeque<u64>,
}

impl Detector {
    pub fn feed(&mut self, s: Sample) -> Shake {
        match s.input {
            Input::Down => {
                *self = Detector {
                    down: true,
                    anchor: s.x,
                    extreme: s.x,
                    ..Default::default()
                };
                Shake::None
            }
            Input::Up => {
                *self = Detector::default();
                Shake::None
            }
            Input::Move if self.down && !self.fired => self.step(s.t_ms, s.x),
            Input::Move => Shake::None,
        }
    }

    fn step(&mut self, t: u64, x: i32) -> Shake {
        if self.dir == 0 {
            let d = x - self.anchor;
            if d.abs() >= MIN_STROKE_PX {
                self.dir = d.signum();
                self.extreme = x;
            }
            return Shake::None;
        }
        if (x - self.extreme) * self.dir > 0 {
            self.extreme = x;
            return Shake::None;
        }
        if (self.extreme - x) * self.dir >= MIN_STROKE_PX {
            if (self.extreme - self.anchor) * self.dir >= MIN_STROKE_PX {
                self.reversals.push_back(t);
            }
            self.anchor = self.extreme;
            self.extreme = x;
            self.dir = -self.dir;
            while self
                .reversals
                .front()
                .is_some_and(|&r| elapsed_ms(r, t) > WINDOW_MS)
            {
                self.reversals.pop_front();
            }
            if self.reversals.len() >= REVERSALS {
                self.fired = true;
                return Shake::Armed;
            }
        }
        Shake::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A button-down at `x0`, then strokes to each x in `turns`, each taking `stroke_ms`,
    /// sampled every 8 ms (a 125 Hz mouse), then (optionally) the button-up.
    fn stream(x0: i32, turns: &[i32], stroke_ms: u64, up: bool) -> Vec<Sample> {
        let mut out = vec![Sample {
            t_ms: 1000,
            x: x0,
            y: 500,
            input: Input::Down,
        }];
        append(&mut out, 0, turns, stroke_ms);
        if up {
            let last = *out.last().unwrap();
            out.push(Sample {
                t_ms: last.t_ms + 8,
                x: last.x,
                y: last.y,
                input: Input::Up,
            });
        }
        out
    }

    /// Continue `out` after `pause_ms` of stillness with more strokes.
    fn append(out: &mut Vec<Sample>, pause_ms: u64, turns: &[i32], stroke_ms: u64) {
        let last = *out.last().unwrap();
        let (mut t, mut x) = (last.t_ms + pause_ms, last.x);
        if pause_ms > 0 {
            out.push(Sample {
                t_ms: t,
                x,
                y: last.y,
                input: Input::Move,
            });
        }
        for &to in turns {
            let steps = (stroke_ms / 8).max(1);
            for i in 1..=steps {
                let xi = x + (to - x) * i as i32 / steps as i32;
                out.push(Sample {
                    t_ms: t + i * 8,
                    x: xi,
                    y: last.y,
                    input: Input::Move,
                });
            }
            t += steps * 8;
            x = to;
        }
    }

    fn run(samples: &[Sample]) -> Vec<Shake> {
        let mut d = Detector::default();
        samples
            .iter()
            .map(|s| d.feed(*s))
            .filter(|r| *r != Shake::None)
            .collect()
    }

    #[test]
    fn a_quick_shake_arms_once_and_the_release_reports_nothing() {
        let s = stream(500, &[580, 500, 580, 500, 580, 500], 90, true);
        assert_eq!(run(&s), vec![Shake::Armed]);
    }

    #[test]
    fn a_shake_across_the_tick_wrap_still_arms() {
        let mut s = stream(500, &[580, 500, 580, 500, 580, 500], 90, true);
        // Place the stream so its tick crosses u32::MAX partway through, as the OS tick
        // does; samples carry the low 32 bits.
        let base = u64::from(u32::MAX) - 1300;
        for x in &mut s {
            x.t_ms = (base + x.t_ms) & u64::from(u32::MAX);
        }
        assert!(s.iter().any(|x| x.t_ms < 300) && s.iter().any(|x| x.t_ms > 1_000_000));
        assert_eq!(run(&s), vec![Shake::Armed]);
        assert_eq!(elapsed_ms(u64::from(u32::MAX) - 5, 10), 16);
    }

    #[test]
    fn slow_waving_does_not_arm() {
        assert!(run(&stream(500, &[580, 500, 580, 500, 580], 320, true)).is_empty());
    }

    #[test]
    fn small_jitter_does_not_arm() {
        assert!(run(&stream(500, &[530, 500, 530, 500, 530, 500, 530], 60, true)).is_empty());
    }

    #[test]
    fn a_straight_drag_like_a_text_selection_does_not_arm() {
        assert!(run(&stream(100, &[900], 600, true)).is_empty());
    }

    #[test]
    fn vertical_shaking_does_not_arm() {
        let mut d = Detector::default();
        d.feed(Sample {
            t_ms: 0,
            x: 500,
            y: 500,
            input: Input::Down,
        });
        let mut armed = false;
        for i in 0..60u64 {
            let y = if (i / 6) % 2 == 0 {
                500 + (i % 6) as i32 * 20
            } else {
                600 - (i % 6) as i32 * 20
            };
            // A hand shaking vertically still wobbles a little sideways (L5).
            let x = 500 + if i % 2 == 0 { 15 } else { -15 };
            armed |= d.feed(Sample {
                t_ms: i * 8,
                x,
                y,
                input: Input::Move,
            }) == Shake::Armed;
        }
        assert!(!armed);
    }

    #[test]
    fn two_fast_reversals_a_pause_and_one_more_do_not_arm() {
        let mut s = stream(500, &[580, 500, 580], 90, false);
        append(&mut s, 600, &[500], 90);
        assert!(run(&s).is_empty());
    }

    #[test]
    fn a_diagonal_shake_with_wide_horizontal_strokes_arms() {
        let mut d = Detector::default();
        d.feed(Sample {
            t_ms: 1000,
            x: 500,
            y: 500,
            input: Input::Down,
        });
        let mut armed = 0;
        let mut t = 1000;
        let mut x = 500;
        for to in [580, 500, 580, 500, 580, 500] {
            for i in 1..=11 {
                t += 8;
                let xi = x + (to - x) * i / 11;
                let r = d.feed(Sample {
                    t_ms: t,
                    x: xi,
                    y: 500 + (xi - 500) / 2,
                    input: Input::Move,
                });
                armed += (r == Shake::Armed) as u32;
            }
            x = to;
        }
        assert_eq!(armed, 1);
    }

    #[test]
    fn moves_without_the_button_do_nothing_and_a_new_press_can_arm_again() {
        let mut d = Detector::default();
        for i in 0..40u64 {
            let x = if (i / 5) % 2 == 0 { 500 } else { 600 };
            assert_eq!(
                d.feed(Sample {
                    t_ms: i * 8,
                    x,
                    y: 0,
                    input: Input::Move
                }),
                Shake::None
            );
        }
        let s = stream(500, &[580, 500, 580, 500], 90, true);
        assert_eq!(
            s.iter()
                .map(|x| d.feed(*x))
                .filter(|r| *r != Shake::None)
                .count(),
            1
        );
        let s = stream(500, &[580, 500, 580, 500], 90, false);
        assert_eq!(
            s.iter()
                .map(|x| d.feed(*x))
                .filter(|r| *r == Shake::Armed)
                .count(),
            1
        );
    }

    #[test]
    fn a_click_without_a_shake_reports_nothing() {
        let mut d = Detector::default();
        assert_eq!(
            d.feed(Sample {
                t_ms: 0,
                x: 1,
                y: 1,
                input: Input::Down
            }),
            Shake::None
        );
        assert_eq!(
            d.feed(Sample {
                t_ms: 50,
                x: 1,
                y: 1,
                input: Input::Up
            }),
            Shake::None
        );
    }
}
