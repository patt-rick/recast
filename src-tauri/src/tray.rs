//! Tray icon: left-click opens the companion; menu has Open, Convert files… and Quit (spec §4.5).
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub fn show_companion(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("companion") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn build(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Recast", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let convert = MenuItem::with_id(app, "convert", "Convert files…", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &convert, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("bundle icon"))
        .tooltip("Recast")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_companion(app),
            "convert" => pick_and_open_pad(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                show_companion(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Plan 2's way into the pad window: choose files, then the pad opens at the cursor.
fn pick_and_open_pad(app: &AppHandle) {
    use tauri_plugin_dialog::DialogExt;
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(files) = app
            .dialog()
            .file()
            .set_title("Choose files to convert")
            .blocking_pick_files()
        else {
            return;
        };
        let paths: Vec<String> = files
            .into_iter()
            .filter_map(|f| f.into_path().ok())
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        if !paths.is_empty() {
            crate::pad::open(&app, paths, crate::pad::PadMode::Click);
        }
    });
}
