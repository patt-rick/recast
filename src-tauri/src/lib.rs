pub mod app_state;
pub mod backdrop;
pub mod detect;
pub mod engines;
pub mod fsutil;
pub mod history;
pub mod ipc;
pub mod jobs;
pub mod lockutil;
pub mod logging;
pub mod pad;
pub mod pad_model;
pub mod paths;
pub mod platform;
pub mod presets;
pub mod proc;
pub mod settings;
pub mod thumbs;
pub mod tray;

#[cfg(test)]
mod testutil;

use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be first. A second copy would mark this copy's running jobs INTERRUPTED
        // and delete their temp dirs; instead it just focuses the running instance.
        // (Plan 3 extends this to forward Explorer selections.)
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            tray::show_companion(app)
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app_state::setup(app)?;
            pad::init(app.handle());
            tray::build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            let tauri::WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            match window.label() {
                // Closing the companion hides it; Recast stays in the tray (spec §4.5).
                "companion" => {
                    let _ = window.hide();
                    api.prevent_close();
                }
                // The pad window is created once and reused: Alt+F4 only dismisses it.
                pad::LABEL => {
                    api.prevent_close();
                    let _ = window.app_handle().emit_to(pad::LABEL, "pad://dismiss", ());
                    pad::close(window.app_handle());
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            ipc::inspect,
            ipc::thumbnail,
            ipc::start_jobs,
            ipc::cancel_job,
            ipc::retry_job,
            ipc::list_jobs,
            ipc::remove_job,
            ipc::engines_list,
            ipc::engine_install,
            ipc::engine_remove,
            ipc::engine_verify,
            ipc::get_settings,
            ipc::set_settings,
            ipc::reveal_job,
            ipc::open_job,
            ipc::notice_text,
            ipc::notices,
            ipc::pad_ready,
            ipc::debug_open_pad,
            ipc::pad_show,
            ipc::pad_close,
            ipc::pad_track_cursor,
            ipc::capture_backdrop,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Recast");
}
