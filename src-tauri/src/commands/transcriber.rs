//! Transcription and speech models: the worker, the model list for Settings, downloads with progress.

use super::{notify, AppState, CmdResult, MEETINGS_CHANGED};
use crate::storage::model::MeetingStatus;
use crate::storage::StorageError;
use crate::transcriber::download::{download_model, DownloadError};
use crate::transcriber::models::{find, EngineKind, Location, ModelInfo, ModelStore, CATALOG, VAD};
use crate::transcriber::pipeline::Detector;
use crate::transcriber::vad::Vad;
use crate::transcriber::worker::{stamp_estimate, Config, Deps, OnGpu, Worker, NO_MODEL};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

pub const MODELS_CHANGED: &str = "models-changed";

pub struct TranscriberState {
    pub worker: Worker,
    data_dir: PathBuf,
    on_gpu: Arc<OnGpu>,
    /// Model id → (cancel flag, progress 0..1).
    downloads: Mutex<HashMap<String, (Arc<AtomicBool>, f32)>>,
}

impl TranscriberState {
    pub fn enqueue(&self, id: &str) {
        self.worker.enqueue(id);
    }
    /// A first time estimate on a meeting that was just handed over (Stop says how long it will take).
    pub fn stamp_estimate(&self, storage: &crate::storage::Storage, id: &str) {
        stamp_estimate(storage, id, self.on_gpu.as_ref());
    }
    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }
    pub fn download_progress(&self, id: &str) -> Option<f32> {
        self.downloads.lock().unwrap_or_else(|e| e.into_inner()).get(id).map(|d| d.1)
    }
}

/// A downloadable model: speech (incl. the voice detection file) or local summary AI.
fn find_any(id: &str) -> Option<&'static crate::transcriber::models::ModelInfo> {
    find(id).filter(|m| m.id != VAD.id).or_else(|| crate::summarizer::models::find_llm(id))
}

fn err(message: String) -> StorageError {
    StorageError::Refused(message)
}

/// Whether a speech model runs on the graphics chip: Whisper with "use the GPU" on, through Metal on the Mac and
/// the Vulkan helper on Windows when `gpu.rs` says it should be used. Parakeet and Nemotron use the processor.
fn stt_on_gpu(model: &ModelInfo, use_gpu: bool, gpu: &crate::gpu::Gpu) -> bool {
    if model.engine != EngineKind::Whisper || !use_gpu {
        return false;
    }
    if cfg!(target_os = "macos") {
        return true;
    }
    cfg!(windows) && matches!(gpu.plan(crate::gpu::Kind::Stt, true), crate::gpu::Plan::Gpu { .. })
}

/// Starts the worker and queues every meeting left `transcribing` (a crash or quit mid-way).
pub fn setup(app: &AppHandle, data_dir: PathBuf) {
    let state = app.state::<AppState>();
    let changed_app = app.clone();
    let on_gpu: Arc<OnGpu> = {
        let gpu = crate::gpu::Gpu::shared(&data_dir);
        Arc::new(move |m: &ModelInfo, use_gpu: bool| stt_on_gpu(m, use_gpu, &gpu))
    };
    let worker = Worker::start(Deps {
        storage: state.shared(),
        data_dir: data_dir.clone(),
        changed: Arc::new(move || notify(&changed_app, MEETINGS_CHANGED)),
        // Whisper on the GPU helper on Windows when it should be used, the processor behind it (fallback.rs).
        load_engine: {
            let gpu = crate::gpu::Gpu::shared(&data_dir);
            Box::new(move |m, dir, use_gpu| crate::transcriber::fallback::load(m, dir, use_gpu, &gpu))
        },
        load_detector: Box::new(|path| Vad::load(path).map(|v| Box::new(v) as Box<dyn Detector + Send>)),
        load_splitter: Box::new(|path| crate::transcriber::diarize::Diarizer::load(path).map(|d| Box::new(d) as Box<dyn crate::transcriber::diarize::SpeakerSplitter>)),
        reuse_roots: None,
        transcribed: {
            let app = app.clone();
            Arc::new(move |id: &str| app.state::<super::summarizer::SummarizerState>().enqueue_auto(id))
        },
        transcript_ready: {
            let app = app.clone();
            Arc::new(move |id: &str, title: &str| super::summarizer::notify_done(&app, id, "notify.transcript_ready", title))
        },
        on_gpu: on_gpu.clone(),
    });
    let storage = state.lock();
    for id in storage.ids_with_status(MeetingStatus::Transcribing).unwrap_or_default() {
        // An estimate from before the restart means nothing now (and would push back the next Stop's estimate).
        let _ = storage.update_meeting_file(&id, |f| f.eta = None);
        worker.enqueue(&id);
    }
    drop(storage);
    app.manage(TranscriberState { worker, data_dir, on_gpu, downloads: Mutex::default() });
}

