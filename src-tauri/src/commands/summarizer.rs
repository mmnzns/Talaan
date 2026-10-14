//! Summaries: the worker, Generate summary, local AI models, API keys and local servers for Settings.

use super::{notify, AppState, CmdResult, MEETINGS_CHANGED};
use crate::keys;
use crate::storage::model::MeetingStatus;
use crate::storage::StorageError;
use crate::summarizer::engines::{compatible_base, default_base, list_models, recommended_model, HttpEngine, LocalEngine, Provider, CLOUD_CONTEXT, LOCAL_CONTEXT};
use crate::summarizer::local::{helper_path, LocalAi};
use crate::summarizer::models::{local_model, LLM_CATALOG};
use crate::summarizer::summarize::{EngineError, SummaryEngine};
use crate::summarizer::worker::{Config, Deps, Worker, NO_ENGINE};
use crate::transcriber::models::{Location, ModelStore};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

pub const SUMMARY_READY: &str = "summary-ready";

pub struct SummarizerState {
    worker: Worker,
}

impl SummarizerState {
    pub fn enqueue(&self, id: &str) {
        self.worker.enqueue(id);
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
    let s = &cfg.settings;
    match cfg.engine.as_str() {
        "ollama" => {
            let base = compatible_base(s["ollamaAddress"].as_str().unwrap_or("http://localhost:11434"));
            let model = match s["ollamaModel"].as_str().filter(|m| !m.is_empty()) {
                Some(m) => m.to_string(),
                None => list_models(Provider::Compatible, &base, "")?
                    .into_iter()
                    .find(|m| !m.contains("embed"))
                    .ok_or_else(|| EngineError::Unavailable("Ollama or LM Studio has no models yet. Download one there first.".into()))?,
            };
            Ok(Box::new(HttpEngine { provider: Provider::Compatible, base, key: None, label: format!("Local server · {model}"), model, context: LOCAL_CONTEXT }))
        }
        "byok" => {
            let provider = Provider::parse(s["byokProvider"].as_str().unwrap_or("claude")).unwrap_or(Provider::Claude);
            let key = keys::get(provider.key_id());
            if key.is_none() && provider != Provider::Compatible {
                return Err(EngineError::Unavailable(format!("Add your {} API key in Settings › Summary AI.", provider.name())));
            }
            let key = key.unwrap_or_default();
            let base = match provider {
                Provider::Compatible => compatible_base(s["byokBaseUrl"].as_str().unwrap_or("")),
                p => default_base(p).to_string(),
            };
            let model = match s["byokModel"].as_str().filter(|m| !m.is_empty() && *m != "recommended") {
                Some(m) => m.to_string(),
                None => recommended_model(provider, &list_models(provider, &base, &key)?)
                    .ok_or_else(|| EngineError::Unavailable(format!("Couldn't find a suitable {} model for this key. Pick one in Settings › Summary AI.", provider.name())))?,
            };
            let context = if provider == Provider::Compatible { LOCAL_CONTEXT } else { CLOUD_CONTEXT };
            Ok(Box::new(HttpEngine { provider, base, key: Some(key).filter(|k| !k.is_empty()), label: format!("{} · {model}", provider.name()), model, context }))
        }
        _ => {
            let model = local_model(s["localModel"].as_str().unwrap_or("recommended"));
            let Location::Own(dir) = ModelStore::new(data_dir, false).locate(model) else {
                return Err(EngineError::Unavailable(format!(
                    "Download the local AI model ({}, {:.1} GB) in Settings › Summary AI.",
                    model.name,
                    model.size() as f64 / 1e9
                )));
            };
            Ok(Box::new(LocalEngine { ai: LocalAi::new(&helper_path()), model_path: dir.join(model.files[0].name), model_name: model.name.into() }))
        }
    }
}

pub fn setup(app: &AppHandle, data_dir: PathBuf) {
    let state = app.state::<AppState>();
    let changed_app = app.clone();
    let ready_app = app.clone();
    let worker = Worker::start(Deps {
        storage: state.shared(),
        changed: Arc::new(move || notify(&changed_app, MEETINGS_CHANGED)),
        make_engine: Box::new(move |cfg| make_engine(cfg, &data_dir)),
        summarized: Arc::new(move |id, title| {
            let _ = ready_app.emit(SUMMARY_READY, serde_json::json!({ "id": id, "title": title }));
            crate::commands::summarizer::notify_ready(&ready_app, id, title);
        }),
    });
    for id in state.lock().ids_with_status(MeetingStatus::Summarizing).unwrap_or_default() {
        worker.enqueue(&id);
    }
    app.manage(SummarizerState { worker });
}

/// The "Summary ready" notification (Task 8 makes it native with click-to-open).
pub fn notify_ready(_app: &AppHandle, _id: &str, _title: &str) {}

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
    let storage = state.lock();
    let meeting = storage.get_meeting(&id)?;
    if meeting.transcript.is_empty() {
        return Err(refused("There's no transcript to summarize yet."));
    }
    if matches!(meeting.item.status, MeetingStatus::Recording | MeetingStatus::Transcribing) {
        return Err(refused("Wait for the transcript to finish first."));
    }
    storage.update_meeting_file(&id, |f| {
        f.status = MeetingStatus::Summarizing;
        f.progress = Some(0.0);
        f.summary_error = None;
    })?;
    drop(storage);
    s.enqueue(&id);
    notify(&app, MEETINGS_CHANGED);
    Ok(())
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
            LlmRow { id: m.id, choice, name: m.name, note: m.note, size_mb: m.size().div_ceil(1_000_000), state, progress }
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
                Some(m) => format!("Connected. Summaries will use {m}."),
                None => "Connected.".into(),
            };
            Ok(KeyTest { ok: true, message, model })
        }
        Err(EngineError::Unavailable(m)) | Err(EngineError::Failed(m)) => Ok(KeyTest { ok: false, message: m, model: None }),
        Err(EngineError::Cancelled) => Ok(KeyTest { ok: false, message: "Cancelled.".into(), model: None }),
    }
}

#[tauri::command]
pub fn keys_clear(provider: String) -> CmdResult<()> {
    let p = Provider::parse(&provider).ok_or_else(|| refused(format!("Unknown provider {provider}")))?;
    keys::clear(p.key_id()).map_err(refused)
}

/// Models offered by a local server (Ollama, LM Studio) at an address.
#[tauri::command]
pub fn local_server_models(address: String) -> CmdResult<Vec<String>> {
    list_models(Provider::Compatible, &compatible_base(&address), "").map_err(|e| match e {
        EngineError::Unavailable(m) | EngineError::Failed(m) => refused(m),
        EngineError::Cancelled => refused("Cancelled."),
    })
}
