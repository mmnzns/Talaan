mod ask;
mod autosave;
mod calendar;
mod calls;
mod commands;
mod eta;
mod gpu;
mod helper;
mod i18n;
mod importer;
mod keys;
mod notion;
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

/// How long Quit waits for a clean exit before ending the process anyway.
const EXIT_GRACE: Duration = Duration::from_secs(5);

/// Quit always quits. A clean exit has to get back out of the event loop, and if the main thread is stuck (in
/// v1.0.2 on Windows, a webview built inside a synchronous command waited forever) Quit did nothing. The
/// recording is saved by `stop_on_exit` well within the grace period, and audio is flushed every 2 s anyway.
fn exit_watchdog() {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new().name("talaan-exit-watchdog".into()).spawn(|| {
        std::thread::sleep(EXIT_GRACE);
        std::process::exit(0);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    // Windows only, and first, so a second launch exits before anything else starts and focuses the running
    // Talaan instead. (On the Mac the OS already does this, and the plugin's socket would make `tauri dev` exit
    // while the installed app runs.)
    #[cfg(target_os = "windows")]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| commands::tray::show_main(app)));
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .setup(|app| {
            let root = commands::resolve_data_dir(std::env::var("TALAAN_DATA_DIR").ok(), dirs::data_dir());
            let mut store = storage::Storage::new(root.clone())?;
            store.load_policy()?;
            commands::calendar::remove_legacy_address(&mut store);
            // Anything still marked "recording" was interrupted; expired audio goes now.
            store.scan_for_interrupted()?;
            store.apply_retention(chrono::Utc::now())?;
            app.manage(AppState::new(store));
            rec_cmd::setup(app.handle());
            commands::summarizer::setup(app.handle(), root.clone());
            commands::ask::setup(app.handle(), root.clone());
            let handle = app.handle().clone();
            platform::notify::setup(move |id| commands::summarizer::open_meeting(&handle, &id));
            commands::transcriber::setup(app.handle(), root);
            commands::calls::setup(app.handle());
            #[cfg(target_os = "windows")]
            commands::calendar::sync_source(app.handle());
            commands::updates::apply_login_item(app.handle());
            commands::updates::check_at_launch(app.handle());
            commands::tray::setup(app.handle())?;

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
        .on_window_event(commands::tray::on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::tray::i18n_set_language,
            commands::tray::tray_toggle,
            commands::tray::window_fit,
            commands::tray::tray_room,
            commands::summarizer::notifications_allowed,
            commands::tray::tray_hide,
            commands::tray::app_show_main,
            commands::tray::app_quit,
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
            meetings::meetings_set_transcript_only,
            meetings::meetings_move_to_folder,
            meetings::folders_list,
            meetings::folders_rename,
            meetings::folders_delete,
            meetings::folders_create,
            meetings::tags_list,
            meetings::meetings_playback,
            meetings::meetings_delete_audio,
            meetings::meetings_reveal,
            meetings::export_write,
            commands::autosave::autosave_write,
            commands::autosave::autosave_status,
            commands::autosave::autosave_reveal,
            commands::calendar::calendar_next,
            commands::import::meetings_import,
            commands::import::import_extensions,
            rec_cmd::meetings_recover,
            commands::calls::calls_prompt,
            commands::calls::calls_accept,
            commands::calls::calls_dismiss,
            commands::calls::calls_keep,
            commands::calls::calls_stop,
            commands::calls::calls_hide,
            commands::calls::browsers_status,
            commands::updates::updates_check,
            rec_cmd::audio_inputs,
            rec_cmd::permissions_request_system_audio,
            commands::updates::updates_install,
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
            commands::transcriber::meetings_retranscribe,
            commands::transcriber::models_list,
            commands::transcriber::models_download,
            commands::transcriber::models_cancel,
            commands::transcriber::models_delete,
            commands::summarizer::meetings_generate_summary,
            commands::gpu::gpu_status,
            commands::gpu::gpu_retry,
            commands::ask::meetings_ask,
            commands::calendar::calendar_access,
            commands::calendar::calendar_request,
            #[cfg(target_os = "windows")]
            commands::calendar::calendar_connect,
            #[cfg(target_os = "windows")]
            commands::calendar::calendar_disconnect,
            commands::notion::notion_connect,
            commands::notion::notion_disconnect,
            commands::notion::notion_destinations,
            commands::notion::notion_export,
            commands::summarizer::llm_models_list,
            commands::summarizer::keys_save,
            commands::summarizer::keys_clear,
            commands::summarizer::keys_clear_all,
            commands::summarizer::local_server_models,
            settings::settings_get,
            settings::settings_set,
            settings::data_delete_all,
            settings::data_info,
            settings::data_reveal,
            dev::dev_seed,
            dev::dev_wipe,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Talaan")
        .run(|app, event| {
            match event {
                tauri::RunEvent::ExitRequested { code, .. } => {
                    // An update's relaunch exits through here too; leave that to Tauri.
                    if code != Some(tauri::RESTART_EXIT_CODE) {
                        exit_watchdog();
                    }
                    rec_cmd::stop_on_exit(app)
                }
                // Dock icon clicked while the main window is hidden.
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { .. } => commands::tray::show_main(app),
                _ => {}
            }
        });
}
