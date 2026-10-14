mod commands;
mod keys;
mod platform;
mod recorder;
mod storage;
mod summarizer;
mod transcriber;

use commands::recorder as rec_cmd;
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
            let mut store = storage::Storage::new(root.clone())?;
            store.load_policy()?;
            // Anything still marked "recording" was interrupted; expired audio goes now.
            store.scan_for_interrupted()?;
            store.apply_retention(chrono::Utc::now())?;
            app.manage(AppState::new(store));
            rec_cmd::setup(app.handle());
            commands::summarizer::setup(app.handle(), root.clone());
            #[cfg(target_os = "macos")]
            {
                let handle = app.handle().clone();
                platform::macos::notify::setup(move |id| commands::summarizer::open_meeting(&handle, &id));
            }
            commands::transcriber::setup(app.handle(), root);

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
            rec_cmd::meetings_recover,
            rec_cmd::recorder_start,
            rec_cmd::recorder_pause,
            rec_cmd::recorder_resume,
            rec_cmd::recorder_mark,
            rec_cmd::recorder_stop,
            rec_cmd::recorder_discard,
            rec_cmd::recorder_status,
            rec_cmd::permissions_status,
            rec_cmd::permissions_request_mic,
            rec_cmd::permissions_open_settings,
            commands::transcriber::meetings_retry,
            commands::transcriber::models_list,
            commands::transcriber::models_download,
            commands::transcriber::models_cancel,
            commands::transcriber::models_delete,
            commands::summarizer::meetings_generate_summary,
            commands::summarizer::llm_models_list,
            commands::summarizer::keys_save,
            commands::summarizer::keys_clear,
            commands::summarizer::local_server_models,
            settings::settings_get,
            settings::settings_set,
            settings::data_delete_all,
            settings::data_info,
            dev::dev_seed,
            dev::dev_wipe,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Talaan")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                rec_cmd::stop_on_exit(app);
            }
        });
}
