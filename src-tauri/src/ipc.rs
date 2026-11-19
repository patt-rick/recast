//! Typed IPC (spec §6.11), mirrored in src/shared/ipc.ts.
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::app_state::AppState;
use crate::engines::{ENGINE_BUSY, EngineRow, Engines};
use crate::history::{History, JobRecord};
use crate::jobs::notify::{Notifier, unless_watching};
use crate::jobs::{Events, JobEvent};
use crate::lockutil::lock;
use crate::pad_model::{self, PadModel};
use crate::settings::Settings;

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

impl TauriEvents {
    fn companion_in_front(&self) -> bool {
        self.app
            .get_webview_window("companion")
            .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
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
        let n = unless_watching(n, self.companion_in_front());
        if let Some(n) = n {
            crate::toast::show(&self.app, &n.title, &n.body, n.target);
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
    /// Where the licence text lives on disk; read by `notice_text`, never sent to the webview.
    #[serde(skip)]
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
    let (catalog, engines, recent) = (
        state.catalog.clone(),
        state.engines.clone(),
        state.recent.clone(),
    );
    blocking(move || {
        let model = pad_model::build(&catalog, &engines, &paths)?;
        let mut r = lock(&recent);
        for f in &model.files {
            r.add(f.path.clone());
        }
        Ok(model)
    })
    .await
}

/// Thumbnail bytes for a file a recent `inspect` returned (empty: show the glyph).
#[tauri::command]
pub async fn thumbnail(
    state: State<'_, AppState>,
    path: String,
) -> Result<tauri::ipc::Response, String> {
    let (engines, recent) = (state.engines.clone(), state.recent.clone());
    let bytes = blocking(move || {
        let p = std::path::absolute(&path).map_err(|e| e.to_string())?;
        if !lock(&recent).contains(&p) {
            return Err("That file wasn't dropped on Recast.".to_string());
        }
        Ok(crate::thumbs::make(&engines, &p).unwrap_or_default())
    })
    .await?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// First run (§4.6): pulse the edge handle (and label it) while the welcome points at it.
#[tauri::command]
pub async fn highlight_handle(app: AppHandle, on: bool) -> Result<(), String> {
    crate::triggers::highlight(&app, on);
    Ok(())
}

#[tauri::command]
pub async fn pad_ready(app: AppHandle) -> Result<(), String> {
    crate::pad::ready(&app);
    Ok(())
}

/// Show and focus the pad for this open (click mode). Ignored if `generation` is stale.
#[tauri::command]
pub async fn pad_show(app: AppHandle, generation: u64) -> Result<(), String> {
    crate::pad::show(&app, generation);
    Ok(())
}

/// Hide the pad. Ignored unless `generation` is the current open.
#[tauri::command]
pub async fn pad_close(app: AppHandle, generation: u64) -> Result<(), String> {
    crate::pad::close_if_current(&app, generation);
    Ok(())
}

#[tauri::command]
pub async fn pad_track_cursor(app: AppHandle, on: bool, generation: u64) -> Result<(), String> {
    crate::pad::track_cursor_for(&app, on, generation);
    Ok(())
}

/// The screen under the pad window, in the backdrop wire format (spec §6.8).
/// Only the pad may ask, and only for its own rectangle.
#[tauri::command]
pub async fn capture_backdrop(
    window: tauri::WebviewWindow,
) -> Result<tauri::ipc::Response, String> {
    if window.label() != crate::pad::LABEL {
        return Err("Only the pad can capture its backdrop.".into());
    }
    let pos = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let max = crate::backdrop::MAX_SIDE;
    if size.width == 0 || size.height == 0 || size.width > max || size.height > max {
        return Err(format!("bad capture size {}x{}", size.width, size.height));
    }
    let rect = crate::platform::ScreenRect {
        x: pos.x,
        y: pos.y,
        w: size.width,
        h: size.height,
    };
    let bytes = blocking(move || {
        let bgra = crate::platform::capture_region(rect).map_err(|e| e.to_string())?;
        crate::backdrop::encode(rect.w, rect.h, &bgra)
    })
    .await
    .inspect_err(|e| log::warn!("backdrop capture failed: {e}"))?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub async fn start_jobs(
    state: State<'_, AppState>,
    preset_id: String,
    paths: Vec<String>,
    options: Option<crate::jobs::JobOptions>,
) -> Result<Vec<String>, String> {
    let jobs = state.jobs.clone();
    blocking(move || {
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        jobs.start_jobs(&preset_id, &paths, options.unwrap_or_default())
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
pub async fn list_jobs(
    state: State<'_, AppState>,
    before_seq: Option<i64>,
) -> Result<Vec<JobListItem>, String> {
    let (history, catalog) = (state.history.clone(), state.catalog.clone());
    blocking(move || {
        let records = lock(&history)
            .list(before_seq, 50)
            .map_err(|e| e.to_string())?;
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
pub async fn engine_install(state: State<'_, AppState>, id: String) -> Result<(), String> {
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
pub async fn set_settings(app: AppHandle, patch: serde_json::Value) -> Result<Settings, String> {
    let handle = app.clone();
    let (before, after) =
        blocking(move || handle.state::<AppState>().update_settings(patch)).await?;
    crate::integration::settings_changed(&app, &before, &after);
    Ok(after)
}

/// The saved output of a job: only a done job whose file still exists qualifies.
pub(crate) fn job_output(history: &History, id: &str) -> Result<PathBuf, String> {
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

/// Preferred licence file in an engine folder: plain text (.txt, .md or no extension)
/// before anything else, then the shallowest path.
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
        .min_by_key(|p| {
            let plain = matches!(
                p.extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .as_deref(),
                None | Some("txt") | Some("md")
            );
            (!plain, p.components().count())
        })
        .map(|p| p.to_string_lossy().into_owned())
}

const LICENCE_TEXT_CAP: u64 = 512 * 1024;

/// Plain-text read, at most 512 KB, invalid UTF-8 replaced; None for unreadable or binary files.
fn read_licence_text(path: &std::path::Path) -> Option<String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(LICENCE_TEXT_CAP)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// File in `licenses/` holding the text of an SPDX-style licence id.
fn bundled_licence_file(licence: &str) -> Option<&'static str> {
    let id = licence
        .trim_end_matches("-or-later")
        .trim_end_matches("-only")
        .trim_end_matches('+');
    match id {
        "GPL-3.0" => Some("GPL-3.0.txt"),
        "GPL-2.0" => Some("GPL-2.0.txt"),
        "AGPL-3.0" => Some("AGPL-3.0.txt"),
        "Apache-2.0" => Some("Apache-2.0.txt"),
        "ImageMagick" => Some("ImageMagick.txt"),
        _ => None,
    }
}

/// The notice's own licence file when it is there, else the text Recast ships.
fn licence_text(n: &Notice, licenses_dir: &std::path::Path) -> Result<String, String> {
    n.licence_file
        .as_deref()
        .and_then(|f| read_licence_text(std::path::Path::new(f)))
        .or_else(|| {
            bundled_licence_file(&n.licence).and_then(|f| read_licence_text(&licenses_dir.join(f)))
        })
        .ok_or_else(|| format!("No licence text is available for {}.", n.name))
}

/// The licence text of the notice at `index` (as listed by `notices`).
#[tauri::command]
pub async fn notice_text(
    state: State<'_, AppState>,
    index: usize,
    name: String,
) -> Result<String, String> {
    let (engines, licence, dir) = (
        state.engines.clone(),
        state.sevenzip_licence.clone(),
        state.licenses_dir.clone(),
    );
    blocking(move || {
        let notices = build_notices(&engines, &licence);
        let n = notice_at(&notices, index, &name)?;
        licence_text(n, &dir)
    })
    .await
}

/// The notice at `index`, provided it is still the one the viewer asked about (the list
/// can change between listing and opening).
fn notice_at<'a>(notices: &'a [Notice], index: usize, name: &str) -> Result<&'a Notice, String> {
    notices
        .get(index)
        .filter(|n| n.name == name)
        .ok_or_else(|| "The list changed; reopen Library.".to_string())
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
        h.insert(id, "image-png", &["C:\\a.jpg".to_string()], 1, None)
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

    fn notice(licence: &str, file: Option<&std::path::Path>) -> Notice {
        Notice {
            name: "Tool".into(),
            licence: licence.into(),
            homepage: String::new(),
            source_url: None,
            licence_file: file.map(|f| f.to_string_lossy().into_owned()),
            bundled: false,
        }
    }

    #[test]
    fn notice_lookup_checks_the_name_as_well_as_the_index() {
        let list = [notice("MIT", None)];
        assert_eq!(
            notice_at(&list, 0, "Tool").ok().map(|n| &*n.name),
            Some("Tool")
        );
        let stale = "The list changed; reopen Library.";
        assert_eq!(notice_at(&list, 0, "Other").err().as_deref(), Some(stale));
        assert_eq!(notice_at(&list, 1, "Tool").err().as_deref(), Some(stale));
    }

    fn bundled_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("licenses")
    }

    #[test]
    fn every_manifest_licence_has_bundled_text() {
        let m =
            crate::engines::manifest::Manifest::parse(crate::engines::manifest::BUNDLED).unwrap();
        for e in &m.engines {
            let t = licence_text(&notice(&e.licence, None), &bundled_dir())
                .unwrap_or_else(|err| panic!("{}: {err}", e.licence));
            assert!(t.len() > 1000, "{} text is too short", e.licence);
        }
    }

    #[test]
    fn an_installed_engines_own_licence_wins_and_is_capped() {
        let d = tempfile::tempdir().unwrap();
        let own = d.path().join("LICENSE.txt");
        std::fs::write(&own, "own terms").unwrap();
        assert_eq!(
            licence_text(&notice("GPL-3.0-or-later", Some(&own)), &bundled_dir()).unwrap(),
            "own terms"
        );
        let big = d.path().join("COPYING");
        std::fs::write(&big, vec![b'a'; 600 * 1024]).unwrap();
        let t = licence_text(&notice("MIT", Some(&big)), &bundled_dir()).unwrap();
        assert_eq!(t.len(), 512 * 1024);
        // Binary files fall back to the bundled text; unknown ids have none.
        let bin = d.path().join("LICENSE.pdf");
        std::fs::write(&bin, b"%PDF\0\0").unwrap();
        assert!(
            licence_text(&notice("Apache-2.0", Some(&bin)), &bundled_dir())
                .unwrap()
                .contains("Apache License")
        );
        assert!(licence_text(&notice("MIT", Some(&bin)), &bundled_dir()).is_err());
        assert!(licence_text(&notice("MIT", None), &bundled_dir()).is_err());
    }

    #[test]
    fn plain_text_licence_files_are_preferred() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("docs")).unwrap();
        std::fs::write(d.path().join("LICENSE.html"), "h").unwrap();
        std::fs::write(d.path().join("docs").join("LICENSE.txt"), "t").unwrap();
        let picked = licence_file(d.path()).unwrap();
        assert!(picked.ends_with("LICENSE.txt"), "{picked}");
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
