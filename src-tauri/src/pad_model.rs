//! inspect(paths) → PadModel (spec §6 data flow).
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::detect::{self, FileInfo, Kind};
use crate::engines::Engines;
use crate::engines::install::EngineStatus;
use crate::jobs::EngineHost;
use crate::presets::Catalog;
use crate::presets::available::{Caps, FileFacts, available};

#[derive(Serialize, Clone, Debug)]
pub struct PresetView {
    pub id: String,
    pub label: String,
    pub description: String,
    pub icon: String,
    pub engine: String,
    pub engine_name: String,
    pub engine_status: EngineStatus,
    pub min_files: usize,
    /// Download size of the engine (manifest), for "Getting FFmpeg · 84 MB".
    pub engine_size: u64,
    /// A caution shown in the ring (e.g. GIF from a long video).
    pub warning: Option<String>,
}

/// The preset's caution if any input is longer than its warn_over_s.
pub fn warning_for(preset: &crate::presets::Preset, files: &[FileInfo]) -> Option<String> {
    let limit = preset.warn_over_s?;
    files
        .iter()
        .any(|f| f.duration_s.is_some_and(|d| d > limit))
        .then(|| preset.warn_text.clone())
        .flatten()
}

/// Paths a recent `inspect` returned: the only files `thumbnail` will read (spec deviation 11).
#[derive(Default)]
pub struct RecentPaths(VecDeque<PathBuf>);

impl RecentPaths {
    const CAP: usize = 256;

    pub fn add(&mut self, p: PathBuf) {
        self.0.retain(|q| q != &p);
        self.0.push_back(p);
        while self.0.len() > Self::CAP {
            self.0.pop_front();
        }
    }

    pub fn contains(&self, p: &Path) -> bool {
        self.0.iter().any(|q| q == p)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct PadModel {
    pub files: Vec<FileInfo>,
    pub presets: Vec<PresetView>,
    pub open_in_recast: bool,
}

pub fn build(catalog: &Catalog, engines: &Engines, paths: &[String]) -> Result<PadModel, String> {
    let mut files = Vec::new();
    for p in paths {
        let path = std::path::absolute(p).map_err(|e| e.to_string())?;
        if path.is_dir() {
            continue; // folders are a v1 non-goal (spec §3)
        }
        let mut info = detect::detect(&path, &|q| engines.probe(q))
            .map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
        if info.kind == Kind::Pdf {
            info.pages = engines.pdf_pages(&path);
        }
        files.push(info);
    }
    let facts: Vec<FileFacts> = files.iter().map(FileFacts::from).collect();
    let caps = engines.magick_caps();
    let avail = available(
        catalog,
        &facts,
        &Caps {
            magick: caps.as_ref(),
        },
    );
    let presets = avail
        .presets
        .iter()
        .map(|p| PresetView {
            id: p.id.clone(),
            label: p.label.clone(),
            description: p.description.clone(),
            icon: p.icon.clone(),
            engine: p.engine.clone(),
            engine_name: engines
                .manifest()
                .get(&p.engine)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| p.engine.clone()),
            engine_status: engines.status(&p.engine),
            min_files: p.min_files,
            engine_size: engines
                .manifest()
                .get(&p.engine)
                .map(|e| e.size)
                .unwrap_or(0),
            warning: warning_for(p, &files),
        })
        .collect();
    Ok(PadModel {
        files,
        presets,
        open_in_recast: avail.open_in_recast,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::install::HttpFetcher;
    use crate::engines::manifest::{BUNDLED, Manifest};
    use crate::presets::BUNDLED as PRESETS;

    #[test]
    fn gif_warns_only_for_long_videos() {
        let catalog = Catalog::parse(PRESETS).unwrap();
        let gif = catalog.get("video-gif").unwrap();
        let mut f = FileInfo::bare(
            std::path::Path::new(r"C:\a.mov"),
            Kind::Video,
            "mov".into(),
            1,
        );
        f.duration_s = Some(61.0);
        assert!(
            warning_for(gif, std::slice::from_ref(&f))
                .unwrap()
                .contains("Over a minute")
        );
        f.duration_s = Some(60.0);
        assert_eq!(warning_for(gif, std::slice::from_ref(&f)), None);
        f.duration_s = None;
        assert_eq!(warning_for(gif, &[f]), None);
        let mp3 = catalog.get("video-mp3").unwrap();
        let mut long = FileInfo::bare(
            std::path::Path::new(r"C:\b.mov"),
            Kind::Video,
            "mov".into(),
            1,
        );
        long.duration_s = Some(600.0);
        assert_eq!(warning_for(mp3, &[long]), None);
    }

    #[test]
    fn recent_paths_remember_the_last_256() {
        let mut r = RecentPaths::default();
        for i in 0..300 {
            r.add(std::path::PathBuf::from(format!(r"C:\f{i}.png")));
        }
        assert!(!r.contains(std::path::Path::new(r"C:\f0.png")));
        assert!(r.contains(std::path::Path::new(r"C:\f299.png")));
        assert!(r.contains(std::path::Path::new(r"C:\f44.png")));
        r.add(std::path::PathBuf::from(r"C:\f44.png")); // re-adding refreshes, no duplicate
        assert_eq!(r.len(), 256);
    }

    #[test]
    fn png_gets_image_presets_with_engine_status() {
        let d = tempfile::tempdir().unwrap();
        let engines = Engines::new(
            Manifest::parse(BUNDLED).unwrap(),
            d.path().join("engines"),
            d.path().join("7z.exe"),
            Box::new(HttpFetcher),
        );
        let catalog = Catalog::parse(PRESETS).unwrap();
        let png = d.path().join("a.png");
        std::fs::write(
            &png,
            [
                0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, b'I', b'H', b'D',
                b'R',
            ],
        )
        .unwrap();
        let pdf = d.path().join("b.pdf");
        std::fs::write(&pdf, b"%PDF-1.7\n").unwrap();

        let m = build(&catalog, &engines, &[png.to_string_lossy().into_owned()]).unwrap();
        assert_eq!(m.files[0].kind, Kind::Image);
        assert_eq!(m.presets[0].id, "image-jpg");
        assert_eq!(m.presets[0].engine_name, "ImageMagick");
        assert_eq!(m.presets[0].engine_status, EngineStatus::NotInstalled);
        assert!(m.presets[0].engine_size > 0);
        assert_eq!(m.presets[0].warning, None);

        let mixed = build(
            &catalog,
            &engines,
            &[
                png.to_string_lossy().into_owned(),
                pdf.to_string_lossy().into_owned(),
            ],
        )
        .unwrap();
        assert!(mixed.presets.is_empty() && mixed.open_in_recast);
    }
}
