//! "Also save each meeting to a folder" (Settings › Connections). Rust says when a meeting's copy is due
//! (`due`, emitted as `autosave-due`); the main window turns the meeting into the same Markdown as Export and
//! hands it back (`autosave_write`), so the file always matches what Export writes. Rust owns the folder: names,
//! renames, edited copies, and keeping a copy pending (in meeting.json) until it's written (`autosave.rs`).

use super::{AppState, CmdResult};
use crate::autosave;
use crate::storage::StorageError;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

pub const AUTOSAVE_DUE: &str = "autosave-due";

fn folder(app: &AppHandle) -> Option<PathBuf> {
    let s = app.state::<AppState>().lock().settings_get().ok().flatten()?;
    s["autoSaveFolder"].as_str().map(str::trim).filter(|f| !f.is_empty()).map(PathBuf::from)
}

/// Onboarding's test recording (while its keep-or-delete offer is open) never gets a copy in the folder.
fn is_test(app: &AppHandle, id: &str) -> bool {
    let s = app.state::<AppState>().lock().settings_get().ok().flatten();
    super::ask::pending_test(s.as_ref()) == Some(id)
}

/// A meeting's copy should be written: `finished` when its summary (or transcript-only result) was just saved;
/// otherwise it changed (title, summary edits, ticks, names) and only an existing or pending copy is updated. It is
/// marked pending first (so a quit before it's written doesn't lose it), then the main window is told. On its own
/// thread: callers may hold the storage lock.
pub fn due(app: &AppHandle, id: &str, finished: bool) {
    let (app, id) = (app.clone(), id.to_string());
    std::thread::spawn(move || {
        if folder(&app).is_none() || is_test(&app, &id) {
            return;
        }
        let due = autosave::mark_due(&app.state::<AppState>().lock(), &id, finished).unwrap_or(false);
        if due {
            let _ = app.emit(AUTOSAVE_DUE, serde_json::json!({ "id": id, "finished": finished }));
        }
    });
}

/// Writes a meeting's copy. Answers "written", "off" (no folder set), "not_ready" (still processing, or a change to a
/// meeting that has no copy), "missing" (the folder isn't there) or "failed"; the last two stay pending and show in
/// Check setup. Async: a folder on a disconnected network drive can take a long time to answer.
#[tauri::command]
pub async fn autosave_write(app: AppHandle, id: String, contents: String, finished: bool) -> CmdResult<&'static str> {
    let Some(dir) = folder(&app) else { return Ok("off") };
    tauri::async_runtime::spawn_blocking(move || write_one(&app, &dir, &id, &contents, finished))
        .await
        .map_err(|e| StorageError::Refused(e.to_string()))?
}

/// One write at a time per meeting, so the main window's queue and Check setup's Try again can't both pick a name
/// and leave a "(2)" copy behind.
fn meeting_lock(id: &str) -> Arc<Mutex<()>> {
    static LOCKS: Mutex<BTreeMap<String, Arc<Mutex<()>>>> = Mutex::new(BTreeMap::new());
    let mut locks = LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    locks.retain(|_, l| Arc::strong_count(l) > 1); // forget meetings nobody is writing
    locks.entry(id.to_string()).or_default().clone()
}

fn write_one(app: &AppHandle, dir: &std::path::Path, id: &str, contents: &str, finished: bool) -> CmdResult<&'static str> {
    let lock = meeting_lock(id);
    let _one_at_a_time = lock.lock().unwrap_or_else(|e| e.into_inner());
    if is_test(app, id) {
        return Ok("not_ready");
    }
    let state = app.state::<AppState>();
    let Some(plan) = autosave::plan(&state.lock(), id, finished)? else { return Ok("not_ready") };
    // The storage lock is not held while the folder is written: it may be a slow or sleeping drive.
    let result = autosave::write(dir, &plan.name, plan.previous.as_ref(), contents, &crate::platform::trash::to_trash);
    if let Err(e) = &result {
        eprintln!("talaan: couldn't save {id} to the folder: {e}");
    }
    Ok(autosave::record(&state.lock(), id, &result)?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    id: String,
    /// "waiting", "missing" or "failed".
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoSaveStatus {
    /// Whether the chosen folder is there right now.
    reachable: bool,
    /// Copies still to be written (retried at launch, after the next copy that works, and with Try again).
    pending: Vec<Pending>,
}

/// For Check setup and the retries. `None` when no folder is chosen.
#[tauri::command]
pub async fn autosave_status(app: AppHandle) -> Option<AutoSaveStatus> {
    let dir = folder(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let reachable = dir.is_dir();
        let pending = autosave::pending(&app.state::<AppState>().lock()).unwrap_or_default();
        AutoSaveStatus { reachable, pending: pending.into_iter().map(|(id, p)| Pending { id, kind: p.kind, error: p.error }).collect() }
    })
    .await
    .ok()
}

/// Shows the folder in Finder or File Explorer.
#[tauri::command]
pub fn autosave_reveal(app: AppHandle) -> CmdResult<()> {
    let dir = folder(&app).ok_or_else(|| StorageError::NotFound("autosave folder".into()))?;
    tauri_plugin_opener::reveal_item_in_dir(&dir).map_err(|e| StorageError::Refused(crate::i18n::tr_err("error.open_folder", e)))
}
