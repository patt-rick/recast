//! Progress from engine stdout (spec §6.4).
use crate::presets::ProgressKind;

#[derive(Debug, Default, PartialEq)]
pub struct Feed {
    /// A progress line (not worth keeping in the stderr tail).
    pub consumed: bool,
    /// Real forward movement: resets the stall watchdog.
    pub advanced: bool,
    /// 0..=1 when known.
    pub pct: Option<f32>,
}

pub struct Tracker {
    kind: ProgressKind,
    duration_s: Option<f64>,
    last_us: i64,
    total_pages: Option<u32>,
    last_page: u32,
}

impl Tracker {
    pub fn new(kind: ProgressKind, duration_s: Option<f64>) -> Self {
        Self { kind, duration_s, last_us: -1, total_pages: None, last_page: 0 }
    }

    pub fn feed(&mut self, line: &str) -> Feed {
        let line = line.trim();
        match self.kind {
            ProgressKind::Ffmpeg => self.ffmpeg(line),
            ProgressKind::GhostscriptPages => self.gs(line),
            ProgressKind::None => Feed::default(),
        }
    }

    /// Whether the stall timer applies yet. ffmpeg is silent until the first output
    /// timestamp (palettegen for GIF decodes the whole input first) and again while
    /// finishing (+faststart rewrites the file); in those windows only the hard cap applies.
    pub fn stall_armed(&self) -> bool {
        match self.kind {
            ProgressKind::Ffmpeg => {
                let near_end = self.duration_s.is_some_and(|d| d > 0.0 && self.last_us as f64 / 1e6 >= d * 0.99);
                self.last_us > 0 && !near_end
            }
            ProgressKind::GhostscriptPages => self.total_pages.is_some() || self.last_page > 0,
            ProgressKind::None => false,
        }
    }

    fn ffmpeg(&mut self, line: &str) -> Feed {
        let Some((k, v)) = line.split_once('=') else { return Feed::default() };
        if k.is_empty() || !k.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
            return Feed::default();
        }
        let mut f = Feed { consumed: true, ..Default::default() };
        if k == "out_time_us" {
            if let Ok(us) = v.trim().parse::<i64>()
                && us > self.last_us {
                    self.last_us = us;
                    f.advanced = true;
                    if let Some(d) = self.duration_s.filter(|d| *d > 0.0) {
                        f.pct = Some(((us as f64 / 1e6) / d).clamp(0.0, 1.0) as f32);
                    }
                }
        } else if k == "progress" && v.trim() == "end" {
            f.advanced = true;
            f.pct = Some(1.0);
        }
        f
    }

    fn gs(&mut self, line: &str) -> Feed {
        if let Some(rest) = line.strip_prefix("Processing pages ") {
            self.total_pages = rest.trim_end_matches('.').rsplit(' ').next().and_then(|t| t.parse().ok());
            return Feed { consumed: true, advanced: true, pct: Some(0.0) };
        }
        if let Some(n) = line.strip_prefix("Page ").and_then(|t| t.trim().parse::<u32>().ok()) {
            self.last_page = self.last_page.max(n);
            let pct = self.total_pages.map(|t| ((n.saturating_sub(1)) as f32 / t.max(1) as f32).min(1.0));
            return Feed { consumed: true, advanced: true, pct };
        }
        Feed::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffmpeg_progress_lines() {
        let mut t = Tracker::new(ProgressKind::Ffmpeg, Some(10.0));
        assert_eq!(t.feed("frame=12"), Feed { consumed: true, advanced: false, pct: None });
        assert_eq!(t.feed("out_time_us=5000000"), Feed { consumed: true, advanced: true, pct: Some(0.5) });
        assert_eq!(t.feed("out_time_us=5000000"), Feed { consumed: true, advanced: false, pct: None }, "no movement is not progress");
        assert_eq!(t.feed("out_time_us=N/A"), Feed { consumed: true, advanced: false, pct: None });
        assert_eq!(t.feed("progress=end"), Feed { consumed: true, advanced: true, pct: Some(1.0) });
        assert_eq!(t.feed("[mp3 @ 0x1] Some warning"), Feed::default());
    }

    #[test]
    fn ffmpeg_without_duration_still_advances() {
        let mut t = Tracker::new(ProgressKind::Ffmpeg, None);
        assert_eq!(t.feed("out_time_us=100"), Feed { consumed: true, advanced: true, pct: None });
    }

    #[test]
    fn ghostscript_pages() {
        let mut t = Tracker::new(ProgressKind::GhostscriptPages, None);
        assert_eq!(t.feed("GPL Ghostscript 10.04.0 (2024-09-18)"), Feed::default());
        assert_eq!(t.feed("Processing pages 1 through 4."), Feed { consumed: true, advanced: true, pct: Some(0.0) });
        assert_eq!(t.feed("Page 1"), Feed { consumed: true, advanced: true, pct: Some(0.0) });
        assert_eq!(t.feed("Page 3"), Feed { consumed: true, advanced: true, pct: Some(0.5) });
    }

    #[test]
    fn stall_timer_arms_only_between_first_output_and_the_end() {
        let mut t = Tracker::new(ProgressKind::Ffmpeg, Some(10.0));
        assert!(!t.stall_armed());
        t.feed("out_time_us=N/A");
        assert!(!t.stall_armed(), "GIF palettegen: no output yet");
        t.feed("out_time_us=0");
        assert!(!t.stall_armed());
        t.feed("out_time_us=2000000");
        assert!(t.stall_armed());
        t.feed("out_time_us=9950000");
        assert!(!t.stall_armed(), "finishing the file (+faststart)");
    }

    #[test]
    fn none_consumes_nothing() {
        let mut t = Tracker::new(ProgressKind::None, None);
        assert_eq!(t.feed("out_time_us=5"), Feed::default());
    }
}
