//! Recording commands, the 10 Hz level/state loop, and reactions to device changes and sleep.

use super::{notify, AppState, CmdResult, MEETINGS_CHANGED};
use crate::platform;
use crate::recorder::engine::{Engine, Status};
use crate::recorder::mic::MicSource;
use crate::storage::StorageError;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

pub const STATE: &str = "recorder-state";
pub const LEVELS: &str = "recorder-levels";
pub const NOTICE: &str = "recorder-notice";

#[derive(Default)]
pub struct RecorderState {
    engine: Mutex<Engine>,
    auto_paused: AtomicBool,
    restarting_mic: Arc<AtomicBool>,
    restarting_output: Arc<AtomicBool>,
    #[cfg(target_os = "macos")]
    watchers: Mutex<Option<(platform::macos::devices::DeviceWatcher, platform::macos::power::SleepWatcher)>>,
}

impl RecorderState {
    pub fn lock(&self) -> MutexGuard<'_, Engine> {
        self.engine.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StatePayload {
    #[serde(flatten)]
    status: Status,
    on_built_in_speakers: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub text: String,
    pub kind: &'static str, // "info" | "warning"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_pane: Option<&'static str>,
}

#[derive(Serialize, Clone)]
pub struct Levels {
    me: f32,
    them: f32,
}

fn speakers() -> bool {
    #[cfg(target_os = "macos")]
    return platform::macos::devices::on_builtin_speakers();
    #[cfg(not(target_os = "macos"))]
    return false;
}

fn payload(engine: &Engine) -> StatePayload {
    StatePayload { status: engine.status(platform::host_now_ns()), on_built_in_speakers: speakers() }
}

fn emit_state(app: &AppHandle) {
    let rec = app.state::<RecorderState>();
    let p = payload(&rec.lock());
    let _ = app.emit(STATE, p);
}

fn notice(app: &AppHandle, text: String, kind: &'static str, settings_pane: Option<&'static str>) {
    let _ = app.emit(NOTICE, Notice { text, kind, settings_pane });
}

fn err(e: String) -> StorageError {
    StorageError::NotYetAvailable(e)
}

fn system_source() -> Box<dyn crate::recorder::source::AudioSource> {
    #[cfg(target_os = "macos")]
    return Box::new(platform::macos::system_tap::SystemTap::default());
    #[cfg(not(target_os = "macos"))]
    return Box::new(MicSource::default()); // replaced by WASAPI loopback in the Windows port
}

#[tauri::command]
pub fn recorder_start(app: AppHandle, state: State<AppState>, rec: State<RecorderState>, app_id: Option<String>, app_label: Option<String>) -> CmdResult<StatePayload> {
    let mut engine = rec.lock();
    let storage = state.lock();
    let started_at = chrono::Local::now().fixed_offset();
    engine
        .start(
            &storage,
            app_id.as_deref().unwrap_or("other"),
            app_label.as_deref().unwrap_or("Call"),
            Box::new(MicSource::default()),
            system_source(),
            platform::host_now_ns(),
            started_at,
        )
        .map_err(err)?;
    let st = engine.status(platform::host_now_ns());
    drop(storage);
    if !st.system_audio {
        notice(&app, "Talaan can't hear the call audio, so only your side is recorded. Allow system audio recording in System Settings.".into(), "warning", Some("system-audio"));
    }
    rec.auto_paused.store(false, Ordering::Relaxed);
    let p = payload(&engine);
    drop(engine);
    notify(&app, MEETINGS_CHANGED);
    let _ = app.emit(STATE, p.clone());
    Ok(p)
}

#[tauri::command]
pub fn recorder_pause(app: AppHandle, rec: State<RecorderState>) -> CmdResult<()> {
    rec.lock().pause(platform::host_now_ns());
    rec.auto_paused.store(false, Ordering::Relaxed);
    emit_state(&app);
    Ok(())
}

#[tauri::command]
pub fn recorder_resume(app: AppHandle, rec: State<RecorderState>) -> CmdResult<()> {
    rec.lock().resume(platform::host_now_ns());
    emit_state(&app);
    Ok(())
}

#[tauri::command]
pub fn recorder_mark(app: AppHandle, rec: State<RecorderState>, note: Option<String>) -> CmdResult<()> {
    rec.lock().mark(platform::host_now_ns(), note);
    emit_state(&app);
    Ok(())
}

#[tauri::command]
pub fn recorder_stop(app: AppHandle, state: State<AppState>, rec: State<RecorderState>) -> CmdResult<String> {
    let mut engine = rec.lock();
    let storage = state.lock();
    let id = engine.stop(&storage, platform::host_now_ns()).map_err(err)?;
    drop(storage);
    drop(engine);
    notify(&app, MEETINGS_CHANGED);
    emit_state(&app);
    Ok(id)
}

#[tauri::command]
pub fn recorder_discard(app: AppHandle, state: State<AppState>, rec: State<RecorderState>) -> CmdResult<()> {
    let mut engine = rec.lock();
    let storage = state.lock();
    engine.discard(&storage, platform::host_now_ns()).map_err(err)?;
    drop(storage);
    drop(engine);
    notify(&app, MEETINGS_CHANGED);
    emit_state(&app);
    Ok(())
}

#[tauri::command]
pub fn recorder_status(rec: State<RecorderState>) -> StatePayload {
    payload(&rec.lock())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    microphone: &'static str,
}

#[tauri::command]
pub fn permissions_status() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    return PermissionStatus { microphone: platform::macos::permissions::mic_status() };
    #[cfg(not(target_os = "macos"))]
    return PermissionStatus { microphone: "granted" };
}

