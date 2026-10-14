//! Developer-only commands. In release builds they refuse to run.

use super::{notify, AppState, CmdResult, MEETINGS_CHANGED};
use crate::storage::model::Meeting;
use crate::storage::StorageError;
use serde::Deserialize;
use tauri::{AppHandle, State};

#[derive(Deserialize)]
pub struct FolderIn {
    id: String,
    name: String,
}

fn debug_only() -> CmdResult<()> {
    if cfg!(debug_assertions) {
        Ok(())
    } else {
        Err(StorageError::NotYetAvailable("Developer tools are only in debug builds".into()))
    }
}

#[tauri::command]
pub fn dev_seed(app: AppHandle, state: State<AppState>, meetings: Vec<Meeting>, folders: Vec<FolderIn>) -> CmdResult<()> {
    debug_only()?;
    let storage = state.lock();
    storage.dev_set_folders(&folders.into_iter().map(|f| (f.id, f.name)).collect::<Vec<_>>())?;
    for m in &meetings {
        storage.dev_seed_meeting(m)?;
    }
    drop(storage);
    notify(&app, MEETINGS_CHANGED);
    Ok(())
}

#[tauri::command]
pub fn dev_wipe(app: AppHandle, state: State<AppState>) -> CmdResult<()> {
    debug_only()?;
    state.lock().dev_wipe_library()?;
    notify(&app, MEETINGS_CHANGED);
    Ok(())
}
