//! What a file is (spec §6.1): kind + canonical format, refined by ffprobe.
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Video,
    Audio,
    Image,
    Pdf,
    Text,
    Unknown,
}

const VIDEO: &[&str] = &[
    "mp4", "mkv", "mov", "webm", "avi", "wmv", "flv", "m4v", "mpg", "ts", "3gp",
];
const AUDIO: &[&str] = &[
    "mp3", "m4a", "aac", "wav", "flac", "ogg", "opus", "wma", "aiff", "aif", "alac",
];
const IMAGE: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "avif", "heic", "heif", "jxl", "bmp", "tif", "tiff", "gif", "ico",
];
const TEXT: &[&str] = &[
    "epub", "docx", "odt", "md", "markdown", "html", "htm", "txt", "rtf",
];

pub fn kind_for_ext(ext: &str) -> Kind {
    let e = ext.to_ascii_lowercase();
    let e = e.as_str();
    if VIDEO.contains(&e) {
        Kind::Video
    } else if AUDIO.contains(&e) {
        Kind::Audio
    } else if IMAGE.contains(&e) {
        Kind::Image
    } else if e == "pdf" {
        Kind::Pdf
    } else if TEXT.contains(&e) {
        Kind::Text
    } else {
        Kind::Unknown
    }
}

/// Extension → canonical format (spec §5). Unlisted extensions map to themselves.
pub fn canonical(ext: &str) -> String {
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "jpeg".into(),
        "tif" | "tiff" => "tiff".into(),
        "htm" | "html" => "html".into(),
        "md" | "markdown" => "markdown".into(),
        "aif" | "aiff" => "aiff".into(),
        "heic" | "heif" => "heif".into(),
        other => other.into(),
    }
}

/// Canonical format → a file extension for it (inverse of `canonical` where they differ).
pub fn ext_for_format(format: &str) -> &str {
    match format {
        "jpeg" => "jpg",
        "tiff" => "tif",
        "heif" => "heic",
        other => other,
    }
}

pub fn ext_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn kind_for_mime(mime: &str) -> Kind {
    if mime.starts_with("video/") {
        Kind::Video
    } else if mime.starts_with("audio/") {
        Kind::Audio
    } else if mime.starts_with("image/") {
        Kind::Image
    } else if mime == "application/pdf" {
        Kind::Pdf
    } else {
        Kind::Unknown
    }
}

/// MPEG-TS packets are 188 bytes and each starts with the sync byte 0x47. A capture may
/// begin mid-packet (sync at some offset < 188), and BDAV/M2TS prefixes each packet with
/// 4 bytes (stride 192, first sync at offset 4).
fn looks_like_mpeg_ts(head: &[u8]) -> bool {
    let sync = |i: usize| head.get(i) == Some(&0x47);
    if head.len() < 189 {
        // Too short to see a second packet: only the first sync byte can be checked.
        return sync(0);
    }
    // A run of sync bytes `stride` apart from `start`: every packet visible in the head
    // must start with 0x47 (at least two), so stray 'G's in text don't qualify.
    let run = |start: usize, stride: usize| {
        let mut i = start;
        let mut seen = 0;
        while i < head.len() {
            if !sync(i) {
                return false;
            }
            seen += 1;
            i += stride;
        }
        seen >= 2
    };
    (0..188).any(|k| run(k, 188)) || run(4, 192)
}

