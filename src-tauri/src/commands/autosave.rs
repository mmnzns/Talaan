//! "Also save each meeting to a folder" (Settings › Connections). Rust says when a meeting's copy is due
//! (`due`, emitted as `autosave-due`); the main window turns the meeting into the same Markdown as Export and
//! hands it back (`autosave_write`), so the file always matches what Export writes. Rust owns the folder: names,
//! renames, and skipping it when it isn't there (`autosave.rs`).

use super::{AppState, CmdResult};
use crate::autosave::{self, Outcome};
use crate::storage::model::{ExportedCopy, MeetingStatus};
use crate::storage::StorageError;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

pub const AUTOSAVE_DUE: &str = "autosave-due";

/// Meetings skipped because the folder wasn't there, so Check setup can say so and Try again can catch up.
static SKIPPED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

fn skipped() -> std::sync::MutexGuard<'static, BTreeSet<String>> {
    SKIPPED.lock().unwrap_or_else(|e| e.into_inner())
}

fn folder(app: &AppHandle) -> Option<PathBuf> {
    let s = app.state::<AppState>().lock().settings_get().ok().flatten()?;
    s["autoSaveFolder"].as_str().map(str::trim).filter(|f| !f.is_empty()).map(PathBuf::from)
}

/// Tells the main window a meeting's copy should be written: `finished` when its summary (or transcript-only
/// result) was just saved; otherwise it changed (title, summary edits, names) and only an existing copy is updated.
/// Sent whether or not a folder is chosen (the window checks): callers may hold the storage lock.
pub fn due(app: &AppHandle, id: &str, finished: bool) {
    let _ = app.emit(AUTOSAVE_DUE, serde_json::json!({ "id": id, "finished": finished }));
}

/// Writes a meeting's copy. Answers "written", "off" (no folder set), "not_ready", "not_saved" (a change to a
/// meeting that has no copy yet) or "missing" (the folder isn't there: skipped, shown in Check setup).
/// Async: a folder on a disconnected network drive can take a long time to answer.
#[tauri::command]
pub async fn autosave_write(app: AppHandle, id: String, contents: String, finished: bool) -> CmdResult<&'static str> {
    let Some(dir) = folder(&app) else { return Ok("off") };
    tauri::async_runtime::spawn_blocking(move || write_one(&app, &dir, &id, &contents, finished))
        .await
        .map_err(|e| StorageError::Refused(e.to_string()))?
}

fn write_one(app: &AppHandle, dir: &Path, id: &str, contents: &str, finished: bool) -> CmdResult<&'static str> {
    let state = app.state::<AppState>();
    let file = state.lock().read_meeting_file(id)?;
    if file.status != MeetingStatus::Ready {
        return Ok("not_ready");
    }
    let was_skipped = skipped().contains(id);
    if !finished && !was_skipped && file.exported_copy.is_none() {
        return Ok("not_saved");
    }
    let name = autosave::file_name(&file.started_at, &file.title);
    let previous = file.exported_copy.as_ref().map(|c| autosave::Copy { path: PathBuf::from(&c.path), sha256: c.sha256.clone(), size: c.size });
    match autosave::write(dir, &name, previous.as_ref(), contents, &crate::platform::trash::to_trash) {
        Ok(Outcome::Written(copy)) => {
            skipped().remove(id);
            let copy = ExportedCopy { path: copy.path.to_string_lossy().into_owned(), sha256: copy.sha256, size: copy.size };
            if file.exported_copy.as_ref() != Some(&copy) {
                state.lock().update_meeting_file(id, |f| f.exported_copy = Some(copy))?;
            }
            Ok("written")
        }
        Ok(Outcome::FolderMissing) => {
            skipped().insert(id.to_string());
            Ok("missing")
        }
        Err(e) => {
            eprintln!("talaan: couldn't save {id} to the folder: {e}");
            Err(StorageError::Refused(crate::i18n::tr_err("error.save_file", e)))
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoSaveStatus {
    /// Whether the chosen folder is there right now.
    reachable: bool,
    /// Meetings not saved because it wasn't (written next time, or with Try again).
    skipped: Vec<String>,
}

/// For Check setup. `None` when no folder is chosen.
#[tauri::command]
pub async fn autosave_status(app: AppHandle) -> Option<AutoSaveStatus> {
    let dir = folder(&app)?;
    let reachable = tauri::async_runtime::spawn_blocking(move || dir.is_dir()).await.unwrap_or(false);
    Some(AutoSaveStatus { reachable, skipped: skipped().iter().cloned().collect() })
}

/// Shows the folder in Finder or File Explorer.
#[tauri::command]
pub fn autosave_reveal(app: AppHandle) -> CmdResult<()> {
    let dir = folder(&app).ok_or_else(|| StorageError::NotFound("autosave folder".into()))?;
    tauri_plugin_opener::reveal_item_in_dir(&dir).map_err(|e| StorageError::Refused(crate::i18n::tr_err("error.open_folder", e)))
}
