//! Typed IPC (spec §6.11), mirrored in src/shared/ipc.ts.
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

use crate::app_state::AppState;
use crate::engines::{ENGINE_BUSY, EngineRow, Engines};
use crate::history::{History, JobRecord};
use crate::jobs::notify::Notifier;
use crate::jobs::{Events, JobEvent};
use crate::lockutil::lock;
use crate::pad_model::{self, PadModel};
use crate::settings::{self, Settings};

/// Sends job events to every webview and raises notifications.
pub struct TauriEvents {
    app: AppHandle,
    notifier: Mutex<Notifier>,
}

impl TauriEvents {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            notifier: Mutex::new(Notifier::default()),
        }
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
        // Out of the lock before show(): the OS call may be slow.
        let n = lock(&self.notifier).on_event(ev);
        if let Some(n) = n
            && let Err(e) = self
                .app
                .notification()
                .builder()
                .title(&n.title)
                .body(&n.body)
                .show()
        {
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

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn inspect(state: State<'_, AppState>, paths: Vec<String>) -> Result<PadModel, String> {
    let (catalog, engines) = (state.catalog.clone(), state.engines.clone());
    blocking(move || pad_model::build(&catalog, &engines, &paths)).await
}

#[tauri::command]
pub async fn start_jobs(
    state: State<'_, AppState>,
    preset_id: String,
    paths: Vec<String>,
) -> Result<Vec<String>, String> {
    let jobs = state.jobs.clone();
    blocking(move || {
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        jobs.start_jobs(&preset_id, &paths)
    })
    .await
}

#[tauri::command]
pub async fn cancel_job(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let jobs = state.jobs.clone();
    blocking(move || {
        jobs.cancel(&id);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn retry_job(
    state: State<'_, AppState>,
    id: String,
    fallback: bool,
) -> Result<Vec<String>, String> {
    let jobs = state.jobs.clone();
    blocking(move || jobs.retry(&id, fallback)).await
}

#[tauri::command]
pub async fn list_jobs(state: State<'_, AppState>, page: u32) -> Result<Vec<JobListItem>, String> {
    let (history, catalog) = (state.history.clone(), state.catalog.clone());
    blocking(move || {
        let records = lock(&history).list(page, 50).map_err(|e| e.to_string())?;
        Ok(records
            .into_iter()
            .map(|record| JobListItem {
                preset_label: catalog
                    .get(&record.preset_id)
                    .map(|p| p.label.clone())
                    .unwrap_or_default(),
                record,
            })
            .collect())
    })
    .await
}

/// A row with a temp folder still recorded is being (or waiting to be) cleaned up.
fn check_removable(history: &History, id: &str) -> Result<(), String> {
    match history.get(id).map_err(|e| e.to_string())? {
        Some(r) if r.temp_path.is_some() => {
            Err("Recast is still cleaning up this job's files.".into())
        }
        _ => Ok(()),
    }
}

#[tauri::command]
pub async fn remove_job(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let (jobs, history) = (state.jobs.clone(), state.history.clone());
    blocking(move || {
        if jobs.is_active(&id) {
            return Err("Cancel the job before removing it.".into());
        }
        let h = lock(&history);
        check_removable(&h, &id)?;
        h.remove(&id).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn engines_list(state: State<'_, AppState>) -> Result<Vec<EngineRow>, String> {
    let engines = state.engines.clone();
    blocking(move || Ok(engines.list())).await
}

#[tauri::command]
pub fn engine_install(state: State<'_, AppState>, id: String) -> Result<(), String> {
    // An early answer for the common case; Engines::install itself refuses (and emits
    // engine://failed) if a conversion takes the engine in the meantime.
    if state.engines.in_use(&id) {
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
    // Engines::remove refuses with ENGINE_BUSY while a conversion holds a lease.
    let engines = state.engines.clone();
    blocking(move || engines.remove(&id)).await
}

#[tauri::command]
pub async fn engine_verify(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let engines = state.engines.clone();
    blocking(move || engines.verify(&id)).await
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let settings = state.settings.clone();
    blocking(move || Ok(lock(&settings).settings.clone())).await
}

#[tauri::command]
pub async fn set_settings(
    state: State<'_, AppState>,
    patch: serde_json::Value,
) -> Result<Settings, String> {
    let (settings, jobs, file) = (
        state.settings.clone(),
        state.jobs.clone(),
        state.paths.settings(),
    );
    blocking(move || {
        let mut current = lock(&settings);
        let next = settings::apply_patch(&current, patch);
        for w in &next.warnings {
            log::warn!("settings: {w}");
        }
        settings::save(&file, &next).map_err(|e| e.to_string())?;
        *current = next;
        let out = current.settings.clone();
        drop(current); // pump reads settings through the same mutex
        jobs.pump(); // a higher concurrency starts queued jobs now
        Ok(out)
    })
    .await
}

/// The saved output of a job: only a done job whose file still exists qualifies.
fn job_output(history: &History, id: &str) -> Result<PathBuf, String> {
    let rec = history
        .get(id)
        .map_err(|e| e.to_string())?
        .ok_or("That conversion is no longer in the list.")?;
    if rec.status != "done" {
        return Err("That conversion has no saved file.".into());
    }
    let out = PathBuf::from(rec.output.ok_or("That conversion has no saved file.")?);
    if !out.exists() {
        return Err("The converted file has been moved or deleted.".into());
    }
    Ok(out)
}

#[tauri::command]
pub async fn reveal_job(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let history = state.history.clone();
    let out = blocking(move || job_output(&lock(&history), &id)).await?;
    app.opener()
        .reveal_item_in_dir(out)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_job(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let history = state.history.clone();
    let out = blocking(move || job_output(&lock(&history), &id)).await?;
    app.opener()
        .open_path(out.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Shows the licence file of the notice at `index` (as listed by `notices`) in its folder.
#[tauri::command]
pub async fn reveal_notice(
    app: AppHandle,
    state: State<'_, AppState>,
    index: usize,
) -> Result<(), String> {
    let (engines, licence) = (state.engines.clone(), state.sevenzip_licence.clone());
    let file = blocking(move || {
        build_notices(&engines, &licence)
            .into_iter()
            .nth(index)
            .and_then(|n| n.licence_file)
            .ok_or_else(|| "There is no licence file to show.".to_string())
    })
    .await?;
    app.opener()
        .reveal_item_in_dir(file)
        .map_err(|e| e.to_string())
}

fn licence_file(dir: &std::path::Path) -> Option<String> {
    crate::fsutil::walk_files(dir)
        .into_iter()
        .filter(|p| {
            let n = p
                .file_name()
                .map(|n| n.to_string_lossy().to_ascii_uppercase())
                .unwrap_or_default();
            n.starts_with("LICENSE") || n.starts_with("COPYING") || n.starts_with("COPYRIGHT")
        })
        .min_by_key(|p| p.components().count())
        .map(|p| p.to_string_lossy().into_owned())
}

fn build_notices(engines: &Engines, sevenzip_licence: &std::path::Path) -> Vec<Notice> {
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
        licence_file: sevenzip_licence
            .exists()
            .then(|| sevenzip_licence.to_string_lossy().into_owned()),
        bundled: true,
    });
    out
}

#[tauri::command]
pub async fn notices(state: State<'_, AppState>) -> Result<Vec<Notice>, String> {
    let (engines, licence) = (state.engines.clone(), state.sevenzip_licence.clone());
    blocking(move || Ok(build_notices(&engines, &licence))).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::Finish;

    fn history_with(id: &str, status: &str, output: Option<&str>, temp: Option<&str>) -> History {
        let h = History::open_in_memory().unwrap();
        h.insert(id, "image-png", &["C:\\a.jpg".to_string()], 1)
            .unwrap();
        h.finish(
            id,
            &Finish {
                status,
                output,
                ..Default::default()
            },
            2,
        )
        .unwrap();
        h.set_temp(id, temp).unwrap();
        h
    }

    #[test]
    fn rows_with_a_temp_folder_are_not_removable() {
        let h = history_with("a", "failed", None, Some("C:\\x\\.recast-a"));
        assert_eq!(
            check_removable(&h, "a").unwrap_err(),
            "Recast is still cleaning up this job's files."
        );
        h.set_temp("a", None).unwrap();
        assert!(check_removable(&h, "a").is_ok());
        assert!(check_removable(&h, "missing").is_ok());
    }

    #[test]
    fn only_a_done_job_with_an_existing_file_can_be_shown() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("out.png");
        std::fs::write(&file, b"x").unwrap();
        let f = file.to_str().unwrap();
        assert_eq!(
            job_output(&history_with("a", "done", Some(f), None), "a").unwrap(),
            file
        );
        assert!(job_output(&history_with("b", "failed", Some(f), None), "b").is_err());
        assert!(job_output(&history_with("c", "done", None, None), "c").is_err());
        assert!(job_output(&history_with("e", "done", Some(f), None), "zzz").is_err());
        std::fs::remove_file(&file).unwrap();
        assert!(
            job_output(&history_with("d", "done", Some(f), None), "d")
                .unwrap_err()
                .contains("moved or deleted")
        );
    }
}
