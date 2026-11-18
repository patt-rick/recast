//! Builds everything at startup and hands it to Tauri.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::lockutil::lock;

use tauri::path::BaseDirectory;
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::engines::install::HttpFetcher;
use crate::engines::manifest::{self, Manifest};
use crate::engines::{EngineEvent, Engines};
use crate::history::History;
use crate::ipc::TauriEvents;
use crate::jobs::{JobManager, now};
use crate::paths::Paths;
use crate::presets::{self, Catalog};
use crate::settings;

pub struct AppState {
    pub paths: Paths,
    pub settings: Arc<Mutex<settings::Loaded>>,
    pub catalog: Arc<Catalog>,
    pub engines: Arc<Engines>,
    pub history: Arc<Mutex<History>>,
    pub jobs: JobManager,
    pub sevenzip_licence: PathBuf,
    /// Bundled licence texts (GPL, AGPL, Apache, ImageMagick).
    pub licenses_dir: PathBuf,
}

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let paths = Paths::from_env();
    std::fs::create_dir_all(&paths.local)?;
    std::fs::create_dir_all(&paths.roaming)?;
    crate::logging::init(&paths.logs());
    log::info!("Recast {} starting", app.package_info().version);

    let loaded = settings::load(&paths.settings());
    for w in &loaded.warnings {
        log::warn!("settings: {w}");
    }

    let manifest = Manifest::parse(manifest::BUNDLED)?;
    manifest.validate().map_err(|e| e.join("; "))?;
    let catalog = Arc::new(Catalog::parse(presets::BUNDLED)?);
    catalog
        .validate(&manifest.bins_map(), &settings::var_keys())
        .map_err(|e| e.join("; "))?;

    let sevenzip = app.path().resolve("bin/7z.exe", BaseDirectory::Resource)?;
    let sevenzip_licence = app
        .path()
        .resolve("bin/7zip-License.txt", BaseDirectory::Resource)?;
    let licenses_dir = app.path().resolve("licenses", BaseDirectory::Resource)?;
    let engine_names: HashMap<String, String> = manifest
        .engines
        .iter()
        .map(|e| (e.id.clone(), e.name.clone()))
        .collect();
    let engines = Arc::new(Engines::new(
        manifest,
        paths.engines(),
        sevenzip,
        Box::new(HttpFetcher),
    ));
    let handle = app.handle().clone();
    let dedupe = Mutex::new(FailureNotices::default());
    engines.set_listener(Arc::new(move |e: EngineEvent| {
        let event = match &e {
            EngineEvent::Progress { .. } => "engine://progress",
            EngineEvent::Done { .. } => "engine://done",
            EngineEvent::Failed { .. } => "engine://failed",
        };
        let _ = handle.emit(event, &e);
        if let EngineEvent::Failed { id, error } = &e
            && lock(&dedupe).should_notify(id, error, std::time::Instant::now())
        {
            let name = engine_names.get(id).unwrap_or(id);
            if let Err(err) = handle
                .notification()
                .builder()
                .title(format!("Couldn't download {name}"))
                .body(error)
                .show()
            {
                log::warn!("notification: {err}");
            }
        }
    }));

    let history = Arc::new(Mutex::new(History::open(&paths.db())?));
    let leftovers = lock(&history).recover_interrupted(now())?;
    for tmp in leftovers {
        // Only ever delete Recast's own work folders, whatever the database says.
        if !is_recast_temp(Path::new(&tmp)) {
            log::warn!("not deleting {tmp}: not a Recast temp folder");
            let _ = lock(&history).forget_temp(&tmp);
            continue;
        }
        match std::fs::remove_dir_all(&tmp) {
            // Still tracked, so the next start tries again.
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => log::warn!("leftover {tmp}: {e}"),
            _ => {
                log::info!("removed leftover temp dir {tmp}");
                let _ = lock(&history).forget_temp(&tmp);
            }
        }
    }

    let settings = Arc::new(Mutex::new(loaded));
    let for_jobs = settings.clone();
    let jobs = JobManager::new(
        catalog.clone(),
        engines.clone(),
        history.clone(),
        Arc::new(move || lock(&for_jobs).settings.clone()),
        Arc::new(TauriEvents::new(app.handle().clone())),
        paths.local.join("tmp"),
    );

    app.manage(AppState {
        paths,
        settings,
        catalog,
        engines,
        history,
        jobs,
        sevenzip_licence,
        licenses_dir,
    });
    Ok(())
}

/// Decides which engine failures deserve a notification: never the "busy" refusal,
/// and at most one per engine per minute.
#[derive(Default)]
struct FailureNotices {
    last: HashMap<String, std::time::Instant>,
}

impl FailureNotices {
    const WINDOW: std::time::Duration = std::time::Duration::from_secs(60);

    fn should_notify(&mut self, id: &str, error: &str, at: std::time::Instant) -> bool {
        if error == crate::engines::ENGINE_BUSY {
            return false;
        }
        if let Some(prev) = self.last.get(id)
            && at.saturating_duration_since(*prev) < Self::WINDOW
        {
            return false;
        }
        self.last.insert(id.to_string(), at);
        true
    }
}

/// A job temp dir is always named `.recast-<job id>`.
fn is_recast_temp(p: &Path) -> bool {
    p.file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with(".recast-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_notices_skip_busy_and_dedupe_per_engine() {
        use std::time::{Duration, Instant};
        let mut n = FailureNotices::default();
        let t0 = Instant::now();
        assert!(!n.should_notify("ffmpeg", crate::engines::ENGINE_BUSY, t0));
        assert!(n.should_notify("ffmpeg", "boom", t0));
        assert!(!n.should_notify("ffmpeg", "boom", t0 + Duration::from_secs(59)));
        assert!(n.should_notify("pandoc", "boom", t0 + Duration::from_secs(1)));
        assert!(n.should_notify("ffmpeg", "boom", t0 + Duration::from_secs(61)));
    }

    #[test]
    fn recovery_only_deletes_recast_temp_dirs() {
        assert!(is_recast_temp(Path::new(r"C:\Users\x\Videos\.recast-0b1c")));
        assert!(!is_recast_temp(Path::new(r"C:\Users\x\Videos")));
        assert!(!is_recast_temp(Path::new(r"C:\")));
        assert!(!is_recast_temp(Path::new(r"C:\x\.recast-a\..")));
    }
}
