//! Engines: download on demand, verify, locate (spec §6.3).
pub mod caps;
pub mod install;
pub mod manifest;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use install::{EngineStatus, Fetcher, InstallCtx};
use manifest::{EngineEntry, Manifest};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineEvent {
    Progress { id: String, pct: f32 },
    Done { id: String },
    Failed { id: String, error: String },
}

#[derive(Serialize, Clone, Debug)]
pub struct EngineRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub size: u64,
    pub licence: String,
    pub homepage: String,
    pub source_url: Option<String>,
    pub status: EngineStatus,
    pub disk_bytes: u64,
}

type Listener = Arc<dyn Fn(EngineEvent) + Send + Sync>;

pub struct Engines {
    manifest: Manifest,
    root: PathBuf,
    sevenzip: PathBuf,
    fetcher: Box<dyn Fetcher>,
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    installing: Mutex<HashMap<String, f32>>,
    listener: RwLock<Option<Listener>>,
}

impl Engines {
    pub fn new(manifest: Manifest, root: PathBuf, sevenzip: PathBuf, fetcher: Box<dyn Fetcher>) -> Self {
        Self {
            manifest,
            root,
            sevenzip,
            fetcher,
            locks: Mutex::new(HashMap::new()),
            installing: Mutex::new(HashMap::new()),
            listener: RwLock::new(None),
        }
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn set_listener(&self, f: Listener) {
        *self.listener.write().unwrap() = Some(f);
    }

    fn emit(&self, e: EngineEvent) {
        if let Some(l) = self.listener.read().unwrap().as_ref() {
            l(e);
        }
    }

    fn entry(&self, id: &str) -> Result<&EngineEntry, String> {
        self.manifest.get(id).ok_or_else(|| format!("unknown engine '{id}'"))
    }

    pub fn status(&self, id: &str) -> EngineStatus {
        if let Some(p) = self.installing.lock().unwrap().get(id) {
            return EngineStatus::Installing { pct: *p };
        }
        match self.entry(id) {
            Ok(e) => install::read_status(e, &self.root),
            Err(_) => EngineStatus::NotInstalled,
        }
    }

    pub fn is_ready(&self, id: &str) -> bool {
        matches!(self.status(id), EngineStatus::Installed { .. })
    }

    pub fn list(&self) -> Vec<EngineRow> {
        self.manifest
            .engines
            .iter()
            .map(|e| EngineRow {
                id: e.id.clone(),
                name: e.name.clone(),
                version: e.version.clone(),
                size: e.size,
                licence: e.licence.clone(),
                homepage: e.homepage.clone(),
                source_url: e.source_url.clone(),
                status: self.status(&e.id),
                disk_bytes: crate::fsutil::dir_size(&self.root.join(&e.id)),
            })
            .collect()
    }

    /// Install if not usable yet (Installed with an update pending still counts as usable).
    pub fn ensure(&self, id: &str, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        self.install_inner(id, false, progress)
    }

    /// Library → Download / Update / Repair: installs unless the manifest version is
    /// already installed and healthy.
    pub fn install(&self, id: &str, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        self.install_inner(id, true, progress)
    }

    fn install_inner(&self, id: &str, force: bool, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        let entry = self.entry(id)?.clone();
        let lock = self.locks.lock().unwrap().entry(id.to_string()).or_default().clone();
        let _held = lock.lock().unwrap();
        let current = install::read_status(&entry, &self.root);
        // ensure(): anything usable is enough. install() (Library): skip only when the
        // manifest version is already installed and healthy, e.g. Download clicked twice.
        let skip = match current {
            EngineStatus::Installed { update_available, .. } => !force || !update_available,
            _ => false,
        };
        if skip {
            return Ok(());
        }
        self.installing.lock().unwrap().insert(id.to_string(), 0.0);
        let ctx = InstallCtx { root: &self.root, sevenzip: &self.sevenzip, fetcher: &*self.fetcher };
        let mut last = Instant::now() - Duration::from_secs(1);
        let res = install::install(&entry, &ctx, &mut |p| {
            self.installing.lock().unwrap().insert(id.to_string(), p);
            progress(p);
            if last.elapsed() >= Duration::from_millis(150) || p >= 1.0 {
                last = Instant::now();
                self.emit(EngineEvent::Progress { id: id.to_string(), pct: p });
            }
        })
        .and_then(|_| self.write_caps(&entry));
        self.installing.lock().unwrap().remove(id);
        match &res {
            Ok(()) => self.emit(EngineEvent::Done { id: id.to_string() }),
            Err(e) => {
                log::warn!("engine {id} install failed: {e}");
                self.emit(EngineEvent::Failed { id: id.to_string(), error: e.clone() });
            }
        }
        res
    }

    fn write_caps(&self, entry: &EngineEntry) -> Result<(), String> {
        if entry.capabilities.as_deref() != Some("magick_formats") {
            return Ok(());
        }
        let (dir, _) = install::installed(entry, &self.root).ok_or("ImageMagick vanished after install")?;
        let magick = self.bin(&entry.id, "magick").ok_or("magick.exe missing")?;
        let out = crate::proc::run_capture(&magick, ["-list", "format"], Duration::from_secs(30))
            .ok_or("`magick -list format` failed")?;
        let caps = caps::parse_magick_formats(&out);
        std::fs::write(dir.join(install::CAPS), serde_json::to_vec(&caps).unwrap()).map_err(|e| e.to_string())
    }

    /// ImageMagick's recorded format capabilities, if installed.
    pub fn magick_caps(&self) -> Option<BTreeSet<String>> {
        let (dir, _) = install::installed(self.entry("imagemagick").ok()?, &self.root)?;
        serde_json::from_slice(&std::fs::read(dir.join(install::CAPS)).ok()?).ok()
    }

    pub fn bin(&self, id: &str, bin: &str) -> Option<PathBuf> {
        let (dir, rec) = install::installed(self.entry(id).ok()?, &self.root)?;
        rec.bins.get(bin).map(|b| dir.join(&b.rel))
    }

    pub fn verify(&self, id: &str) -> Result<bool, String> {
        install::verify(self.entry(id)?, &self.root)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let entry = self.entry(id)?.clone();
        let lock = self.locks.lock().unwrap().entry(id.to_string()).or_default().clone();
        let _held = lock.lock().unwrap();
        install::remove(&entry, &self.root).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::install::tests::{entry, make_archive, sevenzip};
    use super::install::LocalFetcher;
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Counting(LocalFetcher, Arc<AtomicUsize>);
    impl Fetcher for Counting {
        fn fetch(&self, url: &str, dest: &Path, p: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), String> {
            self.1.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(200));
            self.0.fetch(url, dest, p)
        }
    }

    #[test]
    fn concurrent_ensure_downloads_once_and_reports_events() {
        let d = tempfile::tempdir().unwrap();
        let zip = make_archive(d.path());
        let manifest = Manifest { engines: vec![entry(&zip, "1.0")] };
        let count = Arc::new(AtomicUsize::new(0));
        let engines = Arc::new(Engines::new(manifest, d.path().join("engines"), sevenzip(), Box::new(Counting(LocalFetcher { source: zip }, count.clone()))));
        let events = Arc::new(Mutex::new(Vec::new()));
        let ev2 = events.clone();
        engines.set_listener(Arc::new(move |e| ev2.lock().unwrap().push(e)));
        let a = { let e = engines.clone(); std::thread::spawn(move || e.ensure("tool", &mut |_| {})) };
        let b = { let e = engines.clone(); std::thread::spawn(move || e.ensure("tool", &mut |_| {})) };
        a.join().unwrap().unwrap();
        b.join().unwrap().unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(engines.is_ready("tool"));
        assert!(engines.bin("tool", "tool").unwrap().is_file());
        assert!(events.lock().unwrap().contains(&EngineEvent::Done { id: "tool".into() }));
        assert_eq!(engines.list()[0].status, EngineStatus::Installed { version: "1.0".into(), update_available: false });
    }
}