/// Queues meetings that failed only because no model was there.
fn requeue_waiting_for_model(app: &AppHandle) {
    let state = app.state::<AppState>();
    let t = app.state::<TranscriberState>();
    let storage = state.lock();
    let ids = storage.ids_with_status(MeetingStatus::Failed).unwrap_or_default();
    for id in ids {
        let waiting = storage.read_meeting_file(&id).ok().and_then(|f| f.error).is_some_and(|e| e.code == NO_MODEL);
        if waiting && storage.update_meeting_file(&id, |f| {
            f.status = MeetingStatus::Transcribing;
            f.progress = Some(0.0);
            f.error = None;
        })
        .is_ok()
        {
            t.enqueue(&id);
        }
    }
    drop(storage);
    notify(app, MEETINGS_CHANGED);
}

/// Try again on a failed meeting: back into the queue.
#[tauri::command]
pub fn meetings_retry(app: AppHandle, state: State<AppState>, t: State<TranscriberState>, id: String) -> CmdResult<()> {
    let storage = state.lock();
    let dir = storage.dir(&id)?;
    if !dir.join("me.wav").exists() && !dir.join("them.wav").exists() {
        return Err(err(crate::i18n::tr("error.audio_gone")));
    }
    storage.update_meeting_file(&id, |f| {
        f.status = MeetingStatus::Transcribing;
        f.progress = Some(0.0);
        f.error = None;
    })?;
    drop(storage);
    t.enqueue(&id);
    notify(&app, MEETINGS_CHANGED);
    Ok(())
}

