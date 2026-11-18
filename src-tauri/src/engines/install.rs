//! Fetch → verify SHA-256 → unpack with 7z → locate bins → smoke check → atomic rename.
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::manifest::EngineEntry;
use crate::fsutil::{sha256_file, walk_files};
use crate::platform;

pub const RECORD: &str = ".recast-install.json";
pub const DAMAGED: &str = ".recast-damaged";
pub const CAPS: &str = ".recast-caps.json";

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EngineStatus {
    NotInstalled,
    Installing { pct: f32 },
    Installed { version: String, update_available: bool },
    Damaged,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BinRecord {
    pub rel: String,
    pub sha256: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct InstallRecord {
    pub id: String,
    pub version: String,
    pub bins: BTreeMap<String, BinRecord>,
}

pub trait Fetcher: Send + Sync {
    /// Download `url` into `dest`, resuming if `dest` already holds a prefix.
    fn fetch(&self, url: &str, dest: &Path, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), String>;
}

pub struct HttpFetcher;

impl Fetcher for HttpFetcher {
    fn fetch(&self, url: &str, dest: &Path, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), String> {
        let have = fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
        let client = reqwest::blocking::Client::builder()
            .user_agent("Recast")
            .connect_timeout(Duration::from_secs(30))
            // Blocking reqwest applies this per send() and per body read(): an idle
            // timeout, not a cap on the whole download. A stalled connection errors out.
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;
        let mut req = client.get(url);
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let mut resp = req.send().map_err(|e| e.to_string())?;
        let status = resp.status();
        if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            return Ok(()); // already complete; the checksum decides
        }
        let (mut file, mut done) = if status == reqwest::StatusCode::PARTIAL_CONTENT {
            (OpenOptions::new().append(true).open(dest).map_err(|e| e.to_string())?, have)
        } else if status.is_success() {
            (File::create(dest).map_err(|e| e.to_string())?, 0)
        } else {
            return Err(format!("download failed: HTTP {status}"));
        };
        let total = resp.content_length().map(|l| l + done);
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = resp.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
            progress(done, total);
        }
        Ok(())
    }
}

/// Copies a local file instead of downloading (tests, and the real-engine test cache).
pub struct LocalFetcher {
    pub source: PathBuf,
}

impl Fetcher for LocalFetcher {
    fn fetch(&self, _url: &str, dest: &Path, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), String> {
        let n = fs::copy(&self.source, dest).map_err(|e| e.to_string())?;
        progress(n, Some(n));
        Ok(())
    }
}

pub struct InstallCtx<'a> {
    pub root: &'a Path,
    pub sevenzip: &'a Path,
    pub fetcher: &'a dyn Fetcher,
}

pub fn install(entry: &EngineEntry, ctx: &InstallCtx, progress: &mut dyn FnMut(f32)) -> Result<InstallRecord, String> {
    let work = ctx.root.join(".partial").join(&entry.id);
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let file_name = entry.url.rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or("download");
    let archive = work.join(file_name);
    let staging = work.join("x");

    let result = (|| {
        let size = entry.size.max(1);
        ctx.fetcher.fetch(&entry.url, &archive, &mut |done, total| {
            progress((done as f32 / total.unwrap_or(size).max(1) as f32).min(1.0) * 0.85)
        })?;
        let digest = sha256_file(&archive).map_err(|e| e.to_string())?;
        if digest != entry.sha256 {
            let _ = fs::remove_file(&archive);
            return Err(format!("checksum mismatch for {} (the download was deleted)", entry.name));
        }
        let _ = fs::remove_dir_all(&staging);
        extract(ctx.sevenzip, &archive, &staging)?;
        progress(0.92);
        let record = locate_bins(entry, &staging)?;
        fs::write(staging.join(RECORD), serde_json::to_vec_pretty(&record).unwrap()).map_err(|e| e.to_string())?;
        if let Some((bin, args)) = entry.check.split_first() {
            let exe = staging.join(&record.bins[bin].rel);
            crate::proc::run_capture(&exe, args, Duration::from_secs(30))
                .ok_or_else(|| format!("{} didn't start after unpacking", entry.name))?;
        }
        let engine_dir = ctx.root.join(&entry.id);
        let final_dir = engine_dir.join(&entry.version);
        fs::create_dir_all(&engine_dir).map_err(|e| e.to_string())?;
        if final_dir.exists() {
            fs::remove_dir_all(&final_dir).map_err(|e| e.to_string())?;
        }
        // Defender scans hundreds of fresh exe/dll files; retry the directory rename.
        crate::fsutil::rename_with_retry(&staging, &final_dir, Duration::from_secs(15)).map_err(|e| e.to_string())?;
        for old in fs::read_dir(&engine_dir).into_iter().flatten().flatten() {
            if old.path() != final_dir {
                let _ = fs::remove_dir_all(old.path());
            }
        }
        progress(1.0);
        Ok(record)
    })();

    let _ = fs::remove_dir_all(&staging);
    if result.is_ok() {
        let _ = fs::remove_dir_all(&work);
    }
    // On a network failure the partial archive stays in .partial so the next attempt resumes.
    result
}

