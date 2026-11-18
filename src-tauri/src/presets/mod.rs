//! The preset catalog (spec §5): data in presets/presets.toml.
pub mod available;
pub mod command;
pub mod naming;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::detect::{canonical, ext_of, Kind};

pub const BUNDLED: &str = include_str!("../../presets/presets.toml");

/// Pandoc reader for an input extension (spec §5.5).
pub fn pandoc_reader(ext: &str) -> String {
    match ext {
        "txt" | "md" | "markdown" => "markdown",
        "htm" | "html" => "html",
        other => other,
    }
    .to_string()
}

#[cfg(test)]
mod catalog_tests;

/// Placeholders the job runner provides; `setting.*` comes from Settings::to_vars.
pub const BUILTIN_PLACEHOLDERS: &[&str] = &["in", "inputs", "in_dir", "out", "out_dir", "reader", "final_stem", "video_kbps"];

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProgressKind {
    Ffmpeg,
    GhostscriptPages,
    #[default]
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutNaming {
    ReplaceExt,
    Suffix(String),
    Folder(String),
    Fixed(String),
}

impl OutNaming {
    pub fn is_folder(&self) -> bool {
        matches!(self, OutNaming::Folder(_))
    }
}

impl TryFrom<String> for OutNaming {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        let nonempty = |t: &str| if t.trim().is_empty() { Err(format!("empty name in out_naming '{s}'")) } else { Ok(t.to_string()) };
        if s == "replace_ext" {
            Ok(OutNaming::ReplaceExt)
        } else if let Some(t) = s.strip_prefix("suffix:") {
            Ok(OutNaming::Suffix(nonempty(t)?))
        } else if let Some(t) = s.strip_prefix("folder:") {
            Ok(OutNaming::Folder(nonempty(t)?))
        } else if let Some(t) = s.strip_prefix("fixed:") {
            Ok(OutNaming::Fixed(nonempty(t)?))
        } else {
            Err(format!("unknown out_naming '{s}'"))
        }
    }
}

