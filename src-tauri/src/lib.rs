mod commands;
mod platform;
mod storage;

use commands::{dev, meetings, settings, AppState, MEETINGS_CHANGED};
use std::time::Duration;
use tauri::Manager;

#[tauri::command]
fn platform_name() -> &'static str {
    platform::NAME
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let root = commands::resolve_data_dir(std::env::var("TALAAN_DATA_DIR").ok(), dirs::data_dir());
            let mut store = storage::Storage::new(root)?;
            store.load_policy()?;
            // Anything still marked "recording" was interrupted; expired audio goes now.
            store.scan_for_interrupted()?;
            store.apply_retention(chrono::Utc::now())?;
            app.manage(AppState::new(store));

            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(3600));
                let state = handle.state::<AppState>();
                let report = state.lock().apply_retention(chrono::Utc::now());
                if report.is_ok_and(|r| !r.audio_deleted.is_empty() || !r.meetings_deleted.is_empty()) {
                    commands::notify(&handle, MEETINGS_CHANGED);
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            platform_name,
            meetings::meetings_list,
            meetings::meetings_get,
            meetings::meetings_rename,
            meetings::meetings_set_pinned,
            meetings::meetings_delete,
            meetings::meetings_update_transcript_line,
            meetings::meetings_update_summary,
            meetings::meetings_revert_edits,
            meetings::meetings_rename_speaker,
            meetings::meetings_set_tags,
            meetings::meetings_move_to_folder,
            meetings::folders_list,
            meetings::folders_create,
            meetings::tags_list,
            meetings::meetings_recover,
            meetings::meetings_retry,
            meetings::meetings_generate_summary,
            settings::settings_get,
            settings::settings_set,
            settings::data_delete_all,
            settings::data_info,
            dev::dev_seed,
            dev::dev_wipe,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Talaan");
}
