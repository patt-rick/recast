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
    registry_allowed_with(
        cfg!(debug_assertions),
        std::env::var("RECAST_DEV_INTEGRATION").ok().as_deref(),
    )
}

/// `registry_allowed` with its inputs passed in: whether this is a debug build, and the
/// value of RECAST_DEV_INTEGRATION.
pub fn registry_allowed_with(debug: bool, var: Option<&str>) -> bool {
    !debug || var == Some("1")
}

/// Bring the Explorer verb and the Run value in line with the current settings. Re-run
/// at every start so the paths follow the exe after an update or a move. It reads the
/// settings itself, under a lock, so concurrent calls apply in order (B11). Returns what
/// failed (also logged), empty when everything was written.
pub fn sync_registry(app: &AppHandle) -> Vec<String> {
    let mut errors = Vec::new();
    if !registry_allowed() {
        return errors;
    }
    let _guard = lock(&REG);
    let s = app.state::<AppState>().settings_now();
    let exe = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(e) => {
            log::warn!("shell: exe path unknown: {e}");
            errors.push(format!("Recast's own location is unknown ({e})"));
            return errors;
        }
    };
    if s.shell.explorer_entry {
        for e in shell::verb_entries(&exe) {
            if let Err(err) = platform::hkcu_set_string(&e.key, e.name, &e.data) {
                log::warn!("shell: writing {}: {err}", e.key);
                errors.push(format!("the Explorer entry ({err})"));
                break;
            }
        }
    } else if let Err(err) = platform::hkcu_delete_tree(shell::VERB_KEY) {
        log::warn!("shell: removing the Explorer entry: {err}");
        errors.push(format!("removing the Explorer entry ({err})"));
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
        errors.push(format!("start with Windows ({err})"));
    }
    errors
}

/// What a settings change needs doing, decided from the settings alone.
#[derive(Debug, PartialEq, Eq)]
pub struct Dispatch {
    pub registry: bool,
    pub triggers: bool,
}

pub fn dispatch(before: &Settings, after: &Settings) -> Dispatch {
    Dispatch {
        registry: before.shell != after.shell
            || before.app.start_with_windows != after.app.start_with_windows,
        triggers: before.triggers != after.triggers,
    }
}

/// Called after every settings change from the UI. The settings are already saved; an
/// Err says which Windows write failed.
pub fn settings_changed(
    app: &AppHandle,
    before: &Settings,
    after: &Settings,
) -> Result<(), String> {
    let d = dispatch(before, after);
    let errors = if d.registry {
        sync_registry(app)
    } else {
        Vec::new()
    };
    if d.triggers {
        crate::triggers::sync(app);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Couldn't update Windows: {}. Your settings were saved.",
            errors.join("; ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_writes_need_a_release_build_or_the_dev_opt_in() {
        assert!(registry_allowed_with(false, None));
        assert!(registry_allowed_with(false, Some("0")));
        assert!(!registry_allowed_with(true, None));
        assert!(!registry_allowed_with(true, Some("0")));
        assert!(!registry_allowed_with(true, Some("true")));
        assert!(registry_allowed_with(true, Some("1")));
    }

    #[test]
    fn a_settings_change_dispatches_only_what_it_touched() {
        let base = Settings::default();
        assert_eq!(
            dispatch(&base, &base),
            Dispatch {
                registry: false,
                triggers: false
            }
        );
        let mut a = base.clone();
        a.app.start_with_windows = !a.app.start_with_windows;
        assert_eq!(
            dispatch(&base, &a),
            Dispatch {
                registry: true,
                triggers: false
            }
        );
        let mut b = base.clone();
        b.shell.explorer_entry = !b.shell.explorer_entry;
        assert!(dispatch(&base, &b).registry);
        let mut c = base.clone();
        c.triggers.edge_handle = !c.triggers.edge_handle;
        assert_eq!(
            dispatch(&base, &c),
            Dispatch {
                registry: false,
                triggers: true
            }
        );
    }
}
