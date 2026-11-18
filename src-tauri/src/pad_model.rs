//! inspect(paths) → PadModel (spec §6 data flow).
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
