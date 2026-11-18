use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::command::{BuildInput, build_command};
use super::*;

fn sample_ext(p: &Preset) -> &'static str {
    match p.kinds[0] {
        Kind::Video => "mov",
        Kind::Audio => "flac",
        Kind::Image => "png",
        Kind::Pdf => "pdf",
        Kind::Text => "md",
        Kind::Unknown => "bin",
    }
}

/// Engine bins as the manifest will declare them (cross-checked in Task 9).
pub(crate) fn engine_bins() -> HashMap<String, Vec<String>> {
    HashMap::from([
        ("ffmpeg".into(), vec!["ffmpeg".into(), "ffprobe".into()]),
        ("imagemagick".into(), vec!["magick".into()]),
        ("pandoc".into(), vec!["pandoc".into()]),
        ("qpdf".into(), vec!["qpdf".into()]),
        ("ghostscript".into(), vec!["gswin64c".into()]),
    ])
}

#[test]
fn bundled_catalog_is_valid() {
    let c = Catalog::parse(BUNDLED).unwrap();
    c.validate(&engine_bins(), &crate::settings::var_keys())
        .unwrap();
}

fn labels(kind: Kind) -> Vec<String> {
    Catalog::parse(BUNDLED)
        .unwrap()
        .presets
        .iter()
        .filter(|p| !p.tools_only && p.kinds.contains(&kind))
        .map(|p| p.label.clone())
        .collect()
}

#[test]
fn video_ring_order_matches_spec() {
    assert_eq!(
        labels(Kind::Video),
        [
            "MP3", "MP4", "GIF", "WebM", "Shrink", "M4A", "MKV", "MOV", "Opus", "WAV"
        ]
    );
}

#[test]
fn audio_ring_order_matches_spec() {
    assert_eq!(
        labels(Kind::Audio),
        ["MP3", "M4A", "Opus", "FLAC", "WAV", "ALAC", "AIFF", "OGG"]
    );
}

#[test]
fn image_ring_order_matches_spec() {
    assert_eq!(
        labels(Kind::Image),
        [
            "JPG", "PNG", "WebP", "AVIF", "Shrink", "JPEG XL", "ICO", "TIFF", "PDF", "Combine"
        ]
    );
}

#[test]
fn pdf_ring_order_matches_spec() {
    assert_eq!(labels(Kind::Pdf), ["Compress", "Images", "Split", "Merge"]);
}

#[test]
fn text_ring_order_matches_spec() {
    assert_eq!(
        labels(Kind::Text),
        ["EPUB", "DOCX", "HTML", "Markdown", "ODT"]
    );
}

/// Snapshot files in `dir` whose preset id is not in `ids`.
fn stale_snapshots(dir: &Path, ids: &std::collections::HashSet<String>) -> Vec<String> {
    let prefix = "recast_lib__presets__catalog_tests__";
    let mut stale = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(id) = name
            .strip_prefix(prefix)
            .and_then(|n| n.strip_suffix(".snap"))
            && !ids.contains(id)
        {
            stale.push(name);
        }
    }
    stale.sort();
    stale
}

#[test]
fn no_snapshot_outlives_its_preset() {
    let c = Catalog::parse(BUNDLED).unwrap();
    let ids = c.presets.iter().map(|p| p.id.clone()).collect();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/presets/snapshots");
    let stale = stale_snapshots(&dir, &ids);
    assert!(stale.is_empty(), "delete stale snapshots: {stale:?}");
}

#[test]
fn stale_snapshot_check_would_notice() {
    // Guards the guard: a snapshot named like insta's, for a preset that doesn't exist.
    let real = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/presets/snapshots");
    assert!(
        real.join("recast_lib__presets__catalog_tests__video-mp3.snap")
            .is_file(),
        "the naming the check relies on matches what insta writes"
    );
    let d = tempfile::tempdir().unwrap();
    for id in ["video-mp3", "nope"] {
        std::fs::write(
            d.path()
                .join(format!("recast_lib__presets__catalog_tests__{id}.snap")),
            "",
        )
        .unwrap();
    }
    std::fs::write(d.path().join("unrelated.txt"), "").unwrap();
    let ids = ["video-mp3".to_string()].into_iter().collect();
    assert_eq!(
        stale_snapshots(d.path(), &ids),
        ["recast_lib__presets__catalog_tests__nope.snap"]
    );
}

#[test]
fn every_preset_argv_snapshot() {
    let c = Catalog::parse(BUNDLED).unwrap();
    let mut vars = crate::settings::Settings::default().to_vars();
    vars.insert("reader".into(), "markdown".into());
    vars.insert("final_stem".into(), "café 日本 %[1] -x".into());
    vars.insert("video_kbps".into(), "1906".into());
    for p in &c.presets {
        let ext = sample_ext(p);
        let n = p.min_files;
        let inputs: Vec<PathBuf> = (1..=n)
            .map(|i| {
                if p.stage_inputs {
                    PathBuf::from(format!("in-{i}.{ext}"))
                } else {
                    PathBuf::from(format!(r"C:\in\café 日本 %[1] -x{i}.{ext}"))
                }
            })
            .collect();
        let ranges = if p.per_range() {
            crate::presets::ranges::parse("1-2, 4-", None).unwrap()
        } else {
            Vec::new()
        };
        let out_ext = p.out_ext_for(Path::new(&format!("x.{ext}")), "");
        let passes = build_command(
            p,
            &BuildInput {
                inputs: &inputs,
                in_dir: Path::new(r"C:\in"),
                out_ext: &out_ext,
                vars: &vars,
                ranges: &ranges,
            },
        )
        .unwrap();
        let argv: Vec<Vec<String>> = passes
            .iter()
            .map(|a| a.iter().map(|s| s.to_string_lossy().into_owned()).collect())
            .collect();
        insta::assert_debug_snapshot!(p.id.clone(), argv);
    }
}
