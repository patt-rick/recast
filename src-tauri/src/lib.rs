pub mod app_state;
pub mod backdrop;
pub mod detect;
pub mod drops;
pub mod engines;
pub mod fsutil;
pub mod history;
pub mod integration;
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
pub mod shell;
pub mod thumbs;
pub mod toast;
pub mod tray;
pub mod triggers;

#[cfg(test)]
mod testutil;

use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Managed before the plugin so its callback always finds it (B6).
        .manage(shell::Forwarder::default())
        // Every state an IPC command or window event can reach exists before any window
        // does; AppState follows in setup, and the windows only after it.
        .manage(pad::PadState::default())
        .manage(triggers::TriggerState::default())
        .manage(toast::Toasts::default())
        // Must be the first plugin. A second copy would mark this copy's running jobs
        // INTERRUPTED and delete their temp dirs; instead it forwards its arguments here.
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            let launch = shell::parse_args(&argv, std::path::Path::new(&cwd));
            if launch.autostart {
                // A second --autostart (e.g. a repeated sign-in) must not pop the window (L4).
                return;
            }
            if launch.pad.is_empty() {
                tray::show_companion(app)
            } else {
                shell::forward(app, launch.pad)
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .on_page_load(|wv, payload| {
            // A reload drops the pad page's listeners; Rust must stop treating it as ready.
            if wv.label() == pad::LABEL && payload.event() == tauri::webview::PageLoadEvent::Started
            {
                pad::page_loading(wv.app_handle());
            }
        })
        .setup(|app| {
            app_state::setup(app)?;
            toast::init(app.handle());
            drops::init(app.handle());
            // The windows are `create: false` in tauri.conf.json: Tauri would build them
            // before this closure runs, and a page that loads at once (a release build)
            // would then call commands whose state doesn't exist yet. (A second launch
            // forwarded while the companion itself is being built finds no window to
            // show; `--pad` launches are deferred and unaffected.)
            for cfg in app.config().app.windows.clone() {
                tauri::WebviewWindowBuilder::from_config(app.handle(), &cfg)?.build()?;
            }
            pad::init(app.handle());
            triggers::init(app.handle());
            drops::cover_soon(app.handle());
            tray::build(app)?;
            // Failures are logged inside; startup has nobody to tell.
            let _ = integration::sync_registry(app.handle());
            let argv: Vec<String> = std::env::args_os()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            let cwd = std::env::current_dir().unwrap_or_default();
            let launch = shell::parse_args(&argv, &cwd);
            if !launch.pad.is_empty() {
                shell::forward(app.handle(), launch.pad);
            } else if !launch.autostart {
                tray::show_companion(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                // Drags wry delivers; Recast's own drop targets end up in `dispatch` too.
                tauri::WindowEvent::DragDrop(ev) => {
                    drops::dispatch(window, ev);
                    return;
                }
                tauri::WindowEvent::Moved(pos) if window.label() == triggers::DESK => {
                    triggers::desk_moved(window.app_handle(), *pos);
                    return;
                }
                _ => {}
            }
            let tauri::WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            match window.label() {
                // Closing the companion hides it; Recast stays in the tray (spec §4.5).
                "companion" => {
                    // The first-run highlight is only for a visible companion.
                    triggers::highlight(window.app_handle(), false);
                    let _ = window.hide();
                    api.prevent_close();
                }
                // The pad window is created once and reused: Alt+F4 only dismisses it.
                pad::LABEL => {
                    api.prevent_close();
                    let _ = window.app_handle().emit_to(pad::LABEL, "pad://dismiss", ());
                    pad::close(window.app_handle());
                }
                // The widgets are reused for the whole run, like the pad.
                triggers::HANDLE | triggers::DESK => api.prevent_close(),
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
            ipc::pad_show,
            ipc::pad_close,
            ipc::pad_track_cursor,
            ipc::capture_backdrop,
            ipc::highlight_handle,
            ipc::settings_read_only,
            ipc::open_in_recast,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Recast");
}
