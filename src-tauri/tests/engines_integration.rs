//! Real engines (spec §9 "Rust integration" and "Engines").
//! Run: cargo test --features engines --test engines_integration -- --test-threads=1
#![cfg(feature = "engines")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use recast_lib::engines::Engines;
use recast_lib::engines::install::{
    self, EngineStatus, Fetcher, HttpFetcher, InstallCtx, LocalFetcher,
};
use recast_lib::engines::manifest::{BUNDLED, Manifest};
use recast_lib::history::History;
use recast_lib::jobs::{Events, JobEvent, JobManager};
use recast_lib::presets::{BUNDLED as PRESETS, Catalog};
use recast_lib::proc::run_capture;
use recast_lib::settings::Settings;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn sevenzip() -> PathBuf {
    manifest_dir().join("bin").join("7z.exe")
}
fn cache() -> PathBuf {
    manifest_dir().join("target").join("engine-cache")
}

/// Downloads each archive once into the cache, then copies from there.
struct CachedFetcher;
impl Fetcher for CachedFetcher {
    fn fetch(
        &self,
        url: &str,
        dest: &Path,
        max: u64,
        p: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<(), String> {
        let cached = cache()
            .join("downloads")
            .join(url.rsplit('/').next().unwrap());
        if !cached.exists() {
            std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
            let part = cached.with_extension("part");
            HttpFetcher.fetch(url, &part, max, p)?;
            std::fs::rename(&part, &cached).map_err(|e| e.to_string())?;
        }
        LocalFetcher { source: cached }.fetch(url, dest, max, p)
    }
}

fn engines() -> Arc<Engines> {
    static E: OnceLock<Arc<Engines>> = OnceLock::new();
    E.get_or_init(|| {
        let e = Arc::new(Engines::new(
            Manifest::parse(BUNDLED).unwrap(),
            cache().join("engines"),
            sevenzip(),
            Box::new(CachedFetcher),
        ));
        for id in ["ffmpeg", "imagemagick", "pandoc", "qpdf", "ghostscript"] {
            e.ensure(id, &mut |_| {})
                .unwrap_or_else(|err| panic!("{id}: {err}"));
        }
        e
    })
    .clone()
}

fn bin(engine: &str, b: &str) -> PathBuf {
    engines().bin(engine, b).unwrap()
}

fn tool(engine: &str, b: &str, args: &[&str], cwd: &Path) {
    let exe = bin(engine, b);
    let mut cmd = Command::new(&exe);
    cmd.args(args).current_dir(cwd);
    if engine == "imagemagick" {
        cmd.envs(recast_lib::engines::magick_env(&exe));
    }
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{b} {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[derive(Default)]
struct Collect {
    evs: Mutex<Vec<JobEvent>>,
    cv: Condvar,
}
impl Events for Collect {
    fn emit(&self, ev: &JobEvent) {
        self.evs.lock().unwrap().push(ev.clone());
        self.cv.notify_all();
    }
}
impl Collect {
    fn wait_end(&self, id: &str) -> JobEvent {
        let deadline = Instant::now() + Duration::from_secs(600);
        let mut g = self.evs.lock().unwrap();
        loop {
            if let Some(e) = g.iter().find(|e| {
                e.job_id == id
                    && ["done", "skipped", "failed", "cancelled"].contains(&e.status.as_str())
            }) {
                return e.clone();
            }
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "job {id} timed out");
            g = self.cv.wait_timeout(g, left).unwrap().0;
        }
    }
}

struct Rig {
    mgr: JobManager,
    events: Arc<Collect>,
}

fn rig(f: impl Fn(&mut Settings) + Send + Sync + 'static) -> Rig {
    let catalog = Arc::new(Catalog::parse(PRESETS).unwrap());
    let events = Arc::new(Collect::default());
    let mgr = JobManager::new(
        catalog.clone(),
        engines(),
        Arc::new(Mutex::new(History::open_in_memory().unwrap())),
        Arc::new(move || {
            let mut s = Settings::default();
            f(&mut s);
            s
        }),
        events.clone(),
        cache().join("scratch"),
    );
    Rig { mgr, events }
}

impl Rig {
    fn run(&self, preset: &str, inputs: &[&Path]) -> JobEvent {
        let paths: Vec<PathBuf> = inputs.iter().map(|p| p.to_path_buf()).collect();
        let id = self.mgr.start_jobs(preset, &paths).unwrap().remove(0);
        self.events.wait_end(&id)
    }
    fn done(&self, preset: &str, inputs: &[&Path]) -> PathBuf {
        let ev = self.run(preset, inputs);
        assert_eq!(ev.status, "done", "{preset}: {:?}", ev.error);
        PathBuf::from(ev.out_path.unwrap())
    }
}

/// Fixtures with Unicode, `%`, `[ ]` and a leading `-` in their names (spec §9).
struct Fixtures {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

const ODD: &str = "café 日本 %[1] -";

impl Fixtures {
    fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("recast café 日本 ")
            .tempdir()
            .unwrap();
        let root = dir.path().to_path_buf();
        let r = &root;
        let ff = |args: &[&str]| {
            tool(
                "ffmpeg",
                "ffmpeg",
                &[&["-hide_banner", "-loglevel", "error", "-y"], args].concat(),
                r,
            )
        };
        let mk = |args: &[&str]| tool("imagemagick", "magick", args, r);
        ff(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=320x240:rate=25",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
            "v.mp4",
        ]);
        ff(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=duration=2:size=320x240:rate=25",
            "-f",
            "lavfi",
            "-i",
            "sine=duration=2",
            "-c:v",
            "libx265",
            "-pix_fmt",
            "yuv420p10le",
            "-tag:v",
            "hvc1",
            "-c:a",
            "aac",
            "-shortest",
            "h.mov",
        ]);
        ff(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=320x240:rate=25",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "s.mp4",
        ]);
        ff(&[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "a.wav",
        ]);
        ff(&[
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=320x180:d=10,geq=random(1)*255:128:128",
            "-f",
            "lavfi",
            "-i",
            "sine=duration=10",
            "-c:v",
            "libx264",
            "-crf",
            "0",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
            "big.mp4",
        ]);
        mk(&[
            "-size",
            "64x64",
            "xc:none",
            "-fill",
            "red",
            "-draw",
            "circle 32,32 32,8",
            "PNG32:alpha.png",
        ]);
        mk(&[
            "-delay", "10", "-size", "32x32", "xc:red", "xc:blue", "-loop", "0", "anim.gif",
        ]);
        mk(&[
            "-size",
            "200x200",
            "xc:white",
            "xc:gray",
            "xc:black",
            "three.pdf",
        ]);
        mk(&["-size", "20x20", "xc:blue", "pic.png"]);
        if engines()
            .magick_caps()
            .unwrap()
            .contains("magick:write:heic")
        {
            mk(&["rose:", "photo.heic"]);
        }
        std::fs::write(r.join("book.md"), "# Title\n\nHello ![pic](pic.png)\n").unwrap();
        std::fs::write(r.join("note.txt"), "Just *text*.\n").unwrap();
        for f in [
            "v.mp4",
            "h.mov",
            "s.mp4",
            "a.wav",
            "big.mp4",
            "alpha.png",
            "anim.gif",
            "three.pdf",
            "photo.heic",
            "note.txt",
        ] {
            if r.join(f).exists() {
                std::fs::rename(r.join(f), r.join(format!("{ODD}{f}"))).unwrap();
            }
        }
        Self { _dir: dir, root }
    }
    fn p(&self, name: &str) -> PathBuf {
        let odd = self.root.join(format!("{ODD}{name}"));
        if odd.exists() {
            odd
        } else {
            self.root.join(name)
        }
    }
    fn leftovers(&self) -> Vec<PathBuf> {
        recast_lib::fsutil::walk_files(&self.root)
            .into_iter()
            .chain(
                std::fs::read_dir(&self.root)
                    .unwrap()
                    .flatten()
                    .map(|e| e.path()),
            )
            .filter(|p| p.to_string_lossy().contains(".recast-"))
            .collect()
    }
}

