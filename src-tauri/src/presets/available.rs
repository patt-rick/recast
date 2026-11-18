//! Which presets apply to a set of files (spec §5, §5.6, §6.2).
use std::collections::BTreeSet;

use super::{Catalog, Preset};
use crate::detect::{FileInfo, Kind};

#[derive(Debug, Clone)]
pub struct FileFacts {
    pub kind: Kind,
    pub format: String,
    pub has_audio: Option<bool>,
}

impl From<&FileInfo> for FileFacts {
    fn from(i: &FileInfo) -> Self {
        Self {
            kind: i.kind,
            format: i.format.clone(),
            has_audio: i.has_audio,
        }
    }
}

/// Installed-engine capabilities. `magick: None` = unknown (ImageMagick not installed): show everything.
pub struct Caps<'a> {
    pub magick: Option<&'a BTreeSet<String>>,
}

pub struct Availability<'a> {
    pub presets: Vec<&'a Preset>,
    /// Mixed kinds and nothing covers them all: offer "Open in Recast" (§5.6).
    pub open_in_recast: bool,
}

/// ImageMagick's name for a canonical format.
pub fn magick_name(format: &str) -> &str {
    if format == "heif" { "heic" } else { format }
}

fn applies(p: &Preset, files: &[FileFacts], caps: &Caps) -> bool {
    let n = files.len();
    if n == 0 || files.iter().any(|f| !p.kinds.contains(&f.kind)) {
        return false;
    }
    if n == 1 && p.is_multi() {
        return false;
    }
    if n > 1 && (n < p.min_files || p.hide_when_multi) {
        return false;
    }
    if !p.keep_same_format
        && !p.is_multi()
        && files
            .iter()
            .all(|f| p.out_format_for(&f.format) == f.format)
    {
        return false;
    }
    if files.iter().any(|f| p.skip_formats.contains(&f.format)) {
        return false;
    }
    if p.requires.iter().any(|r| r == "audio_stream")
        && files.iter().any(|f| f.has_audio == Some(false))
    {
        return false;
    }
    if p.engine == "imagemagick"
        && let Some(c) = caps.magick
    {
        for f in files {
            let out = p.out_format_for(&f.format);
            let read = format!("magick:read:{}", magick_name(&f.format));
            let write = format!("magick:write:{}", magick_name(&out));
            if !c.contains(&read) || !c.contains(&write) {
                return false;
            }
        }
    }
    true
}

