//! Typed settings at %APPDATA%\Recast\settings.json (spec §6.6, defaults §10).
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const CURRENT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Settings {
    pub version: u32,
    pub triggers: Triggers,
    pub shell: Shell,
    pub output: Output,
    pub quality: Quality,
    pub gif: Gif,
    pub shrink: Shrink,
    pub jobs: Jobs,
    pub app: App,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Triggers { pub edge_handle: bool, pub edge: String, pub edge_pos: f64, pub desk_pad: bool, pub shake: bool }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Shell { pub explorer_entry: bool }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Output {
    /// "next_to_original" | "folder"
    pub location: String,
    pub folder: String,
    /// Empty means Documents\Recast.
    pub fallback_folder: String,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Quality { pub mp3_kbps: u32, pub aac_kbps: u32, pub jpg_q: u32, pub webp_q: u32, pub avif_q: u32 }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Gif { pub fps: u32, pub width: u32 }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Shrink { pub target_mb: u32, pub image_px: u32 }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Jobs { pub concurrency: u32, pub stall_timeout_s: u32, pub no_progress_cap_min: u32 }
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct App { pub start_with_windows: bool, pub first_run_done: bool }

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            triggers: Triggers { edge_handle: true, edge: "left".into(), edge_pos: 0.5, desk_pad: false, shake: true },
            shell: Shell { explorer_entry: true },
            output: Output { location: "next_to_original".into(), folder: String::new(), fallback_folder: String::new() },
            quality: Quality { mp3_kbps: 192, aac_kbps: 192, jpg_q: 85, webp_q: 82, avif_q: 60 },
            gif: Gif { fps: 12, width: 480 },
            shrink: Shrink { target_mb: 25, image_px: 2048 },
            jobs: Jobs { concurrency: 2, stall_timeout_s: 60, no_progress_cap_min: 10 },
            app: App { start_with_windows: true, first_run_done: false },
        }
    }
}

impl Settings {
    /// Every leaf as `setting.<path>` → string, for preset placeholders.
    pub fn to_vars(&self) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        flatten("setting", &serde_json::to_value(self).expect("settings serialize"), &mut m);
        m
    }

    pub fn fallback_dir(&self) -> PathBuf {
        if self.output.fallback_folder.trim().is_empty() {
            dirs::document_dir().unwrap_or_else(std::env::temp_dir).join("Recast")
        } else {
            PathBuf::from(&self.output.fallback_folder)
        }
    }

    fn sanitize(&mut self, w: &mut Vec<String>) {
        let d = Settings::default();
        macro_rules! range {
            ($f:expr, $def:expr, $lo:expr, $hi:expr, $name:literal) => {
                if !($lo..=$hi).contains(&$f) {
                    w.push(format!("{}: out of range, using default", $name));
                    $f = $def;
                }
            };
        }
        range!(self.quality.mp3_kbps, d.quality.mp3_kbps, 32, 320, "quality.mp3_kbps");
        range!(self.quality.aac_kbps, d.quality.aac_kbps, 32, 320, "quality.aac_kbps");
        range!(self.quality.jpg_q, d.quality.jpg_q, 1, 100, "quality.jpg_q");
        range!(self.quality.webp_q, d.quality.webp_q, 1, 100, "quality.webp_q");
        range!(self.quality.avif_q, d.quality.avif_q, 1, 100, "quality.avif_q");
        range!(self.gif.fps, d.gif.fps, 1, 50, "gif.fps");
        range!(self.gif.width, d.gif.width, 16, 4096, "gif.width");
        range!(self.shrink.target_mb, d.shrink.target_mb, 1, 100_000, "shrink.target_mb");
        range!(self.shrink.image_px, d.shrink.image_px, 16, 16_384, "shrink.image_px");
        range!(self.jobs.concurrency, d.jobs.concurrency, 1, 4, "jobs.concurrency");
        range!(self.jobs.stall_timeout_s, d.jobs.stall_timeout_s, 10, 3600, "jobs.stall_timeout_s");
        range!(self.jobs.no_progress_cap_min, d.jobs.no_progress_cap_min, 1, 240, "jobs.no_progress_cap_min");
        range!(self.triggers.edge_pos, d.triggers.edge_pos, 0.0, 1.0, "triggers.edge_pos");
        if !["left", "right", "top", "bottom"].contains(&self.triggers.edge.as_str()) {
            w.push("triggers.edge: unknown value, using default".into());
            self.triggers.edge = d.triggers.edge;
        }
        if !["next_to_original", "folder"].contains(&self.output.location.as_str()) {
            w.push("output.location: unknown value, using default".into());
            self.output.location = d.output.location;
        }
    }
}