impl<'de> Deserialize<'de> for OutNaming {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        OutNaming::try_from(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

fn one() -> usize {
    1
}
fn ok_exit_default() -> Vec<i32> {
    vec![0]
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub id: String,
    pub label: String,
    pub description: String,
    pub icon: String,
    pub kinds: Vec<Kind>,
    #[serde(default = "one")]
    pub min_files: usize,
    /// Hidden when several files are selected (e.g. single image → PDF, replaced by Combine).
    #[serde(default)]
    pub hide_when_multi: bool,
    /// Engine id from engines.toml, and the logical bin to run.
    pub engine: String,
    pub bin: String,
    /// One argv template per pass (Shrink has two).
    pub passes: Vec<Vec<String>>,
    /// Output extension, or "same" for the input's extension.
    pub out_ext: String,
    /// Canonical output format when it differs from canonical(out_ext) (ALAC writes .m4a).
    #[serde(default)]
    pub out_format: Option<String>,
    pub out_naming: OutNaming,
    #[serde(default)]
    pub progress: ProgressKind,
    #[serde(default)]
    pub requires: Vec<String>,
    /// Shrink/Compress: shown even when the output format equals the input's.
    #[serde(default)]
    pub keep_same_format: bool,
    /// Compress: if the result isn't smaller, discard it and report ALREADY_SMALL.
    #[serde(default)]
    pub discard_if_not_smaller: bool,
    /// Hard-link inputs into the temp dir as in-N.<ext> (ImageMagick, Ghostscript).
    #[serde(default)]
    pub stage_inputs: bool,
    /// Extra names the engine writes into the temp dir, moved next to the output.
    #[serde(default)]
    pub extra_outputs: Vec<String>,
    /// Exit codes that mean success.
    #[serde(default = "ok_exit_default")]
    pub ok_exit: Vec<i32>,
}

impl Preset {
    pub fn is_multi(&self) -> bool {
        self.min_files >= 2
    }

    pub fn out_ext_for(&self, input: &Path) -> String {
        if self.out_ext == "same" {
            let e = ext_of(input);
            if e.is_empty() { "bin".into() } else { e }
        } else {
            self.out_ext.clone()
        }
    }

    pub fn out_format_for(&self, input_format: &str) -> String {
        match (&self.out_format, self.out_ext.as_str()) {
            (Some(f), _) => f.clone(),
            (None, "same") => input_format.to_string(),
            (None, ext) => canonical(ext),
        }
    }

    /// Does any pass use this placeholder?
    pub fn uses(&self, key: &str) -> bool {
        self.passes.iter().flatten().any(|a| placeholders(a).map(|k| k.iter().any(|p| p == key)).unwrap_or(false))
    }
}

/// Names inside `{...}` in one argument template.
pub fn placeholders(s: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let end = after.find('}').ok_or_else(|| format!("unclosed placeholder in '{s}'"))?;
        out.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    Ok(out)
}

#[derive(Deserialize, Debug, Clone)]
pub struct Catalog {
    #[serde(rename = "preset")]
    pub presets: Vec<Preset>,
}

impl Catalog {
    pub fn parse(s: &str) -> Result<Self, String> {
        toml::from_str(s).map_err(|e| format!("presets.toml: {e}"))
    }

    pub fn get(&self, id: &str) -> Option<&Preset> {
        self.presets.iter().find(|p| p.id == id)
    }

    /// `engines`: engine id → logical bin names (from the manifest).
    pub fn validate(&self, engines: &HashMap<String, Vec<String>>, setting_keys: &BTreeSet<String>) -> Result<(), Vec<String>> {
        let mut errs = Vec::new();
        let mut seen = HashSet::new();
        for p in &self.presets {
            let id = &p.id;
            if !seen.insert(id.clone()) {
                errs.push(format!("{id}: duplicate id"));
            }
            match engines.get(&p.engine) {
                None => errs.push(format!("{id}: unknown engine '{}'", p.engine)),
                Some(bins) if !bins.contains(&p.bin) => errs.push(format!("{id}: engine '{}' has no bin '{}'", p.engine, p.bin)),
                _ => {}
            }
            if p.kinds.is_empty() {
                errs.push(format!("{id}: kinds is empty"));
            }
            if p.min_files == 0 {
                errs.push(format!("{id}: min_files must be >= 1"));
            }
            if p.ok_exit.is_empty() {
                errs.push(format!("{id}: ok_exit is empty"));
            }
            if p.passes.is_empty() || p.passes.iter().any(|a| a.is_empty()) {
                errs.push(format!("{id}: passes must be non-empty"));
            }
            let mut keys = Vec::new();
            for a in p.passes.iter().flatten().chain(&p.extra_outputs) {
                match placeholders(a) {
                    Ok(k) => keys.extend(k),
                    Err(e) => errs.push(format!("{id}: {e}")),
                }
            }
            for k in &keys {
                if !BUILTIN_PLACEHOLDERS.contains(&k.as_str()) && !setting_keys.contains(k) {
                    errs.push(format!("{id}: unknown placeholder {{{k}}}"));
                }
            }
            let has = |k: &str| keys.iter().any(|x| x == k);
            if has("inputs") && p.min_files < 2 {
                errs.push(format!("{id}: {{inputs}} needs min_files >= 2"));
            }
            if p.min_files >= 2 && !has("inputs") {
                errs.push(format!("{id}: a multi-file preset must use {{inputs}}"));
            }
            if p.out_naming.is_folder() != has("out_dir") || (!p.out_naming.is_folder() && !has("out")) {
                errs.push(format!("{id}: output placeholder doesn't match out_naming"));
            }
        }
        if errs.is_empty() { Ok(()) } else { Err(errs) }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashMap};

    pub(crate) const MINI: &str = r#"
[[preset]]
id = "v-mp3"
label = "MP3"
description = "Audio only · MP3"
icon = "music"
kinds = ["video"]
engine = "ffmpeg"
bin = "ffmpeg"
progress = "ffmpeg"
requires = ["audio_stream"]
out_ext = "mp3"
out_naming = "replace_ext"
passes = [['-i', '{in}', '-b:a', '{setting.quality.mp3_kbps}k', '{out}']]

[[preset]]
id = "pdf-merge"
label = "Merge"
description = "Merge into one PDF"
icon = "merge"
kinds = ["pdf"]
min_files = 2
engine = "qpdf"
bin = "qpdf"
out_ext = "pdf"
out_naming = "fixed:merged"
ok_exit = [0, 3]
passes = [['--empty', '--pages', '{inputs}', '--', '{out}']]
"#;

    fn engines() -> HashMap<String, Vec<String>> {
        HashMap::from([
            ("ffmpeg".to_string(), vec!["ffmpeg".to_string(), "ffprobe".to_string()]),
            ("qpdf".to_string(), vec!["qpdf".to_string()]),
        ])
    }

    fn keys() -> BTreeSet<String> {
        crate::settings::var_keys()
    }

    #[test]
    fn parses_and_validates() {
        let c = Catalog::parse(MINI).unwrap();
        c.validate(&engines(), &keys()).unwrap();
        let m = c.get("pdf-merge").unwrap();
        assert_eq!(m.out_naming, OutNaming::Fixed("merged".into()));
        assert_eq!(m.ok_exit, vec![0, 3]);
        assert_eq!(c.get("v-mp3").unwrap().ok_exit, vec![0]);
        assert_eq!(c.get("v-mp3").unwrap().progress, ProgressKind::Ffmpeg);
    }

    #[test]
    fn out_naming_parsing() {
        assert_eq!(OutNaming::try_from("replace_ext".to_string()).unwrap(), OutNaming::ReplaceExt);
        assert_eq!(OutNaming::try_from("suffix:compressed".to_string()).unwrap(), OutNaming::Suffix("compressed".into()));
        assert_eq!(OutNaming::try_from("folder:pages".to_string()).unwrap(), OutNaming::Folder("pages".into()));
        assert!(OutNaming::try_from("suffix:".to_string()).is_err());
        assert!(OutNaming::try_from("rename".to_string()).is_err());
    }

    #[test]
    fn validation_reports_every_problem() {
        let bad = MINI
            .replace("engine = \"qpdf\"", "engine = \"nope\"")
            .replace("{setting.quality.mp3_kbps}", "{setting.quality.nope}")
            .replace("id = \"pdf-merge\"", "id = \"v-mp3\"");
        let errs = Catalog::parse(&bad).unwrap().validate(&engines(), &keys()).unwrap_err();
        let all = errs.join("\n");
        assert!(all.contains("duplicate id"), "{all}");
        assert!(all.contains("unknown engine 'nope'"), "{all}");
        assert!(all.contains("unknown placeholder {setting.quality.nope}"), "{all}");
    }

    #[test]
    fn inputs_placeholder_requires_multi_file_preset() {
        let bad = MINI.replace("min_files = 2\n", "");
        let errs = Catalog::parse(&bad).unwrap().validate(&engines(), &keys()).unwrap_err();
        assert!(errs.join("\n").contains("{inputs} needs min_files >= 2"));
    }

    #[test]
    fn placeholder_scanner() {
        assert_eq!(placeholders("-sOutputFile={out_dir}\\page-%03d.png").unwrap(), vec!["out_dir"]);
        assert_eq!(placeholders("{in}[0]").unwrap(), vec!["in"]);
        assert!(placeholders("{in").is_err());
    }

    #[test]
    fn out_format_rules() {
        let c = Catalog::parse(MINI).unwrap();
        let p = c.get("v-mp3").unwrap();
        assert_eq!(p.out_format_for("mov"), "mp3");
        assert_eq!(p.out_ext_for(std::path::Path::new("C:/a/b.MOV")), "mp3");
    }
}
