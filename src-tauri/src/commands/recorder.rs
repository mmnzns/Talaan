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
    watchers: Mutex<Option<(platform::devices::DeviceWatcher, platform::power::SleepWatcher)>>,
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
    platform::devices::on_builtin_speakers()
}

fn payload(engine: &Engine) -> StatePayload {
    StatePayload { status: engine.status(platform::host_now_ns()), on_built_in_speakers: speakers() }
}

fn emit_state(app: &AppHandle) {
    let rec = app.state::<RecorderState>();
    let p = payload(&rec.lock());
    super::tray::update(app, &p.status);
    let _ = app.emit(STATE, p);
}

fn notice(app: &AppHandle, text: String, kind: &'static str, settings_pane: Option<&'static str>) {
    let _ = app.emit(NOTICE, Notice { text, kind, settings_pane });
}

fn err(e: String) -> StorageError {
    StorageError::Refused(e)
}

fn system_source() -> Box<dyn crate::recorder::source::AudioSource> {
    Box::new(platform::system_tap::SystemTap::default())
}

#[tauri::command]
pub fn recorder_start(app: AppHandle, app_id: Option<String>, app_label: Option<String>) -> CmdResult<StatePayload> {
    start_recording(&app, app_id.as_deref().unwrap_or("other"), app_label.as_deref().unwrap_or("Call"))
}

/// Starts a recording (Record button, an accepted nudge, or an app on Always).
pub fn start_recording(app: &AppHandle, app_id: &str, app_label: &str) -> CmdResult<StatePayload> {
    let state = app.state::<AppState>();
    let rec = app.state::<RecorderState>();
    let mut engine = rec.lock();
    let storage = state.lock();
    let started_at = chrono::Local::now().fixed_offset();
    let mic = storage.settings_get().ok().flatten().and_then(|s| s["microphone"].as_str().map(String::from)).unwrap_or_default();
    engine
        .start(&storage, app_id, app_label, Box::new(MicSource::with_device(&mic)), system_source(), platform::host_now_ns(), started_at)
        .map_err(err)?;
    let st = engine.status(platform::host_now_ns());
    if let Some(id) = &st.meeting_id {
        if let Some(tag) = auto_tag(storage.settings_get().ok().flatten().as_ref(), app_id, app_label) {
            let _ = storage.set_tags(id, &[tag]);
        }
    }
    drop(storage);
    if let Some(id) = &st.meeting_id {
        super::calendar::link_soon(app, id);
    }
    if !st.system_audio {
        notice(app, "Talaan can't hear the call audio, so only your side is recorded. Allow system audio recording in System Settings.".into(), "warning", Some("system-audio"));
    }
    rec.auto_paused.store(false, Ordering::Relaxed);
    let p = payload(&engine);
    drop(engine);
    notify(app, MEETINGS_CHANGED);
    super::tray::update(app, &p.status);
    let _ = app.emit(STATE, p.clone());
    Ok(p)
}

/// The call app's name as a tag, when "Tag meetings by call app" is on. Manual recordings ("other") get none.
fn auto_tag(settings: Option<&serde_json::Value>, app_id: &str, app_label: &str) -> Option<String> {
    let on = settings.and_then(|s| s["autoTagApps"].as_bool()).unwrap_or(false);
    (on && app_id != "other" && !app_label.trim().is_empty()).then(|| app_label.trim().to_lowercase())
}

