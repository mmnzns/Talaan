//! Tauri commands. Thin wrappers: lock the storage, call it, tell every window what changed.

pub mod meetings;
pub mod settings;

use crate::storage::{Storage, StorageError};
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter};

pub struct AppState {
    storage: Mutex<Storage>,
}

impl AppState {
    pub fn new(storage: Storage) -> Self {
        AppState { storage: Mutex::new(storage) }
    }

    /// A panic while holding the lock must not brick the app, so a poisoned lock is recovered.
    pub fn lock(&self) -> MutexGuard<'_, Storage> {
        self.storage.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub type CmdResult<T> = Result<T, StorageError>;

pub const MEETINGS_CHANGED: &str = "meetings-changed";
pub const SETTINGS_CHANGED: &str = "settings-changed";

pub fn notify(app: &AppHandle, event: &str) {
    let _ = app.emit(event, ());
}

/// Where Talaan keeps its data: `TALAAN_DATA_DIR` if set, otherwise the OS data folder.
pub fn resolve_data_dir(env_override: Option<String>, os_data_dir: Option<std::path::PathBuf>) -> std::path::PathBuf {
    match env_override.filter(|s| !s.trim().is_empty()) {
        Some(p) => p.into(),
        None => os_data_dir.unwrap_or_else(std::env::temp_dir).join("Talaan"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn env_override_wins() {
        assert_eq!(resolve_data_dir(Some("/tmp/x".into()), Some("/Users/a/Library".into())), PathBuf::from("/tmp/x"));
    }

    #[test]
    fn default_is_the_os_data_dir_plus_talaan() {
        assert_eq!(resolve_data_dir(None, Some("/Users/a/Library/Application Support".into())), PathBuf::from("/Users/a/Library/Application Support/Talaan"));
        assert_eq!(resolve_data_dir(Some("  ".into()), Some("/d".into())), PathBuf::from("/d/Talaan"));
    }
}
