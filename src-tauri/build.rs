/// Every app command, so each gets an `allow-<command>` permission and the capability files
/// can scope commands per window. Keep in step with `generate_handler!` in `src/lib.rs`.
const COMMANDS: &[&str] = &[
    "inspect",
    "thumbnail",
    "start_jobs",
    "cancel_job",
    "retry_job",
    "list_jobs",
    "remove_job",
    "engines_list",
    "engine_install",
    "engine_remove",
    "engine_verify",
    "get_settings",
    "set_settings",
    "reveal_job",
    "open_job",
    "notice_text",
    "notices",
    "pad_ready",
    "debug_open_pad",
    "pad_show",
    "pad_close",
    "pad_track_cursor",
    "capture_backdrop",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
