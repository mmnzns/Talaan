use super::{notify, AppState, CmdResult, MEETINGS_CHANGED, SETTINGS_CHANGED};
use serde_json::Value;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn settings_get(state: State<AppState>) -> CmdResult<Option<Value>> {
    state.lock().settings_get()
}

#[tauri::command]
pub fn settings_set(app: AppHandle, state: State<AppState>, value: Value) -> CmdResult<()> {
    let mut storage = state.lock();
    storage.settings_set(value)?;
    // A changed retention setting takes effect right away.
    let report = storage.apply_retention(chrono::Utc::now())?;
    drop(storage);
    super::updates::apply_login_item(&app);
    notify(&app, SETTINGS_CHANGED);
    if !report.audio_deleted.is_empty() || !report.meetings_deleted.is_empty() {
        notify(&app, MEETINGS_CHANGED);
    }
    Ok(())
}

/// Deletes everything, then restarts so the next launch is first-run.
#[tauri::command]
pub fn data_delete_all(app: AppHandle, state: State<AppState>) -> CmdResult<()> {
    state.lock().delete_all_data()?;
    crate::keys::clear_all();
    super::updates::remove_login_item(&app);
    notify(&app, MEETINGS_CHANGED);
    notify(&app, SETTINGS_CHANGED);
    app.restart()
}

#[derive(serde::Serialize)]
pub struct DataInfo {
    path: String,
    bytes: u64,
}

/// Where the data lives and how big it is, for the Privacy tab.
#[tauri::command]
pub fn data_info(state: State<AppState>) -> CmdResult<DataInfo> {
    let storage = state.lock();
    Ok(DataInfo { path: storage.root().to_string_lossy().to_string(), bytes: crate::storage::io::dir_size(storage.root()) })
}
