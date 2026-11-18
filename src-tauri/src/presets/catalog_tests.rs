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

#[test]
fn video_ring_order_matches_spec() {
    let c = Catalog::parse(BUNDLED).unwrap();
    let ids: Vec<&str> = c
        .presets
        .iter()
        .filter(|p| p.kinds.contains(&Kind::Video))
        .map(|p| p.label.as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "MP3", "MP4", "GIF", "WebM", "Shrink", "M4A", "MKV", "MOV", "Opus", "WAV"
        ]
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
        let out_ext = p.out_ext_for(Path::new(&format!("x.{ext}")));
        let passes = build_command(
            p,
            &BuildInput {
                inputs: &inputs,
                in_dir: Path::new(r"C:\in"),
                out_ext: &out_ext,
                vars: &vars,
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
