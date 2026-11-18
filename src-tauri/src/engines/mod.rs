//! Engines: download on demand, verify, locate (spec §6.3).
pub mod caps;
pub mod install;
pub mod manifest;

use std::collections::{BTreeSet, HashMap};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::lockutil::{lock, read, wait, write};
use install::{EngineStatus, Fetcher, InstallCtx, InstallRecord, PostUnpack};
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

pub const ENGINE_BUSY: &str = "A conversion is using this converter. Try again when it finishes.";

/// Shared use of an installed engine: while any lease is held the engine can't be
/// removed or replaced. Dropping it releases the lease.
pub struct EngineLease(Option<Box<dyn FnOnce() + Send>>);

impl EngineLease {
    pub fn new(release: impl FnOnce() + Send + 'static) -> Self {
        Self(Some(Box::new(release)))
    }
    /// A lease that guards nothing (test hosts).
    pub fn noop() -> Self {
        Self(None)
    }
}

impl Drop for EngineLease {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            f();
        }
    }
}

#[derive(Default)]
struct Slot {
    readers: usize,
    writer: bool,
}

/// Per-engine readers (conversions) / writer (install, remove) table.
#[derive(Default)]
struct Leases {
    slots: Mutex<HashMap<String, Slot>>,
    cv: Condvar,
}

impl Leases {
    fn acquire(self: &Arc<Self>, id: &str) -> EngineLease {
        let mut g = lock(&self.slots);
        while g.get(id).is_some_and(|s| s.writer) {
            g = wait(&self.cv, g);
        }
        g.entry(id.to_string()).or_default().readers += 1;
        drop(g);
        let me = self.clone();
        let id = id.to_string();
        EngineLease::new(move || {
            if let Some(s) = lock(&me.slots).get_mut(&id) {
                s.readers = s.readers.saturating_sub(1);
            }
            me.cv.notify_all();
        })
    }

    /// Exclusive access. Waits for another install/remove to end; waits for leases
    /// to be released only if `wait_for_readers`, else fails with ENGINE_BUSY.
    fn exclusive(self: &Arc<Self>, id: &str, wait_for_readers: bool) -> Result<Exclusive, String> {
        let mut g = lock(&self.slots);
        loop {
            let s = g.entry(id.to_string()).or_default();
            if s.readers > 0 && !wait_for_readers {
                return Err(ENGINE_BUSY.into());
            }
            if !s.writer && s.readers == 0 {
                s.writer = true;
                return Ok(Exclusive { leases: self.clone(), id: id.to_string() });
            }
            g = wait(&self.cv, g);
        }
    }

    fn in_use(&self, id: &str) -> bool {
        lock(&self.slots).get(id).is_some_and(|s| s.readers > 0)
    }
}

struct Exclusive {
    leases: Arc<Leases>,
    id: String,
}

impl Drop for Exclusive {
    fn drop(&mut self) {
        if let Some(s) = lock(&self.leases.slots).get_mut(&self.id) {
            s.writer = false;
        }
        self.leases.cv.notify_all();
    }
}

