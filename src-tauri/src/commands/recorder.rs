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

// The "can't hear the call" notices (i18n keys): macOS points at its privacy pane, Windows at Sound settings
// (process loopback has no permission to grant).
#[cfg(not(target_os = "windows"))]
const NO_SYSTEM_AUDIO: &str = "notice.no_system_audio";
#[cfg(target_os = "windows")]
const NO_SYSTEM_AUDIO: &str = "notice.no_system_audio_windows";
#[cfg(not(target_os = "windows"))]
const NO_CALL_AUDIO: &str = "notice.no_call_audio";
#[cfg(target_os = "windows")]
const NO_CALL_AUDIO: &str = "notice.no_call_audio_windows";

#[derive(Default)]
pub struct RecorderState {
    engine: Mutex<Engine>,
    auto_paused: AtomicBool,
    restarting_mic: Arc<AtomicBool>,
    restarting_output: Arc<AtomicBool>,
    watchers: Mutex<Option<(platform::devices::DeviceWatcher, platform::power::SleepWatcher)>>,
    /// The guided test recording, while it runs: it stops itself after `max_ms`.
    test: Mutex<Option<TestRun>>,
}

/// The guided test recording (onboarding's "Try a 1-minute test recording").
#[derive(Debug, Clone, PartialEq)]
pub struct TestRun {
    pub meeting_id: String,
    pub max_ms: u64,
}

/// What the frontend asks for when it starts the test.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestOptions {
    max_ms: u64,
}

/// The tag that marks the test meeting (and the frontend's offer to delete it).
pub const TEST_TAG: &str = "test";

/// True once the test has recorded for its whole length (paused time doesn't count).
fn test_limit_reached(run: Option<&TestRun>, status: &Status) -> bool {
    run.is_some_and(|r| status.meeting_id.as_deref() == Some(r.meeting_id.as_str()) && status.elapsed_ms >= r.max_ms)
}

/// The test records the microphone only: it's about hearing the person speak, and on the Mac opening system
/// audio could show a permission prompt in the middle of it. Its "Them" track is silence.
struct MicOnly;