/// Transcribe a finished meeting again with another speech model (this meeting only). Edits and the summary
/// are dropped, speaker names stay, and the new transcript is summarized like a new call.
#[tauri::command]
pub fn meetings_retranscribe(app: AppHandle, state: State<AppState>, t: State<TranscriberState>, id: String, model: String) -> CmdResult<()> {
    let m = find(&model).filter(|m| m.engine.is_speech()).ok_or_else(|| err(format!("No speech model called {model}")))?;
    let storage = state.lock();
    let config = Config::from_settings(storage.settings_get()?.as_ref());
    if ModelStore::new(&t.data_dir, config.reuse).locate(m) == Location::Missing {
        return Err(err(crate::i18n::tr_with("error.download_speech_model", &[("name", m.name)])));
    }
    app.state::<super::summarizer::SummarizerState>().cancel_if_running(&id);
    storage.start_retranscribe(&id, m.id)?;
    drop(storage);
    t.enqueue(&id);
    notify(&app, MEETINGS_CHANGED);
    Ok(())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelRow {
    id: &'static str,
    name: &'static str,
    maker: &'static str,
    note: &'static str,
    engine: &'static str,
    size_mb: u64,
    speed: u8,
    accuracy: u8,
    languages: &'static str,
    /// not_downloaded | downloading | downloaded | on_this_mac
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<f32>,
}

/// The catalog with what is on disk right now.
#[tauri::command]
pub fn models_list(state: State<AppState>, t: State<TranscriberState>) -> CmdResult<Vec<ModelRow>> {
    let config = Config::from_settings(state.lock().settings_get()?.as_ref());
    let store = ModelStore::new(&t.data_dir, config.reuse);
    let downloads = t.downloads.lock().unwrap_or_else(|e| e.into_inner());
    Ok(CATALOG
        .iter()
        .chain([&crate::transcriber::models::DIARIZATION, &crate::transcriber::models::SEARCH])
        .map(|m| {
            let (state, progress) = match (downloads.get(m.id), store.locate(m)) {
                (Some((_, p)), _) => ("downloading", Some(*p)),
                (None, Location::Own(_)) => ("downloaded", None),
                (None, Location::OnThisMac(_)) => ("on_this_mac", None),
                (None, Location::Missing) => ("not_downloaded", None),
            };
            ModelRow {
                id: m.id,
                name: m.name,
                maker: m.maker,
                note: m.note,
                engine: match m.engine {
                    crate::transcriber::models::EngineKind::Parakeet | crate::transcriber::models::EngineKind::ParakeetUnified | crate::transcriber::models::EngineKind::Nemotron => "parakeet",
                    crate::transcriber::models::EngineKind::Whisper => "whisper",
                    crate::transcriber::models::EngineKind::Llm => "llm",
                    crate::transcriber::models::EngineKind::Diarization => "diarization",
                    crate::transcriber::models::EngineKind::Embedding => "search",
                },
                size_mb: m.size().div_ceil(1_000_000),
                speed: m.speed,
                accuracy: m.accuracy,
                languages: m.languages,
                state,
                progress,
            }
        })
        .collect())
}

#[derive(Serialize, Clone)]
struct DownloadProgress {
    id: String,
    progress: f32,
}

/// Downloads a model (and the voice detection file if it's missing) in the background.
#[tauri::command]
pub fn models_download(app: AppHandle, t: State<TranscriberState>, id: String) -> CmdResult<()> {
    let model = find_any(&id).ok_or_else(|| err(format!("No model called {id}")))?;
    let is_llm = model.engine == crate::transcriber::models::EngineKind::Llm;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut d = t.downloads.lock().unwrap_or_else(|e| e.into_inner());
        if d.contains_key(&id) {
            return Ok(());
        }
        d.insert(id.clone(), (cancel.clone(), 0.0));
    }
    let _ = app.emit(MODELS_CHANGED, ());
    let data_dir = t.data_dir.clone();
    std::thread::spawn(move || {
        log::info!("downloading model {id}");
        let started = Instant::now();
        let store = ModelStore::new(&data_dir, false);
        let mut last = Instant::now();
        let mut report = |p: f32| {
            if last.elapsed() >= Duration::from_millis(250) || p >= 1.0 {
                last = Instant::now();
                if let Some(e) = app.state::<TranscriberState>().downloads.lock().unwrap_or_else(|e| e.into_inner()).get_mut(&id) {
                    e.1 = p;
                }
                let _ = app.emit("model-download", DownloadProgress { id: id.clone(), progress: p });
            }
        };
        // Speech models need the voice detector too; the add-on models don't.
        let speech = model.engine.is_speech();
        let vad_missing = speech && store.locate(&VAD) == Location::Missing && ModelStore::new(&data_dir, true).locate(&VAD) == Location::Missing;
        let mut result = Ok(());
        if vad_missing {
            result = download_model(&VAD, &store.own_dir(&VAD), &cancel, &mut |_| {});
        }
        if result.is_ok() {
            result = download_model(model, &store.own_dir(model), &cancel, &mut report);
        }
        app.state::<TranscriberState>().downloads.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        let _ = app.emit(MODELS_CHANGED, ());
        match &result {
            Ok(()) => log::info!("model {id} downloaded in {:.0}s", started.elapsed().as_secs_f32()),
            Err(DownloadError::Cancelled) => log::info!("model {id} download cancelled"),
            Err(e) => log::warn!("model {id} download failed: {}", crate::logging::text(e)),
        }
        match result {
            Ok(()) if is_llm => super::summarizer::requeue_waiting(&app),
            Ok(()) if model.engine == EngineKind::Embedding => super::ask::reindex_soon(&app),
            Ok(()) if speech => requeue_waiting_for_model(&app),
            Ok(()) => {}
            Err(DownloadError::Cancelled) => {}
            Err(e) => {
                let _ = app.emit("recorder-notice", serde_json::json!({ "text": e.user_message(), "kind": "warning" }));
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn models_cancel(app: AppHandle, t: State<TranscriberState>, id: String) -> CmdResult<()> {
    if let Some((flag, _)) = t.downloads.lock().unwrap_or_else(|e| e.into_inner()).get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
    let _ = app.emit(MODELS_CHANGED, ());
    Ok(())
}

/// Deletes Talaan's own copy of a model (never another app's).
#[tauri::command]
pub fn models_delete(app: AppHandle, t: State<TranscriberState>, id: String) -> CmdResult<()> {
    let model = find_any(&id).ok_or_else(|| err(format!("No model called {id}")))?;
    let dir = ModelStore::new(&t.data_dir, false).own_dir(model);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| err(crate::i18n::tr_err("error.delete_model", e)))?;
    }
    let _ = app.emit(MODELS_CHANGED, ());
    Ok(())
}