/// Settings plus the raw JSON they came from, so unknown fields survive a save.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub settings: Settings,
    pub raw: Value,
    pub warnings: Vec<String>,
    /// settings.json existed but couldn't be read: never overwrite it.
    pub read_only: bool,
}

pub fn var_keys() -> BTreeSet<String> {
    Settings::default().to_vars().into_keys().collect()
}

pub fn load(path: &Path) -> Loaded {
    let defaults = || from_value(Value::Object(Map::new()));
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return defaults(),
        Err(e) => {
            let mut l = defaults();
            l.warnings.push(format!("settings.json couldn't be read ({e}); using defaults and leaving the file alone"));
            l.read_only = true;
            return l;
        }
    };
    let text = String::from_utf8_lossy(&bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    match serde_json::from_str::<Value>(text) {
        Ok(v) => from_value(v),
        Err(e) => {
            let mut l = defaults();
            let bad = path.with_extension("json.bad");
            match std::fs::rename(path, &bad) {
                Ok(()) => l.warnings.push(format!("settings.json is not valid JSON ({e}); using defaults, original kept as settings.json.bad")),
                Err(re) => {
                    l.warnings.push(format!("settings.json is not valid JSON ({e}) and couldn't be set aside ({re}); using defaults"));
                    l.read_only = true;
                }
            }
            l
        }
    }
}

pub fn from_value(raw: Value) -> Loaded {
    let mut warnings = Vec::new();
    let defaults = serde_json::to_value(Settings::default()).expect("defaults serialize");
    let merged = overlay(&defaults, &raw, "", &mut warnings);
    let mut settings: Settings = serde_json::from_value(merged).expect("overlay keeps default types");
    // v1 has no migrations; later versions transform `raw` here before overlaying.
    settings.version = CURRENT_VERSION;
    settings.sanitize(&mut warnings);
    Loaded { settings, raw, warnings, read_only: false }
}

pub fn apply_patch(current: &Loaded, patch: Value) -> Loaded {
    let mut raw = current.raw.clone();
    deep_merge(&mut raw, &patch);
    let mut l = from_value(raw);
    l.read_only = current.read_only;
    l
}

pub fn save(path: &Path, l: &Loaded) -> std::io::Result<()> {
    if l.read_only {
        return Err(std::io::Error::other("settings.json couldn't be read at startup; not overwriting it"));
    }
    let mut out = l.raw.clone();
    deep_merge(&mut out, &serde_json::to_value(&l.settings).expect("settings serialize"));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&out)?)?;
    std::fs::rename(&tmp, path)
}

/// Take `user`'s value wherever its JSON type matches the default's.
fn overlay(default: &Value, user: &Value, path: &str, w: &mut Vec<String>) -> Value {
    match (default, user) {
        (Value::Object(d), Value::Object(u)) => {
            let mut out = Map::new();
            for (k, dv) in d {
                let p = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
                let v = match u.get(k) {
                    Some(uv) => overlay(dv, uv, &p, w),
                    None => dv.clone(),
                };
                out.insert(k.clone(), v);
            }
            Value::Object(out)
        }
        (Value::Bool(_), Value::Bool(_)) | (Value::String(_), Value::String(_)) => user.clone(),
        (Value::Number(dn), Value::Number(un)) => {
            let ok = if dn.is_f64() { true } else { un.as_u64().is_some_and(|v| v <= u32::MAX as u64) };
            if ok {
                user.clone()
            } else {
                w.push(format!("{path}: not a whole number, using default"));
                default.clone()
            }
        }
        _ => {
            w.push(format!("{path}: wrong type, using default"));
            default.clone()
        }
    }
}

fn deep_merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                deep_merge(b.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (b, p) => *b = p.clone(),
    }
}

