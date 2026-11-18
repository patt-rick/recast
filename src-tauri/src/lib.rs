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
pub mod pad_model;
pub mod paths;
pub mod platform;
pub mod presets;
pub mod proc;
pub mod settings;
pub mod tray;

#[cfg(test)]
mod testutil;

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
            tray::build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the companion hides it; Recast stays in the tray (spec §4.5).
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "companion"
            {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            ipc::inspect,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Recast");
}