fn extract(sevenzip: &Path, archive: &Path, dest: &Path) -> Result<(), String> {
    let out = Command::new(sevenzip)
        .arg("x")
        .arg("-y")
        .arg(format!("-o{}", dest.display()))
        .arg(archive)
        .creation_flags(platform::CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("couldn't run 7-Zip: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("unpacking failed: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

fn locate_bins(entry: &EngineEntry, dir: &Path) -> Result<InstallRecord, String> {
    let files = walk_files(dir);
    let mut bins = BTreeMap::new();
    for (logical, exe) in &entry.bins {
        let found = files
            .iter()
            .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(exe)))
            .min_by_key(|p| p.components().count())
            .ok_or_else(|| format!("{exe} not found in the {} download", entry.name))?;
        let rel = found.strip_prefix(dir).unwrap().to_string_lossy().replace('/', "\\");
        bins.insert(logical.clone(), BinRecord { rel, sha256: sha256_file(found).map_err(|e| e.to_string())? });
    }
    Ok(InstallRecord { id: entry.id.clone(), version: entry.version.clone(), bins })
}

/// The installed version dir (preferring the manifest's version) and its record.
pub fn installed(entry: &EngineEntry, root: &Path) -> Option<(PathBuf, InstallRecord)> {
    let engine_dir = root.join(&entry.id);
    let mut dirs: Vec<PathBuf> = fs::read_dir(&engine_dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort_by_key(|p| p.file_name() != Some(std::ffi::OsStr::new(&entry.version)));
    dirs.into_iter().find_map(|d| {
        let rec: InstallRecord = serde_json::from_slice(&fs::read(d.join(RECORD)).ok()?).ok()?;
        Some((d, rec))
    })
}

pub fn read_status(entry: &EngineEntry, root: &Path) -> EngineStatus {
    match installed(entry, root) {
        None if root.join(&entry.id).exists() => EngineStatus::Damaged,
        None => EngineStatus::NotInstalled,
        Some((dir, rec)) => {
            if dir.join(DAMAGED).exists() || rec.bins.values().any(|b| !dir.join(&b.rel).is_file()) {
                EngineStatus::Damaged
            } else {
                EngineStatus::Installed { update_available: rec.version != entry.version, version: rec.version }
            }
        }
    }
}

/// Re-hash every bin; on mismatch mark the install Damaged.
pub fn verify(entry: &EngineEntry, root: &Path) -> Result<bool, String> {
    let (dir, rec) = installed(entry, root).ok_or("not installed")?;
    let ok = rec.bins.values().all(|b| sha256_file(&dir.join(&b.rel)).map(|h| h == b.sha256).unwrap_or(false));
    if ok {
        let _ = fs::remove_file(dir.join(DAMAGED));
    } else {
        fs::write(dir.join(DAMAGED), b"").map_err(|e| e.to_string())?;
    }
    Ok(ok)
}

pub fn remove(entry: &EngineEntry, root: &Path) -> std::io::Result<()> {
    match fs::remove_dir_all(root.join(&entry.id)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        r => r,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::engines::manifest::EngineEntry;
    use std::collections::BTreeMap;

    pub(crate) fn sevenzip() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bin").join("7z.exe")
    }

    /// A zip holding tool/bin/tool.exe, built with the bundled 7z.exe.
    pub(crate) fn make_archive(dir: &Path) -> PathBuf {
        let src = dir.join("pkg").join("tool").join("bin");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("tool.exe"), b"not really an exe").unwrap();
        let zip = dir.join("tool-1.0.zip");
        let st = std::process::Command::new(sevenzip())
            .args(["a", "-tzip"])
            .arg(&zip)
            .arg(dir.join("pkg").join("*"))
            .output()
            .unwrap();
        assert!(st.status.success(), "{}", String::from_utf8_lossy(&st.stdout));
        zip
    }

    pub(crate) fn entry(zip: &Path, version: &str) -> EngineEntry {
        EngineEntry {
            id: "tool".into(),
            name: "Tool".into(),
            version: version.into(),
            platform: "windows-x64".into(),
            url: "https://example.invalid/tool-1.0.zip".into(),
            sha256: crate::fsutil::sha256_file(zip).unwrap(),
            size: fs::metadata(zip).unwrap().len(),
            archive: "zip".into(),
            licence: "MIT".into(),
            homepage: "https://example.invalid".into(),
            source_url: None,
            bins: BTreeMap::from([("tool".to_string(), "tool.exe".to_string())]),
            check: vec![],
            capabilities: None,
        }
    }

    #[test]
    fn installs_locates_and_reports_status() {
        let d = tempfile::tempdir().unwrap();
        let zip = make_archive(d.path());
        let e = entry(&zip, "1.0");
        let root = d.path().join("engines");
        let fetcher = LocalFetcher { source: zip.clone() };
        let ctx = InstallCtx { root: &root, sevenzip: &sevenzip(), fetcher: &fetcher };
        let mut seen = Vec::new();
        let rec = install(&e, &ctx, &mut |p| seen.push(p)).unwrap();
        assert_eq!(rec.bins["tool"].rel, r"tool\bin\tool.exe");
        assert!(root.join("tool").join("1.0").join(r"tool\bin\tool.exe").is_file());
        assert!(!root.join(".partial").join("tool").exists());
        assert!(seen.last().copied().unwrap_or(0.0) > 0.9);
        assert_eq!(read_status(&e, &root), EngineStatus::Installed { version: "1.0".into(), update_available: false });
        let newer = entry(&zip, "2.0");
        assert_eq!(read_status(&newer, &root), EngineStatus::Installed { version: "1.0".into(), update_available: true });
    }

    #[test]
    fn checksum_mismatch_installs_nothing() {
        let d = tempfile::tempdir().unwrap();
        let zip = make_archive(d.path());
        let mut e = entry(&zip, "1.0");
        e.sha256 = "0".repeat(64);
        let root = d.path().join("engines");
        let fetcher = LocalFetcher { source: zip };
        let err = install(&e, &InstallCtx { root: &root, sevenzip: &sevenzip(), fetcher: &fetcher }, &mut |_| {}).unwrap_err();
        assert!(err.contains("checksum"), "{err}");
        assert!(!root.join("tool").exists());
        assert!(crate::fsutil::walk_files(&root).is_empty(), "the bad download must be deleted");
        assert_eq!(read_status(&e, &root), EngineStatus::NotInstalled);
    }

    #[test]
    fn failing_post_install_check_installs_nothing() {
        let d = tempfile::tempdir().unwrap();
        let zip = make_archive(d.path());
        let mut e = entry(&zip, "1.0");
        e.check = vec!["tool".into(), "-v".into()];
        let root = d.path().join("engines");
        let fetcher = LocalFetcher { source: zip };
        assert!(install(&e, &InstallCtx { root: &root, sevenzip: &sevenzip(), fetcher: &fetcher }, &mut |_| {}).is_err());
        assert!(!root.join("tool").exists());
    }

    #[test]
    fn verify_detects_tampering_and_remove_deletes() {
        let d = tempfile::tempdir().unwrap();
        let zip = make_archive(d.path());
        let e = entry(&zip, "1.0");
        let root = d.path().join("engines");
        let fetcher = LocalFetcher { source: zip };
        install(&e, &InstallCtx { root: &root, sevenzip: &sevenzip(), fetcher: &fetcher }, &mut |_| {}).unwrap();
        assert!(verify(&e, &root).unwrap());
        fs::write(root.join("tool").join("1.0").join(r"tool\bin\tool.exe"), b"tampered").unwrap();
        assert!(!verify(&e, &root).unwrap());
        assert_eq!(read_status(&e, &root), EngineStatus::Damaged);
        remove(&e, &root).unwrap();
        assert_eq!(read_status(&e, &root), EngineStatus::NotInstalled);
    }
}
