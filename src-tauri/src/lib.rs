pub mod detect;
pub mod engines;
pub mod fsutil;
pub mod paths;
pub mod platform;
pub mod presets;
pub mod settings;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Recast");
}
