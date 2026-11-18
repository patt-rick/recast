//! Typed IPC (spec §6.11), mirrored in src/shared/ipc.ts.
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

use crate::app_state::AppState;
use crate::engines::EngineRow;
use crate::history::JobRecord;
use crate::jobs::notify::Notifier;
use crate::jobs::{Events, JobEvent};
use crate::pad_model::{self, PadModel};
use crate::settings::{self, Settings};

/// Sends job events to every webview and raises notifications.
pub struct TauriEvents {
    app: AppHandle,
    notifier: Mutex<Notifier>,
}

impl TauriEvents {
    pub fn new(app: AppHandle) -> Self {
        Self { app, notifier: Mutex::new(Notifier::default()) }
    }
}

impl Events for TauriEvents {
    fn emit(&self, ev: &JobEvent) {
        let name = match ev.status.as_str() {
            "queued" => "job://queued",
            "done" | "skipped" => "job://done",
            "failed" => "job://failed",
            "cancelled" => "job://cancelled",
            _ => "job://progress",
        };
        if let Err(e) = self.app.emit(name, ev) {
            log::warn!("emit {name}: {e}");
        }
        if let Some(n) = self.notifier.lock().unwrap().on_event(ev)
            && let Err(e) = self.app.notification().builder().title(&n.title).body(&n.body).show() {
                log::warn!("notification: {e}");
            }
    }
}

#[derive(Serialize)]
pub struct JobListItem {
    #[serde(flatten)]
    pub record: JobRecord,
    pub preset_label: String,
}

#[derive(Serialize)]
pub struct Notice {
    pub name: String,
    pub licence: String,
    pub homepage: String,
    pub source_url: Option<String>,
    pub licence_file: Option<String>,
    /// Shipped inside Recast (7-Zip) rather than downloaded.
    pub bundled: bool,
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn inspect(state: State<'_, AppState>, paths: Vec<String>) -> Result<PadModel, String> {
    let (catalog, engines) = (state.catalog.clone(), state.engines.clone());
    blocking(move || pad_model::build(&catalog, &engines, &paths)).await
}

#[tauri::command]
pub fn start_jobs(state: State<'_, AppState>, preset_id: String, paths: Vec<String>) -> Result<Vec<String>, String> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    state.jobs.start_jobs(&preset_id, &paths)
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, id: String) {
    state.jobs.cancel(&id);
}

#[tauri::command]
pub fn retry_job(state: State<'_, AppState>, id: String, fallback: bool) -> Result<Vec<String>, String> {
    state.jobs.retry(&id, fallback)
}

#[tauri::command]
pub fn list_jobs(state: State<'_, AppState>, page: u32) -> Result<Vec<JobListItem>, String> {
    let records = state.history.lock().unwrap().list(page, 50).map_err(|e| e.to_string())?;
    Ok(records
        .into_iter()
        .map(|record| JobListItem { preset_label: state.catalog.get(&record.preset_id).map(|p| p.label.clone()).unwrap_or_default(), record })
        .collect())
}

#[tauri::command]
pub fn remove_job(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if state.jobs.is_active(&id) {
        return Err("Cancel the job before removing it.".into());
    }
    state.history.lock().unwrap().remove(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn engines_list(state: State<'_, AppState>) -> Result<Vec<EngineRow>, String> {
    let engines = state.engines.clone();
    blocking(move || Ok(engines.list())).await
}

const ENGINE_BUSY: &str = "A conversion is using this converter. Try again when it finishes.";

#[tauri::command]
pub fn engine_install(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if state.jobs.engine_in_use(&id) {
        return Err(ENGINE_BUSY.into());
    }
    let engines = state.engines.clone();
    // Progress/done/failed reach the UI through the Engines listener (app_state.rs).
    std::thread::spawn(move || {
        let _ = engines.install(&id, &mut |_| {});
    });
    Ok(())
}

#[tauri::command]
pub async fn engine_remove(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if state.jobs.engine_in_use(&id) {
        return Err(ENGINE_BUSY.into());
    }
    let engines = state.engines.clone();
    blocking(move || engines.remove(&id)).await
}

#[tauri::command]
pub async fn engine_verify(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let engines = state.engines.clone();
    blocking(move || engines.verify(&id)).await
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().settings.clone()
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, patch: serde_json::Value) -> Result<Settings, String> {
    let mut current = state.settings.lock().unwrap();
    let next = settings::apply_patch(&current, patch);
    for w in &next.warnings {
        log::warn!("settings: {w}");
    }
    settings::save(&state.paths.settings(), &next).map_err(|e| e.to_string())?;
    *current = next;
    let out = current.settings.clone();
    drop(current); // pump reads settings through the same mutex
    state.jobs.pump(); // a higher concurrency starts queued jobs now
    Ok(out)
}

#[tauri::command]
pub fn reveal(app: AppHandle, path: String) -> Result<(), String> {
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> Result<(), String> {
    app.opener().open_path(path, None::<&str>).map_err(|e| e.to_string())
}

fn licence_file(dir: &std::path::Path) -> Option<String> {
    crate::fsutil::walk_files(dir)
        .into_iter()
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().to_ascii_uppercase()).unwrap_or_default();
            n.starts_with("LICENSE") || n.starts_with("COPYING") || n.starts_with("COPYRIGHT")
        })
        .min_by_key(|p| p.components().count())
        .map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn notices(state: State<'_, AppState>) -> Result<Vec<Notice>, String> {
    let engines = state.engines.clone();
    let sevenzip_licence = state.sevenzip_licence.clone();
    blocking(move || {
        let mut out: Vec<Notice> = engines
            .manifest()
            .engines
            .iter()
            .map(|e| Notice {
                name: e.name.clone(),
                licence: e.licence.clone(),
                homepage: e.homepage.clone(),
                source_url: e.source_url.clone(),
                licence_file: licence_file(&engines.root().join(&e.id)),
                bundled: false,
            })
            .collect();
        out.push(Notice {
            name: "7-Zip (7z.exe, 7z.dll)".into(),
            licence: "LGPL-2.1-or-later with BSD-3-Clause and the unRAR restriction".into(),
            homepage: "https://www.7-zip.org".into(),
            source_url: Some("https://www.7-zip.org/download.html".into()),
            licence_file: sevenzip_licence.exists().then(|| sevenzip_licence.to_string_lossy().into_owned()),
            bundled: true,
        });
        Ok(out)
    })
    .await
}
