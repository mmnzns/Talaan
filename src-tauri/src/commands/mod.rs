//! Tauri commands. Thin wrappers: lock the storage, call it, tell every window what changed.

pub mod ask;
pub mod calendar;
pub mod calls;
pub mod dev;
pub mod gpu;
pub mod import;
pub mod meetings;
pub mod notion;
pub mod recorder;
pub mod settings;
pub mod summarizer;
pub mod transcriber;
pub mod tray;
pub mod updates;

use crate::storage::{Storage, StorageError};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter};

pub struct AppState {
    storage: Arc<Mutex<Storage>>,
}

impl AppState {
    pub fn new(storage: Storage) -> Self {
        AppState { storage: Arc::new(Mutex::new(storage)) }
    }

    /// The same storage, for background threads (the transcriber).
    pub fn shared(&self) -> Arc<Mutex<Storage>> {
        self.storage.clone()
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
    if event == MEETINGS_CHANGED {
        ask::reindex_soon(app);
    }
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

    /// Synchronous commands run on the main thread inside WebView2's request handler. Building a webview there
    /// never finishes on Windows and freezes every window (Talaan v1.0.2), so no synchronous command may reach code
    /// that can build one. Checked on the source: each `#[tauri::command]` without `async`, up to its closing brace.
    #[test]
    fn no_synchronous_command_builds_a_window() {
        let sources = [
            ("calls.rs", include_str!("calls.rs")),
            ("tray.rs", include_str!("tray.rs")),
            ("recorder.rs", include_str!("recorder.rs")),
            ("meetings.rs", include_str!("meetings.rs")),
            ("summarizer.rs", include_str!("summarizer.rs")),
            ("transcriber.rs", include_str!("transcriber.rs")),
            ("settings.rs", include_str!("settings.rs")),
            ("import.rs", include_str!("import.rs")),
            ("calendar.rs", include_str!("calendar.rs")),
            ("notion.rs", include_str!("notion.rs")),
            ("updates.rs", include_str!("updates.rs")),
            ("ask.rs", include_str!("ask.rs")),
            ("dev.rs", include_str!("dev.rs")),
        ];
        let builds = ["WebviewWindowBuilder", "show_prompt(", "toggle_popover(", "popover("];
        let mut checked = 0;
        for (file, src) in sources {
            for (i, _) in src.match_indices("#[tauri::command]\n") {
                let rest = &src[i..];
                let head = rest.lines().nth(1).unwrap_or_default();
                if head.contains("async fn") {
                    continue;
                }
                let body = &rest[..rest.find("\n}\n").unwrap_or(rest.len())];
                checked += 1;
                for b in builds {
                    assert!(!body.contains(b), "{file}: synchronous command `{head}` calls {b}; make it #[tauri::command(async)]");
                }
            }
        }
        assert!(checked > 30, "the check found the commands ({checked})");
    }

    #[test]
    fn env_override_wins() {
        assert_eq!(resolve_data_dir(Some("/tmp/x".into()), Some("/Users/a/Library".into())), PathBuf::from("/tmp/x"));
    }

    #[test]
    fn default_is_the_os_data_dir_plus_talaan() {
        assert_eq!(resolve_data_dir(None, Some("/Users/a/Library/Application Support".into())), PathBuf::from("/Users/a/Library/Application Support/Talaan"));
        assert_eq!(resolve_data_dir(Some("  ".into()), Some("/d".into())), PathBuf::from("/d/Talaan"));
    }

    #[cfg(windows)]
    #[test]
    fn default_is_the_roaming_appdata_folder_plus_talaan_on_windows() {
        assert_eq!(
            resolve_data_dir(None, Some(r"C:\Users\a\AppData\Roaming".into())),
            PathBuf::from(r"C:\Users\a\AppData\Roaming\Talaan")
        );
        assert_eq!(resolve_data_dir(Some(r"D:\x".into()), Some(r"C:\Users\a\AppData\Roaming".into())), PathBuf::from(r"D:\x"));
    }
}
