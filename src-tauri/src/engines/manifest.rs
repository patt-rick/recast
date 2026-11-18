//! The bundled engine manifest (spec §6.3). Updated only by app releases.
use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

pub const BUNDLED: &str = include_str!("../../engines/engines.toml");

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct EngineEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub platform: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    /// "zip" | "7z" | "nsis" — all unpacked with the bundled 7z.exe.
    pub archive: String,
    pub licence: String,
    pub homepage: String,
    #[serde(default)]
    pub source_url: Option<String>,
    /// Logical bin name → exe file name, located anywhere in the unpacked tree.
    pub bins: BTreeMap<String, String>,
    /// Post-install smoke test: [bin, args…]; must exit 0.
    #[serde(default)]
    pub check: Vec<String>,
    /// "magick_formats": record `magick -list format` after install.
    #[serde(default)]
    pub capabilities: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Manifest {
    #[serde(rename = "engine")]
    pub engines: Vec<EngineEntry>,
}

impl Manifest {
    pub fn parse(s: &str) -> Result<Self, String> {
        toml::from_str(s).map_err(|e| format!("engines.toml: {e}"))
    }

    pub fn get(&self, id: &str) -> Option<&EngineEntry> {
        self.engines.iter().find(|e| e.id == id)
    }

    pub fn bins_map(&self) -> HashMap<String, Vec<String>> {
        self.engines
            .iter()
            .map(|e| (e.id.clone(), e.bins.keys().cloned().collect()))
            .collect()
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errs = Vec::new();
        let mut seen = HashSet::new();
        for e in &self.engines {
            let id = &e.id;
            if !seen.insert(id.clone()) {
                errs.push(format!("{id}: duplicate id"));
            }
            if e.platform != "windows-x64" {
                errs.push(format!("{id}: platform must be windows-x64"));
            }
            if !e.url.starts_with("https://") {
                errs.push(format!("{id}: url must be https"));
            }
            if e.sha256.len() != 64
                || !e
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                errs.push(format!("{id}: sha256 must be 64 lowercase hex chars"));
            }
            if e.size == 0 {
                errs.push(format!("{id}: size must be > 0"));
            }
            if !["zip", "7z", "nsis"].contains(&e.archive.as_str()) {
                errs.push(format!("{id}: unknown archive '{}'", e.archive));
            }
            if e.bins.is_empty() {
                errs.push(format!("{id}: bins is empty"));
            }
            if let Some(b) = e.check.first()
                && !e.bins.contains_key(b)
            {
                errs.push(format!("{id}: check uses unknown bin '{b}'"));
            }
        }
        if errs.is_empty() { Ok(()) } else { Err(errs) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
[[engine]]
id = "qpdf"
name = "qpdf"
version = "12.0.0"
platform = "windows-x64"
url = "https://github.com/qpdf/qpdf/releases/download/v12.0.0/qpdf-12.0.0-msvc64.zip"
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
size = 10
archive = "zip"
licence = "Apache-2.0"
homepage = "https://qpdf.sourceforge.io"

[engine.bins]
qpdf = "qpdf.exe"
"#;

    #[test]
    fn parses_and_validates() {
        let m = Manifest::parse(GOOD).unwrap();
        m.validate().unwrap();
        assert_eq!(m.get("qpdf").unwrap().bins["qpdf"], "qpdf.exe");
        assert_eq!(m.bins_map()["qpdf"], vec!["qpdf".to_string()]);
    }

    #[test]
    fn rejects_http_bad_hash_and_unknown_archive() {
        let bad = GOOD
            .replace("https://", "http://")
            .replace(
                "\"0000000000000000000000000000000000000000000000000000000000000000\"",
                "\"abc\"",
            )
            .replace("\"zip\"", "\"rar\"");
        let errs = Manifest::parse(&bad)
            .unwrap()
            .validate()
            .unwrap_err()
            .join("\n");
        assert!(errs.contains("https"), "{errs}");
        assert!(errs.contains("sha256"), "{errs}");
        assert!(errs.contains("archive"), "{errs}");
    }

    #[test]
    fn bundled_manifest_is_valid_and_covers_the_catalog() {
        let m = Manifest::parse(BUNDLED).unwrap();
        m.validate().unwrap();
        let c = crate::presets::Catalog::parse(crate::presets::BUNDLED).unwrap();
        c.validate(&m.bins_map(), &crate::settings::var_keys())
            .unwrap();
    }
}