pub fn available<'a>(catalog: &'a Catalog, files: &[FileFacts], caps: &Caps) -> Availability<'a> {
    let presets: Vec<&Preset> = catalog
        .presets
        .iter()
        .filter(|p| !p.tools_only && applies(p, files, caps))
        .collect();
    let kinds: BTreeSet<Kind> = files.iter().map(|f| f.kind).collect();
    Availability {
        open_in_recast: presets.is_empty() && kinds.len() > 1,
        presets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::{BUNDLED, Catalog};

    fn f(kind: Kind, format: &str, has_audio: Option<bool>) -> FileFacts {
        FileFacts {
            kind,
            format: format.into(),
            has_audio,
        }
    }

    fn ids(a: &Availability) -> Vec<String> {
        a.presets.iter().map(|p| p.id.clone()).collect()
    }

    const NO_CAPS: Caps = Caps { magick: None };

    #[test]
    fn tools_only_presets_never_reach_the_ring() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = ids(&available(&c, &[f(Kind::Pdf, "pdf", None)], &NO_CAPS));
        assert!(a.contains(&"pdf-split".to_string()));
        assert!(!a.contains(&"pdf-extract".to_string()));
    }

    #[test]
    fn mov_with_audio_hides_only_mov() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = available(&c, &[f(Kind::Video, "mov", Some(true))], &NO_CAPS);
        assert_eq!(
            ids(&a),
            [
                "video-mp3",
                "video-mp4",
                "video-gif",
                "video-webm",
                "video-shrink",
                "video-m4a",
                "video-mkv",
                "video-opus",
                "video-wav"
            ]
        );
        assert!(!a.open_in_recast);
    }

    #[test]
    fn mp4_keeps_shrink_but_hides_mp4() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = ids(&available(&c, &[f(Kind::Video, "mp4", None)], &NO_CAPS));
        assert!(!a.contains(&"video-mp4".to_string()));
        assert!(a.contains(&"video-shrink".to_string()));
    }

    #[test]
    fn silent_video_hides_audio_presets() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = ids(&available(
            &c,
            &[f(Kind::Video, "mp4", Some(false))],
            &NO_CAPS,
        ));
        for gone in ["video-mp3", "video-m4a", "video-opus", "video-wav"] {
            assert!(!a.contains(&gone.to_string()), "{gone}");
        }
    }

    #[test]
    fn shrink_is_hidden_for_gif_and_ico() {
        let c = Catalog::parse(BUNDLED).unwrap();
        for fmt in ["gif", "ico"] {
            let a = ids(&available(&c, &[f(Kind::Image, fmt, None)], &NO_CAPS));
            assert!(!a.contains(&"image-shrink".to_string()), "{fmt}");
        }
        let a = ids(&available(&c, &[f(Kind::Image, "png", None)], &NO_CAPS));
        assert!(a.contains(&"image-shrink".to_string()));
    }

    #[test]
    fn alac_input_hides_alac_but_keeps_m4a() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = ids(&available(
            &c,
            &[f(Kind::Audio, "alac", Some(true))],
            &NO_CAPS,
        ));
        assert!(!a.contains(&"audio-alac".to_string()));
        assert!(a.contains(&"audio-m4a".to_string()));
    }

    #[test]
    fn two_pdfs_offer_merge_and_per_file_tools() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = ids(&available(
            &c,
            &[f(Kind::Pdf, "pdf", None), f(Kind::Pdf, "pdf", None)],
            &NO_CAPS,
        ));
        assert_eq!(a, ["pdf-compress", "pdf-images", "pdf-split", "pdf-merge"]);
        let one = ids(&available(&c, &[f(Kind::Pdf, "pdf", None)], &NO_CAPS));
        assert!(!one.contains(&"pdf-merge".to_string()));
    }

    #[test]
    fn several_images_swap_pdf_for_combine() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = ids(&available(
            &c,
            &[f(Kind::Image, "png", None), f(Kind::Image, "jpeg", None)],
            &NO_CAPS,
        ));
        assert!(a.contains(&"image-combine-pdf".to_string()));
        assert!(!a.contains(&"image-pdf".to_string()));
        assert!(
            a.contains(&"image-png".to_string()),
            "png→png is pointless but jpeg→png is not"
        );
    }

    #[test]
    fn mixed_kinds_with_no_common_preset_open_in_recast() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = available(
            &c,
            &[f(Kind::Pdf, "pdf", None), f(Kind::Image, "png", None)],
            &NO_CAPS,
        );
        assert!(a.presets.is_empty());
        assert!(a.open_in_recast);
    }

    #[test]
    fn video_plus_audio_has_no_common_preset() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let a = available(
            &c,
            &[
                f(Kind::Video, "mp4", Some(true)),
                f(Kind::Audio, "flac", Some(true)),
            ],
            &NO_CAPS,
        );
        assert!(a.presets.is_empty());
        assert!(a.open_in_recast);
    }

    #[test]
    fn magick_capabilities_hide_unwritable_formats() {
        let c = Catalog::parse(BUNDLED).unwrap();
        let caps: BTreeSet<String> = [
            "magick:read:heic",
            "magick:read:png",
            "magick:write:png",
            "magick:write:jpeg",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let a = ids(&available(
            &c,
            &[f(Kind::Image, "heif", None)],
            &Caps {
                magick: Some(&caps),
            },
        ));
        assert_eq!(a, ["image-jpg", "image-png"]);
    }
}
