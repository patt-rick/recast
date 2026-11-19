//! "Open with Recast" (spec §6.10) and start with Windows: the registry entries, the
//! command line, and the debounce that turns one-process-per-file into one pad.
use std::path::Path;
use std::time::{Duration, Instant};

pub const VERB_KEY: &str = r"Software\Classes\*\shell\Recast";
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Same name the Tauri NSIS uninstaller already removes (`${PRODUCTNAME}`).
pub const RUN_VALUE: &str = "Recast";

/// One REG_SZ value to write under HKCU. `name` None is the key's default value.
#[derive(Debug, Clone, PartialEq)]
pub struct RegString {
    pub key: String,
    pub name: Option<&'static str>,
    pub data: String,
}

/// The classic-menu verb. `MultiSelectModel=Player` raises Explorer's limit for a
/// command-line verb from 15 selected items to 100.
pub fn verb_entries(exe: &str) -> Vec<RegString> {
    let s = |key: String, name: Option<&'static str>, data: String| RegString { key, name, data };
    vec![
        s(VERB_KEY.into(), None, "Open with Recast".into()),
        s(VERB_KEY.into(), Some("MultiSelectModel"), "Player".into()),
        s(VERB_KEY.into(), Some("Icon"), format!("\"{exe}\",0")),
        s(
            format!(r"{VERB_KEY}\command"),
            None,
            format!("\"{exe}\" --pad \"%1\""),
        ),
    ]
}

/// The Run value: start in the tray, without opening the companion.
pub fn run_command(exe: &str) -> String {
    format!("\"{exe}\" --autostart")
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Launch {
    /// Files to open the pad with (`--pad <paths…>`), made absolute against the cwd.
    pub pad: Vec<String>,
    /// Started by Windows at sign-in: stay in the tray.
    pub autostart: bool,
}

/// `argv[0]` is the exe. `--pad` takes every following argument up to the next `--flag`.
pub fn parse_args(argv: &[String], cwd: &Path) -> Launch {
    let mut l = Launch::default();
    let mut in_pad = false;
    for a in argv.iter().skip(1) {
        match a.as_str() {
            "--autostart" => {
                l.autostart = true;
                in_pad = false;
            }
            "--pad" => in_pad = true,
            s if s.starts_with("--") => in_pad = false,
            "" => {}
            s if in_pad => {
                let p = Path::new(s);
                let abs = if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    cwd.join(p)
                };
                l.pad.push(abs.to_string_lossy().into_owned());
            }
            _ => {}
        }
    }
    l
}

/// Explorer starts one process per selected file; their paths arrive one by one.
pub const QUIET: Duration = Duration::from_millis(300);
pub const CAP: Duration = Duration::from_secs(2);

/// Collects forwarded paths until 300 ms pass without a new arrival, or 2 s after the
/// first (spec §6.10).
#[derive(Default, Debug)]
pub struct Collector {
    first: Option<Instant>,
    last: Option<Instant>,
    paths: Vec<String>,
}

