//! Summaries: the worker, Generate summary, local AI models, API keys and local servers for Settings.

use super::{notify, AppState, CmdResult, MEETINGS_CHANGED};
use crate::keys;
use crate::storage::model::MeetingStatus;
use crate::i18n::{tr, tr_with};
use crate::storage::StorageError;
use crate::summarizer::engines::{compatible_base, default_base, list_models, recommended_model, HttpEngine, LocalEngine, Provider, CLOUD_CONTEXT, LOCAL_CONTEXT};
use crate::gpu::{Gpu, Kind, Plan};
use crate::summarizer::local::{helper_path, GpuHelper, LocalAi, GPU_STALL};
use crate::summarizer::models::{local_model, LLM_CATALOG};
use crate::summarizer::summarize::{EngineError, SummaryEngine};
use crate::summarizer::followup::{self, FollowUpError};
use crate::summarizer::templates;
use crate::summarizer::worker::{Config, Deps, Worker, NO_ENGINE};
use std::sync::atomic::{AtomicBool, Ordering};
use crate::transcriber::models::{Location, ModelStore};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

pub const SUMMARY_READY: &str = "summary-ready";

pub struct SummarizerState {
    worker: Worker,
    data_dir: PathBuf,
    /// Follow-up drafts in progress, by the request id the screen gave, with the flag its Cancel sets.
    followups: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl SummarizerState {
    pub fn enqueue(&self, id: &str) {
        self.worker.enqueue(id);
    }
    /// Straight from the transcriber: ending without a summary posts "Transcript ready".
    pub fn enqueue_auto(&self, id: &str) {
        self.worker.enqueue_auto(id);
    }
    pub fn cancel_if_running(&self, id: &str) {
        self.worker.cancel_if_running(id);
    }
}

fn refused(message: impl Into<String>) -> StorageError {
    StorageError::Refused(message.into())
}

/// Turns settings into an engine, or says (in words for the user) why there isn't one.
pub fn make_engine(cfg: &Config, data_dir: &Path) -> Result<Box<dyn SummaryEngine>, EngineError> {
    engine_for(cfg, data_dir, false)
}

/// `followup`: for a follow-up draft, whose GPU failures are remembered only when they are the GPU's own fault
/// (`gpu_fault`), so a short draft that ran out of memory never moves every later summary to the processor.
fn engine_for(cfg: &Config, data_dir: &Path, followup: bool) -> Result<Box<dyn SummaryEngine>, EngineError> {
    let s = &cfg.settings;
    match cfg.engine.as_str() {
        "ollama" => {
            let base = compatible_base(s["ollamaAddress"].as_str().unwrap_or("http://localhost:11434"));
            let model = match s["ollamaModel"].as_str().filter(|m| !m.is_empty()) {
                Some(m) => m.to_string(),
                None => list_models(Provider::Compatible, &base, "")?
                    .into_iter()
                    .find(|m| !m.contains("embed"))
                    .ok_or_else(|| EngineError::Unavailable(tr("ai.server_no_models")))?,
            };
            Ok(Box::new(HttpEngine { provider: Provider::Compatible, base, key: None, label: format!("Local server · {model}"), model, context: LOCAL_CONTEXT }))
        }
        "byok" => {
            let provider = Provider::parse(s["byokProvider"].as_str().unwrap_or("claude")).unwrap_or(Provider::Claude);
            let key = keys::get(provider.key_id());
            if key.is_none() && provider != Provider::Compatible {
                return Err(EngineError::Unavailable(tr_with("ai.add_key", &[("provider", provider.name())])));
            }
            let key = key.unwrap_or_default();
            let base = match provider {
                Provider::Compatible => compatible_base(s["byokBaseUrl"].as_str().unwrap_or("")),
                p => default_base(p).to_string(),
            };
            let model = match s["byokModel"].as_str().filter(|m| !m.is_empty() && *m != "recommended") {
                Some(m) => m.to_string(),
                None => recommended_model(provider, &list_models(provider, &base, &key)?)
                    .ok_or_else(|| EngineError::Unavailable(tr_with("ai.no_suitable_model", &[("provider", provider.name())])))?,
            };
            let context = if provider == Provider::Compatible { LOCAL_CONTEXT } else { CLOUD_CONTEXT };
            Ok(Box::new(HttpEngine { provider, base, key: Some(key).filter(|k| !k.is_empty()), label: format!("{} · {model}", provider.name()), model, context }))
        }
        _ => {
            let model = local_model(s["localModel"].as_str().unwrap_or("recommended"));
            let Location::Own(dir) = ModelStore::new(data_dir, false).locate(model) else {
                let size = crate::i18n::decimal(model.size() as f64 / 1e9);
                return Err(EngineError::Unavailable(tr_with("ai.download_local", &[("name", model.name), ("size", &size)])));
            };
            Ok(Box::new(LocalEngine { ai: local_ai(data_dir, s["useGpu"].as_bool().unwrap_or(true), followup), model_path: dir.join(model.files[0].name), model_name: model.name.into() }))
        }
    }
}

/// The local AI helper for a summary: on Windows the GPU build first when it should be used (`gpu.rs`), with the
/// processor helper behind it; elsewhere the one helper (Metal on the Mac).
fn local_ai(data_dir: &Path, use_gpu: bool, followup: bool) -> LocalAi {
    if cfg!(windows) {
        let gpu = Gpu::shared(data_dir);
        if let Plan::Gpu { exe, env } = gpu.plan(Kind::Llm, use_gpu) {
            let record = gpu.recorder(Kind::Llm);
            let on_failure: Arc<dyn Fn(&str) + Send + Sync> = if followup { Arc::new(move |reason: &str| if gpu_fault(reason) { record(reason) }) } else { record };
            return LocalAi::with_gpu(&helper_path(), GpuHelper { exe, env, on_failure, stall: GPU_STALL });
        }
    }
    LocalAi::new(&helper_path())
}

/// A GPU failure that says something about the graphics chip itself (it crashed, was lost, the driver can't run
/// it), not about memory that something else was using at the time.
pub fn gpu_fault(reason: &str) -> bool {
    !reason.to_lowercase().contains("memory")
}

/// How a summary with these settings is timed: the local model on the graphics chip (Metal on the Mac; on Windows
/// the Vulkan helper when `gpu.rs` says it should be used) or the processor; a cloud service or local server.
fn summary_profile(cfg: &Config, gpu: &Gpu) -> crate::eta::Profile {
    let s = &cfg.settings;
    match cfg.engine.as_str() {
        "ollama" => crate::eta::summary_profile("ollama", "server", false),
        "byok" => crate::eta::summary_profile("byok", s["byokProvider"].as_str().unwrap_or("claude"), false),
        _ => {
            let model = local_model(s["localModel"].as_str().unwrap_or("recommended"));
            let use_gpu = s["useGpu"].as_bool().unwrap_or(true);
            let on_gpu = if cfg!(windows) { matches!(gpu.plan(Kind::Llm, use_gpu), Plan::Gpu { .. }) } else { cfg!(target_os = "macos") };
            crate::eta::summary_profile("local", model.id, on_gpu)
        }
    }
}

pub fn setup(app: &AppHandle, data_dir: PathBuf) {
    let state = app.state::<AppState>();
    let changed_app = app.clone();
    let ready_app = app.clone();
    let done_app = app.clone();
    let gpu = Gpu::shared(&data_dir);
    let state_dir = data_dir.clone();
    let worker = Worker::start(Deps {
        storage: state.shared(),
        changed: Arc::new(move || notify(&changed_app, MEETINGS_CHANGED)),
        make_engine: Box::new(move |cfg| make_engine(cfg, &data_dir)),
        summarized: Arc::new(move |id, title| {
            let _ = ready_app.emit(SUMMARY_READY, serde_json::json!({ "id": id, "title": title }));
            notify_done(&ready_app, id, "notify.summary_ready", title);
        }),
        unsummarized: Arc::new(move |id, title| notify_done(&done_app, id, "notify.transcript_ready", title)),
        profile: Box::new(move |cfg| summary_profile(cfg, &gpu)),
    });
    let storage = state.lock();
    for id in storage.ids_with_status(MeetingStatus::Summarizing).unwrap_or_default() {
        // An estimate from before the restart means nothing now; the worker makes a new one when it starts.
        let _ = storage.update_meeting_file(&id, |f| f.eta = None);
        worker.enqueue(&id);
    }
    drop(storage);
    app.manage(SummarizerState { worker, data_dir: state_dir, followups: Mutex::default() });
}

pub const OPEN_MEETING: &str = "open-meeting";

/// The "Summary ready" (or, without a summary, "Transcript ready") notification that says a meeting's work is done,
/// unless turned off in Settings › General. `heading` is an i18n key.
pub fn notify_done(app: &AppHandle, id: &str, heading: &str, title: &str) {
    let wanted = app.state::<AppState>().lock().settings_get().ok().flatten().and_then(|s| s["notifySummaryReady"].as_bool()).unwrap_or(true);
    if wanted {
        crate::platform::notify::post(id, &crate::i18n::tr(heading), title);
    }
}

/// Whether the OS will show Talaan's notifications (None: it can't say, or hasn't been asked yet), so the screens
/// only promise one when it will appear.
#[tauri::command(async)]
pub fn notifications_allowed() -> Option<bool> {
    crate::platform::notify::allowed()
}

/// Clicking a "Summary ready" notification: bring Talaan forward on that meeting.
pub fn open_meeting(app: &AppHandle, id: &str) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit(OPEN_MEETING, id);
}

