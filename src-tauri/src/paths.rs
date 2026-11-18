use std::path::{Path, PathBuf};

/// Recast's fixed data folders (spec §6.12): not Tauri's identifier-named ones.
#[derive(Clone, Debug)]
pub struct Paths {
    /// %LOCALAPPDATA%\Recast — engines, history, logs.
    pub local: PathBuf,
    /// %APPDATA%\Recast — settings.
    pub roaming: PathBuf,
}

impl Paths {
    pub fn from_env() -> Self {
        Self {
            local: dirs::data_local_dir().expect("LOCALAPPDATA is not set").join("Recast"),
            roaming: dirs::config_dir().expect("APPDATA is not set").join("Recast"),
        }
    }

    pub fn under(root: &Path) -> Self {
        Self { local: root.join("local"), roaming: root.join("roaming") }
    }

    pub fn engines(&self) -> PathBuf {
        self.local.join("engines")
    }
    pub fn db(&self) -> PathBuf {
        self.local.join("recast.db")
    }
    pub fn logs(&self) -> PathBuf {
        self.local.join("logs")
    }
    pub fn settings(&self) -> PathBuf {
        self.roaming.join("settings.json")
    }
}