impl Collector {
    pub fn push(&mut self, paths: Vec<String>, now: Instant) {
        if paths.is_empty() {
            return;
        }
        self.first.get_or_insert(now);
        self.last = Some(now);
        for p in paths {
            if !self.paths.contains(&p) {
                self.paths.push(p);
            }
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        Some((self.last? + QUIET).min(self.first? + CAP))
    }

    pub fn take_if_due(&mut self, now: Instant) -> Option<Vec<String>> {
        if now < self.deadline()? {
            return None;
        }
        self.first = None;
        self.last = None;
        Some(std::mem::take(&mut self.paths))
    }
}

/// A flush within `CAP` of the previous one opens the pad with the earlier paths too
/// (order kept, duplicates removed): a late straggler adds to the pad instead of
/// replacing it.
pub fn merge_recent(
    prev: Option<(&[String], Instant)>,
    new: Vec<String>,
    now: Instant,
) -> Vec<String> {
    let Some((prev, at)) = prev else {
        return new;
    };
    if now.saturating_duration_since(at) > CAP {
        return new;
    }
    let mut out = prev.to_vec();
    for p in new {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{Duration, Instant};

    fn argv(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_pad_paths_and_autostart() {
        let cwd = Path::new(r"C:\work");
        assert_eq!(parse_args(&argv(&["recast.exe"]), cwd), Launch::default());
        assert_eq!(
            parse_args(&argv(&["recast.exe", "--autostart"]), cwd),
            Launch {
                pad: vec![],
                autostart: true
            }
        );
        let l = parse_args(&argv(&["recast.exe", "--pad", r"C:\v\a.mov", "b.png"]), cwd);
        assert_eq!(
            l.pad,
            vec![r"C:\v\a.mov".to_string(), r"C:\work\b.png".to_string()]
        );
        assert!(!l.autostart);
        // A later flag ends the path list; empty arguments are skipped.
        let l = parse_args(
            &argv(&[
                "recast.exe",
                "--pad",
                "",
                r"C:\a.png",
                "--autostart",
                r"C:\b.png",
            ]),
            cwd,
        );
        assert_eq!(l.pad, vec![r"C:\a.png".to_string()]);
        assert!(l.autostart);
    }

    #[test]
    fn verb_entries_match_spec_6_10() {
        let e = verb_entries(r"C:\Users\x\AppData\Local\Recast\recast.exe");
        let get = |key: &str, name: Option<&str>| {
            e.iter()
                .find(|r| r.key == key && r.name == name)
                .map(|r| r.data.clone())
        };
        assert_eq!(get(VERB_KEY, None).as_deref(), Some("Open with Recast"));
        assert_eq!(
            get(VERB_KEY, Some("MultiSelectModel")).as_deref(),
            Some("Player")
        );
        assert_eq!(
            get(VERB_KEY, Some("Icon")).as_deref(),
            Some(r#""C:\Users\x\AppData\Local\Recast\recast.exe",0"#)
        );
        assert_eq!(
            get(&format!(r"{VERB_KEY}\command"), None).as_deref(),
            Some(r#""C:\Users\x\AppData\Local\Recast\recast.exe" --pad "%1""#)
        );
        assert_eq!(VERB_KEY, r"Software\Classes\*\shell\Recast");
        assert_eq!(
            run_command(r"C:\r\recast.exe"),
            r#""C:\r\recast.exe" --autostart"#
        );
    }

    #[test]
    fn collector_slides_300ms_and_caps_at_2s() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut c = Collector::default();
        assert_eq!(c.deadline(), None);
        c.push(vec!["a".into()], ms(0));
        assert_eq!(c.deadline(), Some(ms(300)));
        c.push(vec!["b".into(), "a".into()], ms(200)); // slides; duplicate dropped
        assert_eq!(c.deadline(), Some(ms(500)));
        assert_eq!(c.take_if_due(ms(499)), None);
        assert_eq!(
            c.take_if_due(ms(500)),
            Some(vec!["a".to_string(), "b".to_string()])
        );
        assert_eq!(c.deadline(), None, "taking resets");
        // A steady trickle is flushed 2 s after the first arrival.
        let mut flushed = None;
        for i in 0..20u64 {
            let now = ms(1000 + i * 150);
            if let Some(v) = c.take_if_due(now) {
                flushed = Some(v);
                break;
            }
            c.push(vec![format!("f{i}")], now);
            if i == 0 {
                assert_eq!(c.deadline(), Some(ms(1300)));
            }
        }
        assert_eq!(flushed.map(|v| v.len()), Some(14)); // f0..f13, flushed at t=3100 >= cap 3000
        c.push(vec![], ms(4000));
        assert_eq!(c.deadline(), None, "an empty push starts nothing");
    }

    #[test]
    fn a_late_flush_merges_with_the_previous_one_within_the_cap() {
        let t0 = Instant::now();
        let prev = vec!["a".to_string(), "b".to_string()];
        let new = || vec!["b".to_string(), "c".to_string()];
        let merged = merge_recent(Some((&prev, t0)), new(), t0 + Duration::from_millis(1500));
        assert_eq!(
            merged,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        let merged = merge_recent(Some((&prev, t0)), new(), t0 + Duration::from_millis(2500));
        assert_eq!(merged, new(), "after the cap it does not merge");
        assert_eq!(merge_recent(None, new(), t0), new());
    }
}