/// Connected microphones for Settings › General.
#[tauri::command]
pub fn audio_inputs() -> Vec<String> {
    crate::recorder::mic::input_devices()
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
pub fn recorder_stop(app: AppHandle) -> CmdResult<String> {
    stop_recording(&app)
}

/// Stops and saves, then hands the meeting to the transcriber.
pub fn stop_recording(app: &AppHandle) -> CmdResult<String> {
    let state = app.state::<AppState>();
    let rec = app.state::<RecorderState>();
    let mut engine = rec.lock();
    let storage = state.lock();
    let id = engine.stop(&storage, platform::host_now_ns()).map_err(err)?;
    drop(storage);
    drop(engine);
    // A call started before its event was on the calendar (or a few minutes early) gets a second chance.
    super::calendar::link_soon(app, &id);
    app.state::<super::transcriber::TranscriberState>().enqueue(&id);
    notify(app, MEETINGS_CHANGED);
    emit_state(app);
    Ok(id)
}

/// True while recording or paused.
pub fn is_recording(app: &AppHandle) -> bool {
    app.state::<RecorderState>().lock().is_active()
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
    PermissionStatus { microphone: platform::permissions::mic_status() }
}

#[tauri::command]
pub async fn permissions_request_mic() -> bool {
    tauri::async_runtime::spawn_blocking(platform::permissions::request_mic).await.unwrap_or(false)
}

/// Onboarding: shows the system-audio prompt now rather than at the first call, by opening the tap for a
/// moment (the audio is thrown away). There's no public way to read the answer, so this only reports whether
/// the tap could start.
#[tauri::command]
pub async fn permissions_request_system_audio(app: AppHandle) -> bool {
    if is_recording(&app) {
        return true; // already capturing; asking again would only disturb the recording
    }
    tauri::async_runtime::spawn_blocking(|| {
        use crate::recorder::source::AudioSource;
        let (producer, _consumer) = crate::recorder::feed::feed();
        let mut tap = platform::system_tap::SystemTap::default();
        let ok = tap.start(std::sync::Arc::new(std::sync::Mutex::new(producer))).is_ok();
        if platform::permissions::SYSTEM_AUDIO_PROMPT {
            // Long enough for the system to show its prompt; the prompt stays up on its own after this.
            std::thread::sleep(std::time::Duration::from_millis(1500));
        }
        tap.stop();
        ok
    })
    .await
    .unwrap_or(false)
}

#[tauri::command]
pub fn permissions_open_settings(pane: String) -> CmdResult<()> {
    platform::permissions::open_settings(&pane).map_err(err)
}

/// Recover an interrupted recording: repair the audio files and make the meeting usable.
#[tauri::command]
pub fn meetings_recover(app: AppHandle, state: State<AppState>, id: String) -> CmdResult<()> {
    crate::recorder::recovery::recover(&state.lock(), &id)?;
    app.state::<super::transcriber::TranscriberState>().enqueue(&id);
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

    use platform::{devices::DeviceEvent, devices::DeviceWatcher, power::SleepWatcher};
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

    let app = app.clone();
    std::thread::spawn(move || {
        let mut last_state = Instant::now();
        let mut silent_since: Option<Instant> = None;
        let mut warned = false;
        // A blocked tap delivers zeros from the first sample. Once any sound came through, it isn't blocked.
        let mut heard_them = false;
        // System audio gave up mid-recording (it reconnects on its own first). Said once per recording.
        let mut warned_lost = false;
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let rec = app.state::<RecorderState>();
            let engine = rec.lock();
            if !engine.is_active() {
                silent_since = None;
                warned = false;
                heard_them = false;
                warned_lost = false;
                continue;
            }
            let (me, them) = engine.levels();
            let paused = engine.status(platform::host_now_ns()).state == "paused";
            let system_failed = engine.system_failed();
            drop(engine);
            let _ = app.emit(LEVELS, Levels { me, them });

            if system_failed && !warned_lost {
                warned_lost = true;
                notice(&app, "Call audio stopped coming through, so only your side is being recorded from here.".into(), "warning", None);
            } else if !system_failed {
                // Back (an output switch restarted it): a later failure warns again.
                warned_lost = false;
            }

            heard_them |= them > 0.0;

            // Digital silence from the tap since the start while something plays = system audio is blocked.
            // (Apps keep output streams running while quiet, so silence after real sound means nothing.)
            if !heard_them {
                let playing = platform::devices::others_playing();
                if them == 0.0 && playing && !paused {
                    let since = *silent_since.get_or_insert_with(Instant::now);
                    if !warned && since.elapsed() > Duration::from_secs(10) {
                        warned = true;
                        notice(&app, "No call audio is coming through. If the other people are talking, allow Talaan under Screen & System Audio Recording.".into(), "warning", Some("system-audio"));
                    }
                } else if paused || !playing {
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

#[cfg(test)]
mod tests {
    use super::auto_tag;
    use serde_json::json;

    #[test]
    fn calls_are_tagged_by_app_only_when_turned_on() {
        let on = json!({ "autoTagApps": true });
        assert_eq!(auto_tag(Some(&on), "zoom", "Zoom"), Some("zoom".into()));
        assert_eq!(auto_tag(Some(&on), "browser", "Google Meet"), Some("google meet".into()));
        assert_eq!(auto_tag(Some(&on), "other", "Call"), None);
        assert_eq!(auto_tag(Some(&json!({})), "zoom", "Zoom"), None);
        assert_eq!(auto_tag(None, "zoom", "Zoom"), None);
    }
}
