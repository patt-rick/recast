//! Applies settings to the world outside Recast's window: the Explorer verb and the
//! Run value (spec §6.10, §4.5), and (Task 7) the trigger windows and the shake hook.
use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use crate::app_state::AppState;
use crate::lockutil::lock;
use crate::platform;
use crate::settings::Settings;
use crate::shell;

/// Serialises registry writes so two quick settings changes land in order.
static REG: Mutex<()> = Mutex::new(());

/// Release builds always; a dev build only with RECAST_DEV_INTEGRATION=1, so `cargo run`
/// never points Explorer or the Run key at target\debug (deviation 6).
pub fn registry_allowed() -> bool {
    !cfg!(debug_assertions) || std::env::var("RECAST_DEV_INTEGRATION").as_deref() == Ok("1")
}

/// Bring the Explorer verb and the Run value in line with the current settings. Re-run
/// at every start so the paths follow the exe after an update or a move. It reads the
/// settings itself, under a lock, so concurrent calls apply in order (B11).
pub fn sync_registry(app: &AppHandle) {
    if !registry_allowed() {
        return;
    }
    let _guard = lock(&REG);
    let s = app.state::<AppState>().settings_now();
    let exe = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(e) => {
            log::warn!("shell: exe path unknown: {e}");
            return;
        }
    };
    if s.shell.explorer_entry {
        for e in shell::verb_entries(&exe) {
            if let Err(err) = platform::hkcu_set_string(&e.key, e.name, &e.data) {
                log::warn!("shell: writing {}: {err}", e.key);
            }
        }
    } else if let Err(err) = platform::hkcu_delete_tree(shell::VERB_KEY) {
        log::warn!("shell: removing the Explorer entry: {err}");
    }
    let run = if s.app.start_with_windows {
        platform::hkcu_set_string(
            shell::RUN_KEY,
            Some(shell::RUN_VALUE),
            &shell::run_command(&exe),
        )
    } else {
        platform::hkcu_delete_value(shell::RUN_KEY, shell::RUN_VALUE)
    };
    if let Err(err) = run {
        log::warn!("shell: start with Windows: {err}");
    }
}

/// Called after every settings change from the UI.
pub fn settings_changed(app: &AppHandle, before: &Settings, after: &Settings) {
    if before.shell != after.shell || before.app.start_with_windows != after.app.start_with_windows
    {
        sync_registry(app);
    }
    if before.triggers != after.triggers {
        crate::triggers::sync(app);
    }
}