/// A meeting Talaan was started to open (a toast clicked while it wasn't running, Windows). The window can't hear
/// an event before it has loaded, so it asks with `meetings_launch_link` when it does.
static LAUNCH_MEETING: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn open_meeting_at_launch(app: &AppHandle, id: String) {
    *LAUNCH_MEETING.lock().unwrap_or_else(|e| e.into_inner()) = Some(id);
    super::tray::show_main(app);
}

/// The meeting to open at launch, once (see `open_meeting_at_launch`).
#[tauri::command]
pub fn meetings_launch_link() -> Option<String> {
    LAUNCH_MEETING.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// Re-queues meetings that are waiting for a summary engine (after the local model finishes downloading).
pub fn requeue_waiting(app: &AppHandle) {
    let state = app.state::<AppState>();
    let s = app.state::<SummarizerState>();
    let storage = state.lock();
    for id in storage.ids_with_status(MeetingStatus::Ready).unwrap_or_default() {
        let waiting = storage.read_meeting_file(&id).ok().and_then(|f| f.summary_error).is_some_and(|e| e.code == NO_ENGINE);
        if waiting && storage.update_meeting_file(&id, |f| {
            f.status = MeetingStatus::Summarizing;
            f.progress = Some(0.0);
            f.summary_error = None;
        })
        .is_ok()
        {
            s.enqueue(&id);
        }
    }
    drop(storage);
    notify(app, MEETINGS_CHANGED);
}

/// Generate summary (also: regenerate). Needs a transcript.
#[tauri::command]
pub fn meetings_generate_summary(app: AppHandle, state: State<AppState>, s: State<SummarizerState>, id: String) -> CmdResult<()> {
    start_summary(&app, &state, &s, &id, None)
}

/// The meeting type to summarize a meeting as (from the recording window); none for the Settings default.
#[tauri::command]
pub fn meetings_set_template(app: AppHandle, state: State<AppState>, id: String, template: Option<String>) -> CmdResult<()> {
    state.lock().set_template(&id, template.as_deref())?;
    notify(&app, MEETINGS_CHANGED);
    Ok(())
}

/// The meeting's ⋯ menu › Summarize as…: remembers the type and writes the summary again.
#[tauri::command]
pub fn meetings_summarize_as(app: AppHandle, state: State<AppState>, s: State<SummarizerState>, id: String, template: String) -> CmdResult<()> {
    if !templates::exists(&template) {
        return Err(refused(tr("error.unknown_template")));
    }
    start_summary(&app, &state, &s, &id, Some(&template))
}

fn start_summary(app: &AppHandle, state: &State<AppState>, s: &State<SummarizerState>, id: &str, template: Option<&str>) -> CmdResult<()> {
    let id = id.to_string();
    let storage = state.lock();
    let meeting = storage.get_meeting(&id)?;
    if meeting.transcript.is_empty() {
        return Err(refused(tr("error.no_transcript_yet")));
    }
    if matches!(meeting.item.status, MeetingStatus::Recording | MeetingStatus::Transcribing) {
        return Err(refused(tr("error.wait_transcript")));
    }
    storage.update_meeting_file(&id, |f| {
        f.status = MeetingStatus::Summarizing;
        f.progress = Some(0.0);
        f.summary_error = None;
        if let Some(t) = template {
            f.template = Some(t.to_string());
        }
    })?;
    drop(storage);
    s.enqueue(&id);
    notify(app, MEETINGS_CHANGED);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowUpDraft {
    subject: String,
    body: String,
    /// The invitees' addresses from the calendar event, when it had them.
    to: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowUpEta {
    finish_at: String,
    on_cpu: bool,
    /// The local AI is busy with a summary; the draft starts when it's done.
    waiting: bool,
}

/// When a follow-up draft for this meeting should be done (the screen asks again while it waits). With the local AI
/// it waits for a summary in progress, so that summary's time left comes first.
#[tauri::command(async)]
pub fn meetings_followup_eta(state: State<AppState>, s: State<SummarizerState>, id: String) -> CmdResult<FollowUpEta> {
    let storage = state.lock();
    let summary = storage.get_meeting(&id)?.summary.ok_or_else(|| refused(tr("error.no_summary_for_followup")))?;
    let cfg = Config::from_settings(storage.settings_get().ok().flatten().as_ref());
    let profile = summary_profile(&cfg, &Gpu::shared(&s.data_dir));
    let mut secs = followup::estimate(&profile, storage.speed_rate(&profile.key), &summary);
    let busy = if cfg.engine == "local" { s.worker.current() } else { None };
    if let Some(other) = &busy {
        let left = storage
            .read_meeting_file(other)
            .ok()
            .and_then(|f| f.eta)
            .and_then(|e| chrono::DateTime::parse_from_rfc3339(&e.finish_at).ok())
            .map(|t| (t.with_timezone(&chrono::Local) - chrono::Local::now()).num_seconds().max(0) as f64)
            .unwrap_or(0.0);
        secs += left;
    }
    let eta = crate::storage::speed::eta_in(secs, profile.device == crate::eta::Device::Cpu);
    Ok(FollowUpEta { finish_at: eta.finish_at, on_cpu: eta.on_cpu, waiting: busy.is_some() })
}

fn followup_error(e: FollowUpError) -> StorageError {
    match e {
        FollowUpError::Engine(EngineError::Unavailable(m)) | FollowUpError::Engine(EngineError::Failed(m)) => refused(m),
        FollowUpError::Engine(EngineError::Cancelled) => refused(tr("error.cancelled")),
        FollowUpError::BadOutput => refused(tr("error.followup_bad_output")),
    }
}

/// One draft, start to finish: the engine, then the request. Checks Cancel first (it may have waited in the queue).
fn run_followup(cfg: &Config, data_dir: &Path, meeting: &crate::storage::model::Meeting, summary: &crate::storage::model::Summary, cancel: &AtomicBool) -> Result<followup::FollowUp, FollowUpError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(FollowUpError::Engine(EngineError::Cancelled));
    }
    let mut engine = engine_for(cfg, data_dir, true).map_err(FollowUpError::Engine)?;
    followup::draft(meeting, summary, engine.as_mut(), &cfg.language_for(&meeting.language), cancel)
}

/// Draft follow-up email: a short request over the summary the user sees (with their edits), in the summary
/// language. The local AI runs it on the summarizer's thread, after a summary in progress, so only one model is
/// ever loaded; a cloud service or local server answers straight away. `request`: the screen's id for Cancel.
#[tauri::command(async)]
pub fn meetings_draft_followup(state: State<AppState>, s: State<SummarizerState>, id: String, request: String) -> CmdResult<FollowUpDraft> {
    let (meeting, cfg) = {
        let storage = state.lock();
        (storage.get_meeting(&id)?, Config::from_settings(storage.settings_get().ok().flatten().as_ref()))
    };
    let summary = meeting.summary.clone().ok_or_else(|| refused(tr("error.no_summary_for_followup")))?;
    let cancel = Arc::new(AtomicBool::new(false));
    s.followups.lock().unwrap_or_else(|e| e.into_inner()).insert(request.clone(), cancel.clone());
    let to = meeting.calendar_event.as_ref().map(|e| e.emails.clone()).unwrap_or_default();
    let result = if cfg.engine == "local" {
        let (tx, rx) = std::sync::mpsc::channel();
        let data_dir = s.data_dir.clone();
        let flag = cancel.clone();
        s.worker.run_between(Box::new(move || {
            let _ = tx.send(run_followup(&cfg, &data_dir, &meeting, &summary, &flag));
        }));
        rx.recv().unwrap_or(Err(FollowUpError::Engine(EngineError::Cancelled)))
    } else {
        run_followup(&cfg, &s.data_dir, &meeting, &summary, &cancel)
    };
    s.followups.lock().unwrap_or_else(|e| e.into_inner()).remove(&request);
    let f = result.map_err(followup_error)?;
    Ok(FollowUpDraft { subject: f.subject, body: f.body, to })
}

/// Cancel on a follow-up draft: stops that request (waiting or running), not anyone else's.
#[tauri::command]
pub fn meetings_cancel_followup(s: State<SummarizerState>, request: String) {
    if let Some(flag) = s.followups.lock().unwrap_or_else(|e| e.into_inner()).get(&request) {
        flag.store(true, Ordering::Relaxed);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmRow {
    id: &'static str,
    /// "recommended" | "larger": the settings value that picks it.
    choice: &'static str,
    name: &'static str,
    note: &'static str,
    size_mb: u64,
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<f32>,
    /// What an interrupted download left on disk (0..1); Download resumes from it.
    #[serde(skip_serializing_if = "Option::is_none")]
    partial: Option<f32>,
    /// Why the last download failed, until the next try.
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// The partial download was stopped with Cancel: it waits for a click (no resume at launch).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    cancelled: bool,
}

#[tauri::command]
pub fn llm_models_list(t: State<super::transcriber::TranscriberState>) -> CmdResult<Vec<LlmRow>> {
    let store = ModelStore::new(t.data_dir(), false);
    Ok(LLM_CATALOG
        .iter()
        .zip(["recommended", "larger"])
        .map(|(m, choice)| {
            let (state, progress) = match (t.download_progress(m.id), store.locate(m)) {
                (Some(p), _) => ("downloading", Some(p)),
                (None, Location::Missing) => ("not_downloaded", None),
                (None, _) => ("downloaded", None),
            };
            let partial = if state == "not_downloaded" { crate::transcriber::download::partial_fraction(m, &store.own_dir(m)) } else { None };
            let cancelled = state == "not_downloaded" && crate::transcriber::download::was_cancelled(&store.own_dir(m));
            LlmRow { id: m.id, choice, name: m.name, note: m.note, size_mb: m.size().div_ceil(1_000_000), state, progress, partial, error: t.download_error(m.id), cancelled }
        })
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyTest {
    ok: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

/// Saves a key in the Keychain after checking it works (a model list request, no tokens spent).
#[tauri::command]
pub fn keys_save(provider: String, key: String, base_url: Option<String>) -> CmdResult<KeyTest> {
    let p = Provider::parse(&provider).ok_or_else(|| refused(format!("Unknown provider {provider}")))?;
    let base = match p {
        Provider::Compatible => compatible_base(base_url.as_deref().unwrap_or("")),
        p => default_base(p).to_string(),
    };
    match list_models(p, &base, key.trim()) {
        Ok(ids) => {
            keys::set(p.key_id(), &key).map_err(refused)?;
            let model = recommended_model(p, &ids);
            let message = match &model {
                Some(m) => tr_with("ai.connected_model", &[("model", m)]),
                None => tr("ai.connected"),
            };
            Ok(KeyTest { ok: true, message, model })
        }
        Err(EngineError::Unavailable(m)) | Err(EngineError::Failed(m)) => Ok(KeyTest { ok: false, message: m, model: None }),
        Err(EngineError::Cancelled) => Ok(KeyTest { ok: false, message: tr("error.cancelled"), model: None }),
    }
}

#[tauri::command]
pub fn keys_clear(provider: String) -> CmdResult<()> {
    let p = Provider::parse(&provider).ok_or_else(|| refused(format!("Unknown provider {provider}")))?;
    keys::clear(p.key_id()).map_err(refused)
}

/// Privacy › Saved API keys: removes every summary provider's key (the Notion secret has its own Disconnect).
#[tauri::command]
pub fn keys_clear_all() -> CmdResult<()> {
    for p in Provider::ALL {
        keys::clear(p.key_id()).map_err(refused)?;
    }
    Ok(())
}

/// Models offered by a local server (Ollama, LM Studio) at an address.
#[tauri::command]
pub fn local_server_models(address: String) -> CmdResult<Vec<String>> {
    list_models(Provider::Compatible, &compatible_base(&address), "").map_err(|e| match e {
        EngineError::Unavailable(m) | EngineError::Failed(m) => refused(m),
        EngineError::Cancelled => refused(tr("error.cancelled")),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_a_real_gpu_fault_counts_against_the_gpu_after_a_follow_up() {
        assert!(super::gpu_fault("the GPU helper stopped answering"));
        assert!(super::gpu_fault("ErrorDeviceLost"));
        assert!(!super::gpu_fault("not enough memory for the AI model: ErrorOutOfDeviceMemory"));
    }
}
