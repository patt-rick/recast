//! Builds everything at startup and hands it to Tauri.
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::path::BaseDirectory;
use tauri::{Emitter, Manager};

use crate::engines::install::HttpFetcher;
use crate::engines::manifest::{self, Manifest};
use crate::engines::{EngineEvent, Engines};
use crate::history::History;
use crate::ipc::TauriEvents;
use crate::jobs::{now, JobManager};
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
    catalog.validate(&manifest.bins_map(), &settings::var_keys()).map_err(|e| e.join("; "))?;

    let sevenzip = app.path().resolve("bin/7z.exe", BaseDirectory::Resource)?;
    let sevenzip_licence = app.path().resolve("bin/7zip-License.txt", BaseDirectory::Resource)?;
    let engines = Arc::new(Engines::new(manifest, paths.engines(), sevenzip, Box::new(HttpFetcher)));
    let handle = app.handle().clone();
    engines.set_listener(Arc::new(move |e: EngineEvent| {
        let name = match &e {
            EngineEvent::Progress { .. } => "engine://progress",
            EngineEvent::Done { .. } => "engine://done",
            EngineEvent::Failed { .. } => "engine://failed",
        };
        let _ = handle.emit(name, &e);
    }));

    let history = Arc::new(Mutex::new(History::open(&paths.db())?));
    for tmp in history.lock().unwrap().recover_interrupted(now())? {
        match std::fs::remove_dir_all(&tmp) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => log::warn!("leftover {tmp}: {e}"),
            _ => log::info!("removed leftover temp dir {tmp}"),
        }
    }

    let settings = Arc::new(Mutex::new(loaded));
    let for_jobs = settings.clone();
    let jobs = JobManager::new(
        catalog.clone(),
        engines.clone(),
        history.clone(),
        Arc::new(move || for_jobs.lock().unwrap().settings.clone()),
        Arc::new(TauriEvents::new(app.handle().clone())),
        paths.local.join("tmp"),
    );

    app.manage(AppState { paths, settings, catalog, engines, history, jobs, sevenzip_licence });
    Ok(())
}
