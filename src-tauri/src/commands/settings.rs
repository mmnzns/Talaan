use super::{notify, AppState, CmdResult, MEETINGS_CHANGED, SETTINGS_CHANGED};
use crate::models::Models;
use serde_json::Value;
use std::sync::Arc;
use tauri::{AppHandle, State};

#[tauri::command(async)]
pub fn settings_get(state: State<AppState>) -> CmdResult<Option<Value>> {
    state.lock().settings_get()
}

#[tauri::command]
pub fn settings_set(app: AppHandle, state: State<AppState>, models: State<Arc<Models>>, mut value: Value) -> CmdResult<()> {
    let mut storage = state.lock();
    // The card default, once: a first launch has no settings file for the start-up default, so the first save
    // carries it (the card listing runs once per process).
    let stored = storage.settings_get().ok().flatten();
    crate::gpu::apply_default_on_save(&mut value, stored.as_ref(), cfg!(target_os = "macos"), crate::gpu::discrete_card().as_deref());
    let claim = value.clone();
    let name_before = storage.your_name();
    storage.settings_set(value)?;
    // Your name shows in every meeting.
    let renamed = storage.your_name() != name_before;
    crate::logging::set_your_name(storage.your_name());
    // A changed retention setting takes effect right away.
    let report = storage.apply_retention(chrono::Utc::now())?;
    drop(storage);
    // A model the new settings point to that another app installed: Talaan uses it from now on.
    models.claim(Some(&claim));
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
pub fn data_delete_all(app: AppHandle, state: State<AppState>, models: State<Arc<Models>>, t: State<super::transcriber::TranscriberState>) -> CmdResult<()> {
    super::recorder::discard_for_wipe(&app);
    // Let go of every model first: an engine or helper that has a model open keeps the engine from erasing it
    // (and on Windows from deleting the file).
    t.stop_for_wipe();
    crate::summarizer::local::shutdown_all();
    // The models are the shared folder's: Talaan stops using them, and the engine erases each one no other app
    // (Ensayo, Parlato) still uses. A failure here must not leave the rest of the data behind: it is logged (the
    // kind only, the text can hold a path) and the wipe goes on.
    if let Err(e) = models.store().release_all() {
        let kind = e.kind();
        log::warn!("couldn't release every model while deleting all data ({kind})");
    }
    state.lock().delete_all_data()?;
    // Talaan's old model folders (the links the move left for 1.1.1).
    models.remove_legacy_dirs();
    crate::keys::clear_all();
    super::updates::remove_login_item(&app);
    crate::logging::delete_files();
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

/// What Talaan's models take on this computer: the installed ones Talaan uses (a model only another app uses is
/// that app's).
pub fn models_bytes(models: &Models) -> u64 {
    voice_engine_models::catalog::all()
        .iter()
        .filter(|m| matches!(models.status(m.id), voice_engine_models::Status::Installed { used_by_me: true, .. }))
        .map(|m| m.size())
        .sum()
}

/// Where the data lives and how big it is, for the Privacy tab.
#[tauri::command]
pub fn data_info(state: State<AppState>, models: State<Arc<Models>>) -> CmdResult<DataInfo> {
    let storage = state.lock();
    // The data folder, plus the models Talaan uses (they live in the shared folder, outside it).
    let bytes = crate::storage::io::dir_size(storage.root()) + models_bytes(&models);
    Ok(DataInfo { path: storage.root().to_string_lossy().to_string(), bytes })
}