/// Kind and canonical format from the extension and the first bytes.
/// The extension wins unless the magic bytes say a different top-level kind;
/// audio vs video containers are left to ffprobe; images take the magic format.
pub fn classify(path: &Path, head: &[u8]) -> (Kind, String) {
    let ext = ext_of(path);
    let mut ext_kind = kind_for_ext(&ext);
    // `.ts` is also TypeScript: only MPEG-TS content counts as video.
    if ext == "ts" && !looks_like_mpeg_ts(head) {
        ext_kind = Kind::Unknown;
    }
    let magic = infer::get(head).map(|t| (kind_for_mime(t.mime_type()), canonical(t.extension())));
    match magic {
        Some((mk, mfmt)) if mk != Kind::Unknown => {
            let av = |k: Kind| matches!(k, Kind::Video | Kind::Audio);
            if ext_kind == Kind::Unknown || (ext_kind != mk && !(av(ext_kind) && av(mk))) {
                (mk, mfmt)
            } else if mk == Kind::Image {
                (Kind::Image, mfmt)
            } else {
                (ext_kind, canonical(&ext))
            }
        }
        _ => (ext_kind, canonical(&ext)),
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Probe {
    pub duration_s: Option<f64>,
    pub has_audio: bool,
    pub has_video: bool,
    pub audio_codec: Option<String>,
}

#[derive(Deserialize)]
struct JProbe {
    #[serde(default)]
    streams: Vec<JStream>,
    format: Option<JFormat>,
}
#[derive(Deserialize)]
struct JStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    disposition: Option<JDisposition>,
}
#[derive(Deserialize)]
struct JDisposition {
    #[serde(default)]
    attached_pic: i32,
}
#[derive(Deserialize)]
struct JFormat {
    duration: Option<String>,
}

/// Parse `ffprobe -v error -print_format json -show_format -show_streams`.
pub fn parse_ffprobe_json(s: &str) -> Option<Probe> {
    let j: JProbe = serde_json::from_str(s).ok()?;
    let mut p = Probe {
        duration_s: j
            .format
            .and_then(|f| f.duration)
            .and_then(|d| d.parse().ok()),
        ..Default::default()
    };
    for st in j.streams {
        let cover = st.disposition.map(|d| d.attached_pic == 1).unwrap_or(false);
        match st.codec_type.as_deref() {
            Some("video") if !cover => p.has_video = true,
            Some("audio") => {
                p.has_audio = true;
                if p.audio_codec.is_none() {
                    p.audio_codec = st.codec_name;
                }
            }
            _ => {}
        }
    }
    Some(p)
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub kind: Kind,
    pub ext: String,
    /// Canonical format (spec §5), e.g. "jpeg", "alac".
    pub format: String,
    pub size: u64,
    pub duration_s: Option<f64>,
    pub has_audio: Option<bool>,
    pub pages: Option<u32>,
}

impl FileInfo {
    pub fn bare(path: &Path, kind: Kind, format: String, size: u64) -> Self {
        Self {
            path: path.to_path_buf(),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            kind,
            ext: ext_of(path),
            format,
            size,
            duration_s: None,
            has_audio: None,
            pages: None,
        }
    }
}

/// Apply ffprobe facts: audio-only video containers become Audio, ALAC m4a becomes "alac".
pub fn refine(info: &mut FileInfo, p: &Probe) {
    info.duration_s = p.duration_s;
    info.has_audio = Some(p.has_audio);
    if info.kind == Kind::Video && !p.has_video && p.has_audio {
        info.kind = Kind::Audio;
    }
    if info.format == "m4a" && p.audio_codec.as_deref() == Some("alac") {
        info.format = "alac".into();
    }
}

/// Bytes read from the start of a file for classification (enough for MPEG-TS's second packet).
pub const HEAD_LEN: usize = 512;

pub fn read_head(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut head = Vec::with_capacity(HEAD_LEN);
    std::fs::File::open(path)?
        .take(HEAD_LEN as u64)
        .read_to_end(&mut head)?;
    Ok(head)
}

/// Detect one file. `probe` runs ffprobe when FFmpeg is installed (else returns None).
pub fn detect(path: &Path, probe: &dyn Fn(&Path) -> Option<Probe>) -> std::io::Result<FileInfo> {
    let meta = std::fs::metadata(path)?;
    let head = read_head(path)?;
    let (kind, format) = classify(path, &head);
    let mut info = FileInfo::bare(path, kind, format, meta.len());
    if matches!(kind, Kind::Video | Kind::Audio)
        && let Some(p) = probe(path)
    {
        refine(&mut info, &p);
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: [u8; 16] = [
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, b'I', b'H', b'D', b'R',
    ];
    const MP4: [u8; 16] = [
        0, 0, 0, 0x18, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm', 0, 0, 2, 0,
    ];

    #[test]
    fn canonical_formats() {
        assert_eq!(canonical("JPG"), "jpeg");
        assert_eq!(canonical("tif"), "tiff");
        assert_eq!(canonical("htm"), "html");
        assert_eq!(canonical("markdown"), "markdown");
        assert_eq!(canonical("md"), "markdown");
        assert_eq!(canonical("aif"), "aiff");
        assert_eq!(canonical("heic"), "heif");
        assert_eq!(canonical("mkv"), "mkv");
    }

    #[test]
    fn extensions_for_formats() {
        assert_eq!(ext_for_format("jpeg"), "jpg");
        assert_eq!(ext_for_format("tiff"), "tif");
        assert_eq!(ext_for_format("heif"), "heic");
        assert_eq!(ext_for_format("png"), "png");
    }

    #[test]
    fn kinds_by_extension() {
        assert_eq!(kind_for_ext("MOV"), Kind::Video);
        assert_eq!(kind_for_ext("opus"), Kind::Audio);
        assert_eq!(kind_for_ext("jxl"), Kind::Image);
        assert_eq!(kind_for_ext("pdf"), Kind::Pdf);
        assert_eq!(kind_for_ext("docx"), Kind::Text);
        assert_eq!(kind_for_ext("exe"), Kind::Unknown);
    }

    #[test]
    fn magic_bytes_correct_a_misnamed_image() {
        assert_eq!(
            classify(Path::new("C:/x/photo.jpg"), &PNG),
            (Kind::Image, "png".to_string())
        );
    }

    #[test]
    fn extension_wins_between_audio_and_video_containers() {
        assert_eq!(
            classify(Path::new("C:/x/song.m4a"), &MP4),
            (Kind::Audio, "m4a".to_string())
        );
        assert_eq!(
            classify(Path::new("C:/x/clip.mp4"), &MP4),
            (Kind::Video, "mp4".to_string())
        );
    }

    #[test]
    fn magic_bytes_rescue_an_unknown_extension() {
        assert_eq!(
            classify(Path::new("C:/x/download.bin"), &PNG),
            (Kind::Image, "png".to_string())
        );
    }

    /// `len` bytes with a 0x47 sync byte every `stride` bytes from `start`, as in a real stream.
    fn packets(len: usize, start: usize, stride: usize) -> Vec<u8> {
        let mut v = vec![0u8; len];
        let mut i = start;
        while i < len {
            v[i] = 0x47;
            i += stride;
        }
        v
    }

    #[test]
    fn ts_is_video_only_when_it_looks_like_mpeg_ts() {
        let ts = packets(400, 0, 188);
        assert_eq!(
            classify(Path::new("C:/x/clip.ts"), &ts),
            (Kind::Video, "ts".to_string())
        );
        // Short head: only the first sync byte can be checked.
        assert_eq!(
            classify(Path::new("C:/x/clip.ts"), &[0x47, 0, 0]).0,
            Kind::Video
        );
        assert_eq!(
            classify(
                Path::new("C:/x/app.ts"),
                b"import { a } from './a';
"
            )
            .0,
            Kind::Unknown
        );
        // 0x47 first but no second sync byte: not a transport stream.
        let mut ts = ts;
        ts[188] = 0;
        assert_eq!(classify(Path::new("C:/x/app.ts"), &ts).0, Kind::Unknown);
        assert_eq!(classify(Path::new("C:/x/empty.ts"), &[]).0, Kind::Unknown);
    }

    #[test]
    fn ts_accepts_offset_starts_and_192_byte_m2ts_packets() {
        let ts = Path::new("C:/x/clip.ts");
        // Starts mid-packet: syncs at 37 and 225.
        assert_eq!(classify(ts, &packets(512, 37, 188)).0, Kind::Video);
        // BDAV: 4-byte prefix, stride 192.
        assert_eq!(classify(ts, &packets(512, 4, 192)).0, Kind::Video);
        // Two 'G's 188 apart but the next packet position isn't a sync byte: text, not TS.
        let mut g = vec![b'x'; 512];
        g[10] = 0x47;
        g[198] = 0x47;
        assert_eq!(classify(ts, &g).0, Kind::Unknown);
        // A lone 0x47 in a long head is not enough.
        let mut lone = vec![0u8; 512];
        lone[10] = 0x47;
        assert_eq!(classify(ts, &lone).0, Kind::Unknown);
        // TypeScript source.
        let src = "import { Greeter } from './g';\nexport const g = new Greeter();\n".repeat(10);
        assert_eq!(classify(ts, src.as_bytes()).0, Kind::Unknown);
    }

    #[test]
    fn detect_reads_enough_head_for_mpeg_ts() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("clip.ts");
        std::fs::write(&p, packets(600, 0, 188)).unwrap();
        assert_eq!(detect(&p, &|_| None).unwrap().kind, Kind::Video);
        let q = d.path().join("app.ts");
        std::fs::write(
            &q,
            b"export const a = 1;
"
            .repeat(30),
        )
        .unwrap();
        assert_eq!(detect(&q, &|_| None).unwrap().kind, Kind::Unknown);
    }

    #[test]
    fn parses_ffprobe_json_ignoring_cover_art() {
        let json = r#"{"streams":[
            {"codec_type":"video","codec_name":"mjpeg","disposition":{"attached_pic":1}},
            {"codec_type":"audio","codec_name":"alac","disposition":{"attached_pic":0}}],
            "format":{"duration":"134.5"}}"#;
        let p = parse_ffprobe_json(json).unwrap();
        assert_eq!(
            p,
            Probe {
                duration_s: Some(134.5),
                has_audio: true,
                has_video: false,
                audio_codec: Some("alac".into())
            }
        );
    }

    #[test]
    fn refine_turns_audio_only_mov_into_audio_and_alac_m4a_into_alac() {
        let mut mov = FileInfo::bare(Path::new("C:/x/voice.mov"), Kind::Video, "mov".into(), 10);
        refine(
            &mut mov,
            &Probe {
                duration_s: Some(3.0),
                has_audio: true,
                has_video: false,
                audio_codec: Some("aac".into()),
            },
        );
        assert_eq!(mov.kind, Kind::Audio);
        assert_eq!(mov.duration_s, Some(3.0));

        let mut m4a = FileInfo::bare(Path::new("C:/x/song.m4a"), Kind::Audio, "m4a".into(), 10);
        refine(
            &mut m4a,
            &Probe {
                duration_s: None,
                has_audio: true,
                has_video: false,
                audio_codec: Some("alac".into()),
            },
        );
        assert_eq!(m4a.format, "alac");
        assert_eq!(m4a.has_audio, Some(true));
    }
}