impl crate::recorder::source::AudioSource for MicOnly {
    fn start(&mut self, _feed: crate::recorder::source::SharedFeed) -> Result<String, String> {
        Err("microphone only".into())
    }
    fn restart(&mut self) -> Result<String, String> {
        Err("microphone only".into())
    }
    fn stop(&mut self) {}
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

/// `on_built_in_speakers` comes from `speakers()`, asked before taking the engine lock: on Windows it can read
/// every output's meter.
fn payload(engine: &Engine, on_built_in_speakers: bool) -> StatePayload {
    StatePayload { status: engine.status(platform::host_now_ns()), on_built_in_speakers }
}

fn emit_state(app: &AppHandle) {
    let rec = app.state::<RecorderState>();
    let on_speakers = speakers();
    let p = payload(&rec.lock(), on_speakers);
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

/// `async` (off the main thread): it opens the audio devices, as an auto-start from the calls loop already does.
#[tauri::command(async)]
pub fn recorder_start(app: AppHandle, app_id: Option<String>, app_label: Option<String>, test: Option<TestOptions>) -> CmdResult<StatePayload> {
    start_with(&app, app_id.as_deref().unwrap_or("other"), app_label.as_deref().unwrap_or("Call"), test.map(|t| t.max_ms))
}

/// Starts a recording (Record button, an accepted nudge, or an app on Always).
pub fn start_recording(app: &AppHandle, app_id: &str, app_label: &str) -> CmdResult<StatePayload> {
    start_with(app, app_id, app_label, None)
}

/// `test_max_ms`: the guided test recording. It is titled and tagged as a test from the start, records the
/// microphone only, isn't linked to a calendar event, and stops itself after that long.
fn start_with(app: &AppHandle, app_id: &str, app_label: &str, test_max_ms: Option<u64>) -> CmdResult<StatePayload> {
    let state = app.state::<AppState>();
    let rec = app.state::<RecorderState>();
    let on_speakers = speakers();
    let mut engine = rec.lock();
    let storage = state.lock();
    let started_at = chrono::Local::now().fixed_offset();
    let mic = storage.settings_get().ok().flatten().and_then(|s| s["microphone"].as_str().map(String::from)).unwrap_or_default();
    let system: Box<dyn crate::recorder::source::AudioSource> = if test_max_ms.is_some() { Box::new(MicOnly) } else { system_source() };
    engine
        .start(&storage, app_id, app_label, Box::new(MicSource::with_device(&mic)), system, platform::host_now_ns(), started_at)
        .map_err(err)?;
    let st = engine.status(platform::host_now_ns());
    log::info!(
        "recording started: meeting {}, app {app_id}, microphone {}, call audio {}, built-in speakers {on_speakers}",
        st.meeting_id.as_deref().unwrap_or("?"),
        crate::logging::device(&st.mic_name),
        if st.system_audio { "on" } else { "off" }
    );
    if let Some(id) = &st.meeting_id {
        if test_max_ms.is_some() {
            let _ = storage.rename_meeting(id, &crate::i18n::tr("recorder.test_title"));
            let _ = storage.set_tags(id, &[TEST_TAG.to_string()]);
        } else if let Some(tag) = auto_tag(storage.settings_get().ok().flatten().as_ref(), app_id, app_label) {
            let _ = storage.set_tags(id, &[tag]);
        }
    }
    drop(storage);
    *rec.test.lock().unwrap_or_else(|e| e.into_inner()) = match (test_max_ms, &st.meeting_id) {
        (Some(max_ms), Some(id)) => Some(TestRun { meeting_id: id.clone(), max_ms }),
        _ => None,
    };
    if test_max_ms.is_none() {
        if let Some(id) = &st.meeting_id {
            super::calendar::link_soon(app, id);
        }
        if !st.system_audio {
            log::warn!("shown: {NO_SYSTEM_AUDIO}");
            notice(app, crate::i18n::tr(NO_SYSTEM_AUDIO), "warning", Some("system-audio"));
        }
    }
    rec.auto_paused.store(false, Ordering::Relaxed);
    let p = payload(&engine, on_speakers);
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
    log::info!("recording paused");
    rec.auto_paused.store(false, Ordering::Relaxed);
    emit_state(&app);
    Ok(())
}

#[tauri::command]
pub fn recorder_resume(app: AppHandle, rec: State<RecorderState>) -> CmdResult<()> {
    rec.lock().resume(platform::host_now_ns());
    log::info!("recording resumed");
    emit_state(&app);
    Ok(())
}

#[tauri::command]
pub fn recorder_mark(app: AppHandle, rec: State<RecorderState>, note: Option<String>) -> CmdResult<()> {
    rec.lock().mark(platform::host_now_ns(), note);
    emit_state(&app);
    Ok(())
}

/// `async` (off the main thread): stopping joins the audio threads and closes the files.
#[tauri::command(async)]
pub fn recorder_stop(app: AppHandle) -> CmdResult<String> {
    stop_recording(&app)
}

/// Stops and saves, then hands the meeting to the transcriber.
pub fn stop_recording(app: &AppHandle) -> CmdResult<String> {
    let state = app.state::<AppState>();
    let rec = app.state::<RecorderState>();
    let mut engine = rec.lock();
    let storage = state.lock();
    let elapsed = engine.status(platform::host_now_ns()).elapsed_ms;
    let id = engine.stop(&storage, platform::host_now_ns()).map_err(err)?;
    drop(engine);
    let transcriber = app.state::<super::transcriber::TranscriberState>();
    transcriber.stamp_estimate(&storage, &id);
    drop(storage);
    log::info!("recording stopped: meeting {id}, {}s recorded", elapsed / 1000);
    let was_test = rec.test.lock().unwrap_or_else(|e| e.into_inner()).take().is_some_and(|t| t.meeting_id == id);
    // A call started before its event was on the calendar (or a few minutes early) gets a second chance. The test
    // recording is never a calendar event.
    if !was_test {
        super::calendar::link_soon(app, &id);
    }
    transcriber.enqueue(&id);
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
    *rec.test.lock().unwrap_or_else(|e| e.into_inner()) = None;
    log::info!("recording discarded");
    notify(&app, MEETINGS_CHANGED);
    emit_state(&app);
    Ok(())
}

#[tauri::command(async)]
pub fn recorder_status(rec: State<RecorderState>) -> StatePayload {
    let on_speakers = speakers();
    payload(&rec.lock(), on_speakers)
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
    log::info!("recovering interrupted meeting {id}");
    crate::recorder::recovery::recover(&state.lock(), &id)?;
    app.state::<super::transcriber::TranscriberState>().enqueue(&id);
    notify(&app, MEETINGS_CHANGED);
    Ok(())
}

/// An output change during the mic-only test recording leaves it alone: there's no system audio to rebuild.
fn output_restart_skipped(mic: bool, test_running: bool) -> bool {
    !mic && test_running
}

/// Rebuilds a source after a device change. Coalesces bursts (AirPods fire several changes) and runs off the Core Audio thread.
fn restart_later(app: AppHandle, flag: Arc<AtomicBool>, mic: bool) {
    if flag.swap(true, Ordering::AcqRel) {
        return;
    }
    let reset = flag.clone();
    let spawned = std::thread::Builder::new().name("talaan-device-restart".into()).spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        flag.store(false, Ordering::Release);
        let rec = app.state::<RecorderState>();
        let mut engine = rec.lock();
        if !engine.is_active() {
            return;
        }
        // The test recording has no system audio to move to the new output (MicOnly would only fail).
        if output_restart_skipped(mic, rec.test.lock().unwrap_or_else(|e| e.into_inner()).is_some()) {
            return;
        }
        let result = if mic { engine.restart_mic() } else { engine.restart_system() };
        drop(engine);
        let which = if mic { "microphone" } else { "call audio" };
        match &result {
            Ok(device_name) => log::info!("{which} switched to {}", crate::logging::device(device_name)),
            Err(e) => log::warn!("{which} couldn't follow the device change: {}", crate::logging::text(e)),
        }
        match (mic, result) {
            (true, Ok(name)) => notice(&app, crate::i18n::tr_with("notice.mic_switched", &[("name", name.as_str())]), "info", None),
            (false, Ok(name)) => notice(&app, crate::i18n::tr_with("notice.output_switched", &[("name", name.as_str())]), "info", None),
            (true, Err(e)) => notice(&app, crate::i18n::tr_with("notice.mic_switch_failed", &[("error", e.as_str())]), "warning", Some("microphone")),
            (false, Err(e)) => notice(&app, crate::i18n::tr_with("notice.output_failed", &[("error", e.as_str())]), "warning", None),
        }
        emit_state(&app);
    });
    if let Err(e) = spawned {
        // No thread, no restart: log it and let the next device change try again.
        log::error!("couldn't start the device restart thread: {}", crate::logging::text(&e));
        reset.store(false, Ordering::Release);
    }
}