fn ffprobe(p: &Path) -> serde_json::Value {
    let out = run_capture(
        &bin("ffmpeg", "ffprobe"),
        [
            std::ffi::OsStr::new("-v"),
            "error".as_ref(),
            "-print_format".as_ref(),
            "json".as_ref(),
            "-show_streams".as_ref(),
            "-show_format".as_ref(),
            p.as_os_str(),
        ],
        Duration::from_secs(30),
    )
    .expect("ffprobe");
    serde_json::from_str(&out).unwrap()
}

fn first_codec(p: &Path, kind: &str) -> String {
    ffprobe(p)["streams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["codec_type"] == kind)
        .unwrap()["codec_name"]
        .as_str()
        .unwrap()
        .to_string()
}

/// `magick identify` on a staged copy: output names contain `%[1]`, which ImageMagick
/// would parse as a property escape (the same reason presets stage their inputs).
fn identify(p: &Path, fmt: &str) -> String {
    let d = tempfile::tempdir().unwrap();
    let ext = p
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    let staged = format!("x.{ext}");
    std::fs::copy(p, d.path().join(&staged)).unwrap();
    let magick = bin("imagemagick", "magick");
    let out = Command::new(&magick)
        .envs(recast_lib::engines::magick_env(&magick))
        .args(["identify", "-format", fmt, &format!("{staged}[0]")])
        .current_dir(d.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "identify {}: {}",
        p.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn qpdf_check(p: &Path) {
    let out = Command::new(bin("qpdf", "qpdf"))
        .arg("--check")
        .arg(p)
        .output()
        .unwrap();
    // 0 = clean, 3 = warnings only
    assert!(
        matches!(out.status.code(), Some(0) | Some(3)),
        "qpdf --check {}: {}",
        p.display(),
        String::from_utf8_lossy(&out.stdout)
    );
}

fn npages(p: &Path) -> u32 {
    run_capture(
        &bin("qpdf", "qpdf"),
        [std::ffi::OsStr::new("--show-npages"), p.as_os_str()],
        Duration::from_secs(30),
    )
    .unwrap()
    .trim()
    .parse()
    .unwrap()
}

#[test]
fn inspect_uses_ffprobe_to_hide_audio_presets_for_silent_video() {
    let fx = Fixtures::new();
    let catalog = Catalog::parse(PRESETS).unwrap();
    let ids = |name: &str| -> Vec<String> {
        let m = recast_lib::pad_model::build(
            &catalog,
            &engines(),
            &[fx.p(name).to_string_lossy().into_owned()],
        )
        .unwrap();
        m.presets.into_iter().map(|p| p.id).collect()
    };
    let silent = ids("s.mp4");
    for gone in ["video-mp3", "video-m4a", "video-opus", "video-wav"] {
        assert!(
            !silent.contains(&gone.to_string()),
            "{gone} shown for a silent video"
        );
    }
    assert!(ids("v.mp4").contains(&"video-mp3".to_string()));
}

#[test]
fn video_presets() {
    let fx = Fixtures::new();
    let r = rig(|_| {});
    let v = fx.p("v.mp4");
    for (preset, codec_kind, codec) in [
        ("video-mp3", "audio", "mp3"),
        ("video-gif", "video", "gif"),
        ("video-webm", "video", "vp9"),
        ("video-m4a", "audio", "aac"),
        ("video-mkv", "video", "h264"),
        ("video-mov", "video", "h264"),
        ("video-opus", "audio", "opus"),
        ("video-wav", "audio", "pcm_s16le"),
    ] {
        assert_eq!(
            first_codec(&r.done(preset, &[&v]), codec_kind),
            codec,
            "{preset}"
        );
    }
    // 10-bit HEVC iPhone-style source → 8-bit yuv420p H.264 (spec §5.1 MP4)
    let mp4 = r.done("video-mp4", &[&fx.p("h.mov")]);
    let s = &ffprobe(&mp4)["streams"][0];
    assert_eq!(
        (s["codec_name"].as_str(), s["pix_fmt"].as_str()),
        (Some("h264"), Some("yuv420p"))
    );
    assert!(fx.leftovers().is_empty());
}

#[test]
fn shrink_hits_the_target_and_parallel_jobs_dont_collide() {
    let fx = Fixtures::new();
    let r = rig(|s| s.shrink.target_mb = 1);
    assert_eq!(
        r.run("video-shrink", &[&fx.p("v.mp4")]).status,
        "skipped",
        "already under 1 MB"
    );
    let big = fx.p("big.mp4");
    let copy = fx.root.join("big copy.mp4");
    std::fs::copy(&big, &copy).unwrap();
    let ids = [
        r.mgr
            .start_jobs("video-shrink", std::slice::from_ref(&big))
            .unwrap()
            .remove(0),
        r.mgr
            .start_jobs("video-shrink", std::slice::from_ref(&copy))
            .unwrap()
            .remove(0),
    ];
    for id in ids {
        let ev = r.events.wait_end(&id);
        assert_eq!(ev.status, "done", "{:?}", ev.error);
        let out = PathBuf::from(ev.out_path.unwrap());
        assert!(
            std::fs::metadata(&out).unwrap().len() <= 1024 * 1024,
            "{} too big",
            out.display()
        );
    }
    assert!(fx.leftovers().is_empty());
}

#[test]
fn audio_presets() {
    let fx = Fixtures::new();
    let r = rig(|_| {});
    let a = fx.p("a.wav");
    for (preset, codec) in [
        ("audio-mp3", "mp3"),
        ("audio-m4a", "aac"),
        ("audio-opus", "opus"),
        ("audio-flac", "flac"),
        ("audio-alac", "alac"),
        ("audio-aiff", "pcm_s16be"),
        ("audio-ogg", "vorbis"),
    ] {
        assert_eq!(
            first_codec(&r.done(preset, &[&a]), "audio"),
            codec,
            "{preset}"
        );
    }
}

#[test]
fn image_presets() {
    let fx = Fixtures::new();
    let r = rig(|_| {});
    let png = fx.p("alpha.png");
    let has_alpha = |p: &Path| !matches!(identify(p, "%A").as_str(), "Undefined" | "False");
    for (preset, magick_fmt, alpha) in [
        ("image-jpg", "JPEG", Some(false)),
        ("image-webp", "WEBP", Some(true)),
        ("image-avif", "AVIF", None),
        ("image-jxl", "JXL", None),
        ("image-ico", "ICO", Some(true)),
        ("image-tiff", "TIFF", None),
        ("image-pdf", "PDF", None),
    ] {
        let out = r.done(preset, &[&png]);
        if magick_fmt != "PDF" {
            assert_eq!(identify(&out, "%m"), magick_fmt, "{preset}");
        }
        if let Some(a) = alpha {
            assert_eq!(has_alpha(&out), a, "{preset} alpha");
        }
    }
    let shrunk = r.done("image-shrink", &[&png]);
    assert!(
        shrunk
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("(compressed).png")
    );
    // animated GIF → exactly one PNG
    let one = r.done("image-png", &[&fx.p("anim.gif")]);
    assert!(one.to_string_lossy().ends_with("anim.png"));
    assert!(!fx.root.join(format!("{ODD}anim-1.png")).exists());
    // HEIC read (if the build writes HEIC we made a fixture)
    if fx.p("photo.heic").exists() {
        r.done("image-jpg", &[&fx.p("photo.heic")]);
    }
    let combined = r.done("image-combine-pdf", &[&png, &fx.p("pic.png")]);
    assert_eq!(npages(&combined), 2);
    assert!(fx.leftovers().is_empty());
}

#[test]
fn pdf_presets() {
    let fx = Fixtures::new();
    let r = rig(|_| {});
    let pdf = fx.p("three.pdf");
    let c = r.run("pdf-compress", &[&pdf]);
    assert!(c.status == "done" || c.status == "skipped", "{:?}", c.error);
    let pages = r.done("pdf-images", &[&pdf]);
    assert_eq!(std::fs::read_dir(&pages).unwrap().count(), 3);
    let split = r.done("pdf-split", &[&pdf]);
    assert_eq!(std::fs::read_dir(&split).unwrap().count(), 3);
    let merged = r.done("pdf-merge", &[&pdf, &pdf]);
    assert_eq!(merged.file_name().unwrap(), "merged.pdf");
    assert_eq!(npages(&merged), 6);
    qpdf_check(&merged);
    for page in std::fs::read_dir(&split).unwrap().flatten() {
        qpdf_check(&page.path());
    }
    if c.status == "done" {
        qpdf_check(Path::new(c.out_path.as_deref().unwrap()));
    }
    assert!(fx.leftovers().is_empty());
}

#[test]
fn text_presets() {
    let fx = Fixtures::new();
    let r = rig(|_| {});
    let md = fx.p("book.md");
    for preset in ["text-epub", "text-docx", "text-odt"] {
        assert!(r.done(preset, &[&md]).is_file(), "{preset}");
    }
    // EPUB as input
    let epub = fx.root.join("book.epub");
    assert!(r.done("text-docx", &[&epub]).is_file());
    let html = r.done("text-html", &[&md]);
    assert!(
        std::fs::read_to_string(&html)
            .unwrap()
            .contains("data:image"),
        "image embedded"
    );
    let docx = fx.root.join("book.docx");
    let back = r.done("text-md", &[&docx]);
    let media = back.with_file_name(format!(
        "{}_media",
        back.file_stem().unwrap().to_string_lossy()
    ));
    assert!(media.is_dir(), "media folder beside {}", back.display());
    assert!(r.done("text-md", &[&fx.p("note.txt")]).is_file());
    assert!(fx.leftovers().is_empty());
}

#[test]
fn paths_over_260_characters() {
    let fx = Fixtures::new();
    let r = rig(|_| {});
    let mut deep = fx.root.clone();
    while deep.as_os_str().len() < 280 {
        deep.push("a-long-folder-name-to-pass-max-path");
    }
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::copy(fx.p("pic.png"), deep.join("pic.png")).unwrap(); // book.md embeds it
    for (name, preset) in [
        ("v.mp4", "video-mp3"),
        ("alpha.png", "image-png"),
        ("three.pdf", "pdf-split"),
        ("three.pdf", "pdf-compress"),
        ("book.md", "text-html"),
    ] {
        let src = deep.join(name);
        if !src.exists() {
            std::fs::copy(fx.p(name), &src).unwrap();
        }
        let ev = r.run(preset, &[&src]);
        assert!(
            ev.status == "done" || ev.status == "skipped",
            "{preset} on a long path: {:?}",
            ev.error
        );
    }
}

#[test]
fn install_rejects_corruption_and_verify_catches_tampering() {
    let e = engines(); // ensures the download cache is filled
    let manifest = Manifest::parse(BUNDLED).unwrap();
    let qpdf = manifest.get("qpdf").unwrap();
    let cached = cache()
        .join("downloads")
        .join(qpdf.url.rsplit('/').next().unwrap());
    let d = tempfile::tempdir().unwrap();
    let bad = d.path().join("bad.zip");
    let mut bytes = std::fs::read(&cached).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xff;
    std::fs::write(&bad, bytes).unwrap();
    let root = d.path().join("engines");
    let fetcher = LocalFetcher { source: bad };
    let err = install::install(
        qpdf,
        &InstallCtx {
            root: &root,
            sevenzip: &sevenzip(),
            fetcher: &fetcher,
            post_unpack: None,
        },
        &mut |_| {},
    )
    .unwrap_err();
    assert!(err.contains("checksum"));
    assert_eq!(
        install::read_status(qpdf, &root),
        EngineStatus::NotInstalled
    );

    // Ghostscript unpacked from its NSIS installer, never run as an installer, works from its folder
    let gs = e.bin("ghostscript", "gswin64c").unwrap();
    assert!(
        run_capture(&gs, ["-v"], Duration::from_secs(30))
            .unwrap()
            .contains("Ghostscript")
    );

    // Verify catches a modified exe in a private copy
    let fetcher = LocalFetcher { source: cached };
    install::install(
        qpdf,
        &InstallCtx {
            root: &root,
            sevenzip: &sevenzip(),
            fetcher: &fetcher,
            post_unpack: None,
        },
        &mut |_| {},
    )
    .unwrap();
    assert!(install::verify(qpdf, &root).unwrap());
    let (dir, rec) = install::installed(qpdf, &root).unwrap();
    let exe = dir.join(&rec.bins["qpdf"].rel);
    let mut b = std::fs::read(&exe).unwrap();
    b.push(0);
    std::fs::write(&exe, b).unwrap();
    assert!(!install::verify(qpdf, &root).unwrap());
}