pub struct Engines {
    manifest: Manifest,
    root: PathBuf,
    sevenzip: PathBuf,
    fetcher: Box<dyn Fetcher>,
    leases: Arc<Leases>,
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
            leases: Arc::new(Leases::default()),
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
        *write(&self.listener) = Some(f);
    }

    fn emit(&self, e: EngineEvent) {
        if let Some(l) = read(&self.listener).as_ref() {
            l(e);
        }
    }

    fn entry(&self, id: &str) -> Result<&EngineEntry, String> {
        self.manifest.get(id).ok_or_else(|| format!("unknown engine '{id}'"))
    }

    pub fn status(&self, id: &str) -> EngineStatus {
        if let Some(p) = lock(&self.installing).get(id) {
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

    /// Use an engine: blocks while it is being installed or removed.
    pub fn acquire(&self, id: &str) -> EngineLease {
        self.leases.acquire(id)
    }

    /// Is a conversion holding a lease on this engine?
    pub fn in_use(&self, id: &str) -> bool {
        self.leases.in_use(id)
    }

    /// Install if not usable yet (Installed with an update pending still counts as
    /// usable). Replacing a Damaged install waits for running conversions to end.
    pub fn ensure(&self, id: &str, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        let entry = self.entry(id)?.clone();
        if matches!(install::read_status(&entry, &self.root), EngineStatus::Installed { .. }) {
            return Ok(());
        }
        let _x = self.leases.exclusive(id, true)?;
        self.install_locked(&entry, false, progress)
    }

    /// Library → Download / Update / Repair: installs unless the manifest version is
    /// already installed and healthy. Refused while a conversion uses the engine.
    pub fn install(&self, id: &str, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        let entry = self.entry(id)?.clone();
        let _x = match self.leases.exclusive(id, false) {
            Ok(x) => x,
            Err(e) => {
                self.emit(EngineEvent::Failed { id: id.to_string(), error: e.clone() });
                return Err(e);
            }
        };
        self.install_locked(&entry, true, progress)
    }

    /// The caller holds exclusive access to the engine.
    fn install_locked(&self, entry: &EngineEntry, force: bool, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        let id = entry.id.as_str();
        let current = install::read_status(entry, &self.root);
        // ensure(): anything usable is enough. install() (Library): skip only when the
        // manifest version is already installed and healthy, e.g. Download clicked twice.
        let skip = match current {
            EngineStatus::Installed { update_available, .. } => !force || !update_available,
            _ => false,
        };
        if skip {
            return Ok(());
        }
        lock(&self.installing).insert(id.to_string(), 0.0);
        // Even a panicking install must not leave the engine "Installing" forever.
        struct Clear<'a>(&'a Mutex<HashMap<String, f32>>, &'a str);
        impl Drop for Clear<'_> {
            fn drop(&mut self) {
                lock(self.0).remove(self.1);
            }
        }
        let clear = Clear(&self.installing, id);
        let hook: &PostUnpack<'static> = &magick_post_unpack;
        let magick = entry.capabilities.as_deref() == Some("magick_formats");
        let ctx = InstallCtx { root: &self.root, sevenzip: &self.sevenzip, fetcher: &*self.fetcher, post_unpack: magick.then_some(hook) };
        let mut last = Instant::now() - Duration::from_secs(1);
        let res = install::install(entry, &ctx, &mut |p| {
            lock(&self.installing).insert(id.to_string(), p);
            progress(p);
            if last.elapsed() >= Duration::from_millis(150) || p >= 1.0 {
                last = Instant::now();
                self.emit(EngineEvent::Progress { id: id.to_string(), pct: p });
            }
        })
        .map(|_| ());
        drop(clear);
        match &res {
            Ok(()) => self.emit(EngineEvent::Done { id: id.to_string() }),
            Err(e) => {
                log::warn!("engine {id} install failed: {e}");
                self.emit(EngineEvent::Failed { id: id.to_string(), error: e.clone() });
            }
        }
        res
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

    /// Refused while a conversion uses the engine.
    pub fn remove(&self, id: &str) -> Result<(), String> {
        let entry = self.entry(id)?.clone();
        let _x = self.leases.exclusive(id, false)?;
        install::remove(&entry, &self.root).map_err(|e| e.to_string())
    }
}

/// Environment for running ImageMagick: read policy.xml from the exe's own folder.
pub fn magick_env(magick: &Path) -> Vec<(OsString, OsString)> {
    let dir = magick.parent().unwrap_or(Path::new("."));
    vec![(OsString::from("MAGICK_CONFIGURE_PATH"), dir.as_os_str().to_owned())]
}

/// Runs on the staging folder before it is renamed into place: the policy and the
/// capability list are part of the install, so a failure here installs nothing.
fn magick_post_unpack(_entry: &EngineEntry, staging: &Path, rec: &InstallRecord) -> Result<(), String> {
    let magick = staging.join(&rec.bins.get("magick").ok_or("magick.exe missing")?.rel);
    let dir = magick.parent().unwrap_or(staging);
    write_magick_policy(dir).map_err(|e| format!("couldn't write ImageMagick's policy: {e}"))?;
    let out = crate::proc::run_capture_env(&magick, ["-list", "format"], &magick_env(&magick), Duration::from_secs(30))
        .ok_or("`magick -list format` failed")?;
    let caps = caps::parse_magick_formats(&out);
    std::fs::write(staging.join(install::CAPS), serde_json::to_vec(&caps).unwrap()).map_err(|e| e.to_string())
}

/// Restrictive ImageMagick policy: no delegates, no network/script coders, no `@file`
/// paths, and resource limits so a hostile image can't exhaust memory or disk.
pub fn write_magick_policy(dir: &Path) -> std::io::Result<()> {
    const POLICY: &str = r#"<policymap>
  <policy domain="resource" name="memory" value="1GiB"/>
  <policy domain="resource" name="map" value="2GiB"/>
  <policy domain="resource" name="disk" value="4GiB"/>
  <policy domain="resource" name="width" value="32KP"/>
  <policy domain="resource" name="height" value="32KP"/>
  <policy domain="resource" name="area" value="512MP"/>
  <policy domain="delegate" rights="none" pattern="*" />
  <policy domain="coder" rights="none" pattern="{URL,HTTP,HTTPS,FTP,MVG,MSL,SVG,MSVG,TEXT,EPHEMERAL,SHOW,WIN,PLASMA}" />
  <policy domain="path" rights="none" pattern="@*" />
</policymap>
"#;
    std::fs::write(dir.join(install::MAGICK_POLICY), POLICY)
}

#[cfg(test)]
mod tests {
    use super::install::tests::{entry, make_archive, sevenzip};
    use super::install::LocalFetcher;
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn magick_policy_is_written_and_overwrites() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("policy.xml"), "shipped").unwrap();
        write_magick_policy(d.path()).unwrap();
        let t = std::fs::read_to_string(d.path().join("policy.xml")).unwrap();
        assert!(t.contains(r#"pattern="{URL,HTTP,HTTPS,FTP,MVG,MSL,SVG,MSVG,TEXT,EPHEMERAL,SHOW,WIN,PLASMA}""#));
        for limit in [
            r#"name="memory" value="1GiB""#,
            r#"name="map" value="2GiB""#,
            r#"name="disk" value="4GiB""#,
            r#"name="width" value="32KP""#,
            r#"name="height" value="32KP""#,
            r#"name="area" value="512MP""#,
        ] {
            assert!(t.contains(limit), "missing resource limit {limit}");
        }
        assert!(!t.contains("shipped"));
    }

    #[test]
    fn magick_env_points_at_the_exe_folder() {
        let env = magick_env(Path::new(r"C:\e\imagemagick\7.1\magick.exe"));
        assert_eq!(env, [(OsString::from("MAGICK_CONFIGURE_PATH"), OsString::from(r"C:\e\imagemagick\7.1"))]);
    }

    struct Counting(LocalFetcher, Arc<AtomicUsize>);
    impl Fetcher for Counting {
        fn fetch(&self, url: &str, dest: &Path, max: u64, p: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), String> {
            self.1.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(200));
            self.0.fetch(url, dest, max, p)
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

    fn installed_tool(d: &Path) -> Engines {
        let zip = make_archive(d);
        let manifest = Manifest { engines: vec![entry(&zip, "1.0")] };
        let engines = Engines::new(manifest, d.join("engines"), sevenzip(), Box::new(LocalFetcher { source: zip }));
        engines.ensure("tool", &mut |_| {}).unwrap();
        engines
    }

    #[test]
    fn remove_is_refused_while_a_lease_is_held() {
        let d = tempfile::tempdir().unwrap();
        let engines = installed_tool(d.path());
        let lease = engines.acquire("tool");
        assert!(engines.in_use("tool"));
        assert_eq!(engines.remove("tool").unwrap_err(), ENGINE_BUSY);
        assert_eq!(engines.install("tool", &mut |_| {}).unwrap_err(), ENGINE_BUSY);
        assert!(engines.is_ready("tool"));
        drop(lease);
        assert!(!engines.in_use("tool"));
        engines.remove("tool").unwrap();
        assert_eq!(engines.status("tool"), EngineStatus::NotInstalled);
    }

    #[test]
    fn repairing_a_damaged_engine_waits_for_leases() {
        let d = tempfile::tempdir().unwrap();
        let engines = Arc::new(installed_tool(d.path()));
        let lease = engines.acquire("tool");
        std::fs::remove_file(engines.bin("tool", "tool").unwrap()).unwrap();
        assert_eq!(engines.status("tool"), EngineStatus::Damaged);
        let e2 = engines.clone();
        let repair = std::thread::spawn(move || e2.ensure("tool", &mut |_| {}));
        std::thread::sleep(Duration::from_millis(300));
        assert!(!repair.is_finished(), "ensure must wait while a conversion holds the engine");
        drop(lease);
        repair.join().unwrap().unwrap();
        assert!(engines.is_ready("tool"));
        drop(engines.acquire("tool"));
    }
}