fn flatten(prefix: &str, v: &Value, m: &mut BTreeMap<String, String>) {
    match v {
        Value::Object(o) => {
            for (k, v) in o {
                flatten(&format!("{prefix}.{k}"), v, m);
            }
        }
        Value::String(s) => {
            m.insert(prefix.to_string(), s.clone());
        }
        other => {
            m.insert(prefix.to_string(), other.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_match_spec_section_10() {
        let s = Settings::default();
        assert!(s.triggers.edge_handle && !s.triggers.desk_pad && s.triggers.shake);
        assert_eq!(s.triggers.edge, "left");
        assert_eq!(s.triggers.edge_pos, 0.5);
        assert!(s.shell.explorer_entry);
        assert_eq!(s.output.location, "next_to_original");
        assert_eq!((s.quality.mp3_kbps, s.quality.aac_kbps), (192, 192));
        assert_eq!((s.quality.jpg_q, s.quality.webp_q, s.quality.avif_q), (85, 82, 60));
        assert_eq!((s.gif.fps, s.gif.width), (12, 480));
        assert_eq!((s.shrink.target_mb, s.shrink.image_px), (25, 2048));
        assert_eq!(s.jobs.concurrency, 2);
        assert_eq!((s.jobs.stall_timeout_s, s.jobs.no_progress_cap_min), (60, 10));
        assert!(s.app.start_with_windows && !s.app.first_run_done);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let l = load(&dir.path().join("settings.json"));
        assert_eq!(l.settings, Settings::default());
        assert!(l.warnings.is_empty());
    }

    #[test]
    fn wrong_types_and_out_of_range_values_fall_back_with_warnings() {
        let l = from_value(json!({
            "jobs": { "concurrency": 9, "stall_timeout_s": "soon" },
            "quality": { "mp3_kbps": 320 },
            "triggers": { "edge": "diagonal" }
        }));
        assert_eq!(l.settings.jobs.concurrency, 2);
        assert_eq!(l.settings.jobs.stall_timeout_s, 60);
        assert_eq!(l.settings.quality.mp3_kbps, 320);
        assert_eq!(l.settings.triggers.edge, "left");
        assert_eq!(l.warnings.len(), 3, "{:?}", l.warnings);
    }

    #[test]
    fn unknown_fields_survive_a_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"future_flag":true,"quality":{"mp3_kbps":256,"later":1}}"#).unwrap();
        let l = load(&path);
        save(&path, &l).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["future_flag"], json!(true));
        assert_eq!(v["quality"]["later"], json!(1));
        assert_eq!(v["quality"]["mp3_kbps"], json!(256));
        assert_eq!(v["jobs"]["concurrency"], json!(2));
    }

    #[test]
    fn corrupt_file_gives_defaults_and_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{not json").unwrap();
        let l = load(&path);
        assert_eq!(l.settings, Settings::default());
        assert_eq!(l.warnings.len(), 1);
    }

    #[test]
    fn corrupt_file_is_preserved_as_bad() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{not json").unwrap();
        let l = load(&path);
        assert!(!l.read_only);
        assert!(!path.exists());
        assert_eq!(std::fs::read_to_string(dir.path().join("settings.json.bad")).unwrap(), "{not json");
        save(&path, &l).unwrap();
    }

    #[test]
    fn bom_file_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "\u{feff}{\"quality\":{\"mp3_kbps\":256}}").unwrap();
        let l = load(&path);
        assert_eq!(l.settings.quality.mp3_kbps, 256);
        assert!(l.warnings.is_empty(), "{:?}", l.warnings);
    }

    #[test]
    fn unreadable_file_is_read_only_and_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::create_dir(&path).unwrap(); // reading a directory fails with a non-NotFound error
        let l = load(&path);
        assert!(l.read_only);
        assert_eq!(l.warnings.len(), 1);
        assert!(save(&path, &l).is_err());
        assert!(apply_patch(&l, json!({})).read_only);
    }

    #[test]
    fn patch_changes_one_field_and_is_sanitized() {
        let l = from_value(json!({}));
        let l = apply_patch(&l, json!({ "jobs": { "concurrency": 3 } }));
        assert_eq!(l.settings.jobs.concurrency, 3);
        let l = apply_patch(&l, json!({ "jobs": { "concurrency": 0 } }));
        assert_eq!(l.settings.jobs.concurrency, 2);
    }

    #[test]
    fn vars_are_flattened_with_setting_prefix() {
        let v = Settings::default().to_vars();
        assert_eq!(v["setting.quality.mp3_kbps"], "192");
        assert_eq!(v["setting.gif.width"], "480");
        assert!(var_keys().contains("setting.shrink.image_px"));
    }
}
