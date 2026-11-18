//! Thumbnails for the pad's centre disc (spec §4.2, deviation 11). Only installed
//! engines are used; nothing is ever downloaded for a thumbnail.
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use crate::detect::{self, Kind};
use crate::engines::Engines;

/// Formats the webview decodes itself.
const NATIVE: &[&str] = &["png", "jpeg", "gif", "webp", "bmp", "avif", "ico"];
pub const PASSTHROUGH_MAX: u64 = 25 * 1024 * 1024;
const OUTPUT_MAX: usize = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq, Eq)]
pub enum Plan {
    /// Send the file's own bytes.
    Passthrough,
    Ffmpeg,
    Magick,
    Ghostscript,
    /// Show the type glyph.
    Glyph,
}

/// ImageMagick and Ghostscript interpret some filename characters; only plain
/// ASCII paths without %, [ or ] are handed to them.
pub fn plain_path(p: &Path) -> bool {
    p.to_str()
        .is_some_and(|s| s.is_ascii() && !s.contains(['%', '[', ']']) && !s.starts_with('-'))
}

/// `installed(engine_id)`: whether that engine is ready.
pub fn plan(
    kind: Kind,
    format: &str,
    size: u64,
    path: &Path,
    installed: &dyn Fn(&str) -> bool,
) -> Plan {
    match kind {
        Kind::Image if NATIVE.contains(&format) && size <= PASSTHROUGH_MAX => Plan::Passthrough,
        Kind::Image if installed("imagemagick") && plain_path(path) => Plan::Magick,
        Kind::Video if installed("ffmpeg") => Plan::Ffmpeg,
        Kind::Pdf if installed("ghostscript") && plain_path(path) => Plan::Ghostscript,
        _ => Plan::Glyph,
    }
}

fn is_png(b: &[u8]) -> bool {
    b.starts_with(&[0x89, b'P', b'N', b'G'])
}

fn run(program: &Path, args: &[OsString], env: &[(OsString, OsString)]) -> Option<Vec<u8>> {
    let out = crate::proc::run_full(program, args, env, TIMEOUT).ok()?;
    let ok = out.status.is_some_and(|s| s.success());
    (ok && is_png(&out.stdout) && out.stdout.len() <= OUTPUT_MAX).then_some(out.stdout)
}

fn ffmpeg_frame(ffmpeg: &Path, path: &Path, at: &str) -> Option<Vec<u8>> {
    let mut args: Vec<OsString> = [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-ss",
        at,
        "-i",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.push(path.as_os_str().to_owned());
    args.extend(
        [
            "-frames:v",
            "1",
            "-vf",
            "scale=176:-2",
            "-f",
            "image2pipe",
            "-c:v",
            "png",
            "pipe:1",
        ]
        .iter()
        .map(OsString::from),
    );
    run(ffmpeg, &args, &[])
}

/// Thumbnail bytes for `path` (PNG, or the file itself for native formats), or None for the glyph.
pub fn make(engines: &Engines, path: &Path) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    let head = detect::read_head(path).ok()?;
    let (kind, format) = detect::classify(path, &head);
    // Never wait on an install or removal for a thumbnail.
    let installed = |id: &str| engines.is_ready(id) && engines.lease_available(id);
    let plan = plan(kind, &format, meta.len(), path, &installed);
    // Hold the engine while it runs so it can't be removed or replaced underneath us.
    let _lease = match plan {
        Plan::Ffmpeg => Some(engines.acquire("ffmpeg")),
        Plan::Magick => Some(engines.acquire("imagemagick")),
        Plan::Ghostscript => Some(engines.acquire("ghostscript")),
        Plan::Passthrough | Plan::Glyph => None,
    };
    match plan {
        Plan::Passthrough => std::fs::read(path).ok(),
        Plan::Ffmpeg => {
            let ffmpeg = engines.bin("ffmpeg", "ffmpeg")?;
            // Clips shorter than a second have no frame at 1 s.
            ffmpeg_frame(&ffmpeg, path, "1").or_else(|| ffmpeg_frame(&ffmpeg, path, "0"))
        }
        Plan::Magick => {
            let magick = engines.bin("imagemagick", "magick")?;
            let mut first = path.as_os_str().to_owned();
            first.push("[0]");
            let args: Vec<OsString> = vec![
                first,
                "-auto-orient".into(),
                "-thumbnail".into(),
                "176x176".into(),
                "png:-".into(),
            ];
            run(&magick, &args, &crate::engines::magick_env(&magick))
        }
        Plan::Ghostscript => {
            let gs = engines.bin("ghostscript", "gswin64c")?;
            let mut args: Vec<OsString> = [
                "-q",
                "-dSAFER",
                "-dBATCH",
                "-dNOPAUSE",
                "-sDEVICE=png16m",
                "-r48",
                "-dFirstPage=1",
                "-dLastPage=1",
                "-dTextAlphaBits=4",
                "-dGraphicsAlphaBits=4",
                "-sOutputFile=-",
                // Keep Ghostscript's own messages out of the PNG on stdout.
                "-sstdout=%stderr",
            ]
            .iter()
            .map(OsString::from)
            .collect();
            args.push(path.as_os_str().to_owned());
            run(&gs, &args, &[])
        }
        Plan::Glyph => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn plans_by_kind_format_size_and_engines() {
        let p = PathBuf::from(r"C:\x\a.png");
        let all = |_: &str| true;
        let none = |_: &str| false;
        assert_eq!(plan(Kind::Image, "png", 10, &p, &none), Plan::Passthrough);
        assert_eq!(
            plan(Kind::Image, "png", PASSTHROUGH_MAX + 1, &p, &all),
            Plan::Magick
        );
        assert_eq!(
            plan(Kind::Image, "png", PASSTHROUGH_MAX + 1, &p, &none),
            Plan::Glyph
        );
        assert_eq!(plan(Kind::Image, "heif", 10, &p, &all), Plan::Magick);
        assert_eq!(plan(Kind::Image, "heif", 10, &p, &none), Plan::Glyph);
        assert_eq!(plan(Kind::Video, "mov", 10, &p, &all), Plan::Ffmpeg);
        assert_eq!(plan(Kind::Video, "mov", 10, &p, &none), Plan::Glyph);
        assert_eq!(plan(Kind::Pdf, "pdf", 10, &p, &all), Plan::Ghostscript);
        assert_eq!(plan(Kind::Audio, "mp3", 10, &p, &all), Plan::Glyph);
        assert_eq!(plan(Kind::Text, "markdown", 10, &p, &all), Plan::Glyph);
    }

    #[test]
    fn odd_paths_skip_the_filename_interpreting_engines() {
        let all = |_: &str| true;
        assert!(plain_path(Path::new(r"C:\docs\report 1.pdf")));
        assert!(!plain_path(Path::new(r"C:\docs\100% done.pdf")));
        assert!(!plain_path(Path::new(r"C:\docs\[draft].pdf")));
        assert!(!plain_path(Path::new(r"C:\docs\café.pdf")));
        assert_eq!(
            plan(Kind::Pdf, "pdf", 1, Path::new(r"C:\café.pdf"), &all),
            Plan::Glyph
        );
        assert_eq!(
            plan(Kind::Image, "heif", 1, Path::new(r"C:\[x].heic"), &all),
            Plan::Glyph
        );
    }
}