#[tauri::command]
pub async fn permissions_request_mic() -> bool {
    #[cfg(target_os = "macos")]
    return tauri::async_runtime::spawn_blocking(platform::macos::permissions::request_mic).await.unwrap_or(false);
    #[cfg(not(target_os = "macos"))]
    return true;
}

#[tauri::command]
pub fn permissions_open_settings(pane: String) -> CmdResult<()> {
    #[cfg(target_os = "macos")]
    return platform::macos::permissions::open_settings(&pane).map_err(err);
    #[cfg(not(target_os = "macos"))]
    return Err(err(format!("No settings pane {pane} on this platform")));
}

/// Recover an interrupted recording: repair the audio files and make the meeting usable.
#[tauri::command]
pub fn meetings_recover(app: AppHandle, state: State<AppState>, id: String) -> CmdResult<()> {
    crate::recorder::recovery::recover(&state.lock(), &id)?;
    notify(&app, MEETINGS_CHANGED);
    Ok(())
}

/// Rebuilds a source after a device change. Coalesces bursts (AirPods fire several changes) and runs off the Core Audio thread.
fn restart_later(app: AppHandle, flag: Arc<AtomicBool>, mic: bool) {
    if flag.swap(true, Ordering::AcqRel) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        flag.store(false, Ordering::Release);
        let rec = app.state::<RecorderState>();
        let mut engine = rec.lock();
        if !engine.is_active() {
            return;
        }
        let result = if mic { engine.restart_mic() } else { engine.restart_system() };
        drop(engine);
        match (mic, result) {
            (true, Ok(name)) => notice(&app, format!("Switched to {name}. Recording continues."), "info", None),
            (false, Ok(name)) => notice(&app, format!("Sound now plays through {name}. Recording continues."), "info", None),
            (true, Err(e)) => notice(&app, format!("Couldn't switch microphones: {e}"), "warning", Some("microphone")),
            (false, Err(e)) => notice(&app, format!("Couldn't follow the new output device: {e}"), "warning", None),
        }
        emit_state(&app);
    });
}

/// Starts the watchers and the 10 Hz loop. Called once at startup.
pub fn setup(app: &AppHandle) {
    app.manage(RecorderState::default());

    #[cfg(target_os = "macos")]
    {
        use platform::macos::{devices::DeviceEvent, devices::DeviceWatcher, power::SleepWatcher};
        let rec = app.state::<RecorderState>();
        let (a1, mic_flag, out_flag) = (app.clone(), rec.restarting_mic.clone(), rec.restarting_output.clone());
        let devices = DeviceWatcher::new(move |e| match e {
            DeviceEvent::InputChanged => restart_later(a1.clone(), mic_flag.clone(), true),
            DeviceEvent::OutputChanged => restart_later(a1.clone(), out_flag.clone(), false),
        });
        let a2 = app.clone();
        let sleep = SleepWatcher::new(move |sleeping| {
            let rec = a2.state::<RecorderState>();
            let mut engine = rec.lock();
            if sleeping {
                if engine.pause(platform::host_now_ns()) {
                    rec.auto_paused.store(true, Ordering::Relaxed);
                }
            } else if rec.auto_paused.swap(false, Ordering::Relaxed) {
                engine.resume(platform::host_now_ns());
            }
            drop(engine);
            emit_state(&a2);
        });
        if let Ok(devices) = devices {
            *rec.watchers.lock().unwrap() = Some((devices, sleep));
        }
    }

    let app = app.clone();
    std::thread::spawn(move || {
        let mut last_state = Instant::now();
        let mut silent_since: Option<Instant> = None;
        let mut warned = false;
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let rec = app.state::<RecorderState>();
            let engine = rec.lock();
            if !engine.is_active() {
                silent_since = None;
                warned = false;
                continue;
            }
            let (me, them) = engine.levels();
            let paused = engine.status(platform::host_now_ns()).state == "paused";
            drop(engine);
            let _ = app.emit(LEVELS, Levels { me, them });

            // Digital silence from the tap while something plays = system audio is blocked.
            #[cfg(target_os = "macos")]
            {
                let playing = platform::macos::devices::output_is_running();
                if them == 0.0 && playing && !paused {
                    let since = *silent_since.get_or_insert_with(Instant::now);
                    if !warned && since.elapsed() > Duration::from_secs(10) {
                        warned = true;
                        notice(&app, "No call audio is coming through. If the other people are talking, allow Talaan under Screen & System Audio Recording.".into(), "warning", Some("system-audio"));
                    }
                } else if them > 0.0 {
                    silent_since = None;
                }
            }

            if last_state.elapsed() >= Duration::from_secs(1) {
                last_state = Instant::now();
                let state = app.state::<AppState>();
                {
                    let rec = app.state::<RecorderState>();
                    let mut engine = rec.lock();
                    engine.tick(&state.lock(), platform::host_now_ns());
                }
                emit_state(&app);
            }
        }
    });
}

/// On quit: finish a running recording so nothing is lost.
pub fn stop_on_exit(app: &AppHandle) {
    let Some(rec) = app.try_state::<RecorderState>() else { return };
    let mut engine = rec.lock();
    if engine.is_active() {
        let state = app.state::<AppState>();
        let _ = engine.stop(&state.lock(), platform::host_now_ns());
    }
}