/// Starts the watchers and the 10 Hz loop. Called once at startup.
pub fn setup(app: &AppHandle) {
    app.manage(RecorderState::default());

    use platform::{devices::DeviceEvent, devices::DeviceWatcher, power::SleepWatcher};
    let rec = app.state::<RecorderState>();
    let (a1, mic_flag, out_flag) = (app.clone(), rec.restarting_mic.clone(), rec.restarting_output.clone());
    let devices = DeviceWatcher::new(move |e| match e {
        DeviceEvent::InputChanged => {
            log::info!("the default microphone changed");
            restart_later(a1.clone(), mic_flag.clone(), true)
        }
        DeviceEvent::OutputChanged => {
            log::info!("the default speakers or headset changed");
            restart_later(a1.clone(), out_flag.clone(), false)
        }
    });
    let a2 = app.clone();
    let sleep = SleepWatcher::new(move |sleeping| {
        let rec = a2.state::<RecorderState>();
        let mut engine = rec.lock();
        if sleeping {
            if engine.pause(platform::host_now_ns()) {
                rec.auto_paused.store(true, Ordering::Relaxed);
                log::info!("the computer is going to sleep: recording paused");
            }
        } else if rec.auto_paused.swap(false, Ordering::Relaxed) {
            engine.resume(platform::host_now_ns());
            log::info!("the computer woke up: recording resumed");
        }
        drop(engine);
        emit_state(&a2);
    });
    match devices {
        Ok(devices) => *rec.watchers.lock().unwrap() = Some((devices, sleep)),
        Err(e) => log::warn!("device watcher unavailable: {}", crate::logging::text(&e)),
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
            let status = engine.status(platform::host_now_ns());
            let paused = status.state == "paused";
            let system_failed = engine.system_failed();
            drop(engine);
            let test = rec.test.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if test_limit_reached(test.as_ref(), &status) {
                // The guided test is over: saved and transcribed like any recording.
                let _ = stop_recording(&app);
                continue;
            }
            let _ = app.emit(LEVELS, Levels { me, them });

            if system_failed && !warned_lost {
                warned_lost = true;
                log::warn!("shown: notice.call_audio_lost");
                notice(&app, crate::i18n::tr("notice.call_audio_lost"), "warning", None);
            } else if !system_failed {
                // Back (an output switch restarted it): a later failure warns again.
                warned_lost = false;
            }

            heard_them |= them > 0.0;

            // Digital silence from the tap since the start while something plays = system audio is blocked.
            // (Apps keep output streams running while quiet, so silence after real sound means nothing.)
            // The test records the microphone only, so the other side is silent on purpose.
            if !heard_them && test.is_none() {
                let playing = platform::devices::others_playing();
                if them == 0.0 && playing && !paused {
                    let since = *silent_since.get_or_insert_with(Instant::now);
                    if !warned && since.elapsed() > Duration::from_secs(10) {
                        warned = true;
                        log::warn!("shown: {NO_CALL_AUDIO} (only silence from the call audio while something plays)");
                        notice(&app, crate::i18n::tr(NO_CALL_AUDIO), "warning", Some("system-audio"));
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
        let saved = engine.stop(&state.lock(), platform::host_now_ns());
        log::info!("quitting while recording: saved {}", saved.as_deref().unwrap_or("nothing"));
    }
}

/// "Delete everything" while recording: drop the recording (its files are about to go) so nothing writes into
/// the emptied folder and Windows has no open WAV files blocking the delete.
pub fn discard_for_wipe(app: &AppHandle) {
    let Some(rec) = app.try_state::<RecorderState>() else { return };
    let mut engine = rec.lock();
    if engine.is_active() {
        let state = app.state::<AppState>();
        let _ = engine.discard(&state.lock(), platform::host_now_ns());
    }
}

#[cfg(test)]
mod tests {
    use super::{auto_tag, test_limit_reached, MicOnly, TestRun};
    use crate::recorder::engine::Status;
    use crate::recorder::source::AudioSource;
    use serde_json::json;

    fn status(id: &str, elapsed_ms: u64) -> Status {
        Status { state: "recording", meeting_id: Some(id.into()), elapsed_ms, marks: 0, notes: 0, mic_name: String::new(), system_audio: false }
    }

    #[test]
    fn the_test_recording_stops_at_its_limit_and_only_itself() {
        let run = TestRun { meeting_id: "m1".into(), max_ms: 60_000 };
        assert!(!test_limit_reached(Some(&run), &status("m1", 59_999)));
        assert!(test_limit_reached(Some(&run), &status("m1", 60_000)));
        assert!(!test_limit_reached(Some(&run), &status("m2", 90_000)));
        assert!(!test_limit_reached(None, &status("m1", 90_000)));
    }

    #[test]
    fn an_output_change_leaves_the_mic_only_test_alone() {
        assert!(super::output_restart_skipped(false, true));
        assert!(!super::output_restart_skipped(true, true)); // a new microphone is still picked up
        assert!(!super::output_restart_skipped(false, false));
    }

    #[test]
    fn the_test_records_the_microphone_only() {
        let (p, _c) = crate::recorder::feed::feed_with_capacity(10, 2);
        assert!(MicOnly.start(std::sync::Arc::new(std::sync::Mutex::new(p))).is_err());
    }

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
