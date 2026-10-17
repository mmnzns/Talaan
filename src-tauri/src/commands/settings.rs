use super::{notify, AppState, CmdResult, MEETINGS_CHANGED, SETTINGS_CHANGED};
use serde_json::Value;
use tauri::{AppHandle, State};

#[tauri::command(async)]
pub fn settings_get(state: State<AppState>) -> CmdResult<Option<Value>> {
    state.lock().settings_get()
}

#[tauri::command]
pub fn settings_set(app: AppHandle, state: State<AppState>, value: Value) -> CmdResult<()> {
    let mut storage = state.lock();
    let name_before = storage.your_name();
    storage.settings_set(value)?;
    // Your name shows in every meeting.
    let renamed = storage.your_name() != name_before;
    // A changed retention setting takes effect right away.
    let report = storage.apply_retention(chrono::Utc::now())?;
    drop(storage);
    super::updates::apply_login_item(&app);
    #[cfg(target_os = "windows")]
    super::calendar::sync_source(&app);
    notify(&app, SETTINGS_CHANGED);
    if renamed || !report.audio_deleted.is_empty() || !report.meetings_deleted.is_empty() {
        notify(&app, MEETINGS_CHANGED);
    }
    Ok(())
}

/// Deletes everything, then quits (as the Privacy tab says). The next launch starts at onboarding.
#[tauri::command]
pub fn data_delete_all(app: AppHandle, state: State<AppState>) -> CmdResult<()> {
    super::recorder::discard_for_wipe(&app);
    // A models copy still running (Windows) lets go of its files first, and makes nothing afterwards.
    crate::transcriber::relocate::stop(std::time::Duration::from_secs(10));
    let models = {
        let mut storage = state.lock();
        storage.delete_all_data()?;
        crate::transcriber::models::models_root(storage.root())
    };
    // Windows keeps models outside the data folder (%LOCALAPPDATA%); the data folder's own went above.
    if models.exists() {
        std::fs::remove_dir_all(&models).map_err(|e| crate::storage::StorageError::Refused(crate::i18n::tr_err("error.delete_model", e)))?;
    }
    crate::keys::clear_all();
    super::updates::remove_login_item(&app);
    app.exit(0);
    Ok(())
}

#[derive(serde::Serialize)]
pub struct DataInfo {
    path: String,
    bytes: u64,
}

/// Shows the data folder (meetings, models, settings) in the file manager.
#[tauri::command]
pub fn data_reveal(state: State<AppState>) -> CmdResult<()> {
    let meetings = crate::storage::paths::meetings_dir(state.lock().root());
    tauri_plugin_opener::reveal_item_in_dir(meetings).map_err(|e| crate::storage::StorageError::Refused(crate::i18n::tr_err("error.open_folder", e)))
}

/// Where the data lives and how big it is, for the Privacy tab.
#[tauri::command]
pub fn data_info(state: State<AppState>) -> CmdResult<DataInfo> {
    let storage = state.lock();
    let mut bytes = crate::storage::io::dir_size(storage.root());
    // Windows: models live outside the data folder (%LOCALAPPDATA%); count them too.
    let models = crate::transcriber::models::models_root(storage.root());
    if !models.starts_with(storage.root()) {
        bytes += crate::storage::io::dir_size(&models);
    }
    Ok(DataInfo { path: storage.root().to_string_lossy().to_string(), bytes })
}
