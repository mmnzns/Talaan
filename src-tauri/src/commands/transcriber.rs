//! Transcription and speech models: the worker, the model list for Settings, downloads with progress.

use super::{notify, AppState, CmdResult, MEETINGS_CHANGED};
use crate::i18n::{tr, tr_with};
use crate::models::{Models, DIARIZATION, SEARCH, VAD};
use crate::storage::model::MeetingStatus;
use crate::storage::StorageError;
use crate::transcriber::models::speech;
use crate::transcriber::pipeline::Detector;
use crate::transcriber::vad::Vad;
use crate::transcriber::worker::{restamp_queue, stamp_estimate, Deps, OnGpu, Worker, NO_MODEL};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use voice_engine_models::{catalog, App, DownloadError, Kind, ModelInfo, Released, Status, StoreError};

pub const MODELS_CHANGED: &str = "models-changed";

pub struct TranscriberState {
    pub worker: Worker,
    data_dir: PathBuf,
    on_gpu: Arc<OnGpu>,
    /// Model id → the flag its Cancel sets, while Talaan is downloading it. Progress comes from the engine's status.
    downloads: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Model id → why its last download failed (shown until the next try).
    failed: Mutex<HashMap<String, String>>,
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
    /// Why each model's last download failed, until its next try.
    pub fn download_errors(&self) -> HashMap<String, String> {
        self.failed.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// A downloadable model: speech, speaker separation, search or local AI. Not the voice detection file, which
/// comes with every speech model.
fn find_any(id: &str) -> Option<&'static ModelInfo> {
    catalog::find(id).filter(|m| m.kind != Kind::Vad)
}

fn err(message: String) -> StorageError {
    StorageError::Refused(message)
}

/// Whether a speech model runs on the graphics chip: Whisper with "use the GPU" on, through Metal on the Mac and
/// the Vulkan helper on Windows when `gpu.rs` says it should be used. Parakeet and Nemotron use the processor.
fn stt_on_gpu(model: &ModelInfo, use_gpu: bool, gpu: &crate::gpu::Gpu) -> bool {
    if model.kind != Kind::Whisper || !use_gpu {
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
        models: app.state::<Arc<Models>>().inner().clone(),
        changed: Arc::new(move || notify(&changed_app, MEETINGS_CHANGED)),
        // Whisper on the GPU helper on Windows when it should be used, the processor behind it (fallback.rs).
        load_engine: {
            let gpu = crate::gpu::Gpu::shared(&data_dir);
            Box::new(move |m, dir, use_gpu| crate::transcriber::fallback::load(m, dir, use_gpu, &gpu))
        },
        load_detector: Box::new(|path| Vad::load(path).map(|v| Box::new(v) as Box<dyn Detector + Send>)),
        load_splitter: Box::new(|path| crate::transcriber::diarize::Diarizer::load(path).map(|d| Box::new(d) as Box<dyn crate::transcriber::diarize::SpeakerSplitter>)),
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
    let waiting = storage.ids_with_status(MeetingStatus::Transcribing).unwrap_or_default();
    // Fresh estimates, chained in the order they're queued, so a Stop made before the worker reaches them counts them.
    restamp_queue(&storage, &waiting, on_gpu.as_ref());
    for id in &waiting {
        worker.enqueue(id);
    }
    drop(storage);
    app.manage(TranscriberState { worker, data_dir, on_gpu, downloads: Mutex::default(), failed: Mutex::default() });
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
pub fn meetings_retranscribe(app: AppHandle, state: State<AppState>, t: State<TranscriberState>, models: State<Arc<Models>>, id: String, model: String) -> CmdResult<()> {
    let m = catalog::find(&model).filter(|m| m.is_speech()).ok_or_else(|| err(format!("No speech model called {model}")))?;
    let storage = state.lock();
    if models.ready_dir(m.id).is_none() {
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
    /// whisper | parakeet | diarization | search
    engine: &'static str,
    size_mb: u64,
    speed: u8,
    accuracy: u8,
    languages: &'static str,
    /// SPDX id ("MIT", "CC-BY-4.0").
    licence: &'static str,
    english_only: bool,
    /// not_downloaded | downloading | downloaded. A model another app downloaded shows as downloaded.
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<f32>,
    /// How much an interrupted download left on disk (0..1), while not downloading. Download resumes from it.
    #[serde(skip_serializing_if = "Option::is_none")]
    partial: Option<f32>,
    /// Why the last download failed, until the next try.
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// The partial download was stopped with Cancel (not by a quit or a crash): it waits for a click.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    cancelled: bool,
}

/// What a model's row shows for the engine's status: (state, progress, partial, cancelled).
pub fn state(status: &Status) -> (&'static str, Option<f32>, Option<f32>, bool) {
    match status {
        Status::Installed { .. } => ("downloaded", None, None, false),
        Status::Downloading { fraction, .. } => ("downloading", Some(*fraction), None, false),
        Status::Partial { fraction, cancelled, .. } => ("not_downloaded", None, Some(*fraction), *cancelled),
        Status::Missing => ("not_downloaded", None, None, false),
    }
}

/// The speech models in Talaan's order, then speaker separation and search, with what is on disk right now.
pub fn model_rows(models: &Models, failed: &HashMap<String, String>) -> Vec<ModelRow> {
    let extras = [DIARIZATION, SEARCH].into_iter().filter_map(catalog::find);
    speech()
        .into_iter()
        .chain(extras)
        .map(|m| {
            let (state, progress, partial, cancelled) = state(&models.status(m.id));
            ModelRow {
                id: m.id,
                name: m.name,
                maker: m.maker,
                note: m.note,
                engine: match m.kind {
                    Kind::Parakeet | Kind::ParakeetUnified | Kind::Nemotron => "parakeet",
                    Kind::Whisper => "whisper",
                    Kind::Diarization => "diarization",
                    Kind::Embedding => "search",
                    Kind::Vad => "vad",
                    Kind::Llm => "llm",
                },
                size_mb: m.size().div_ceil(1_000_000),
                speed: m.speed,
                accuracy: m.accuracy,
                languages: m.languages,
                licence: m.licence.spdx,
                english_only: m.english_only,
                state,
                progress,
                partial,
                cancelled,
                error: failed.get(m.id).cloned(),
            }
        })
        .collect()
}

/// The catalog with what is on disk right now.
#[tauri::command]
pub fn models_list(models: State<Arc<Models>>, t: State<TranscriberState>) -> CmdResult<Vec<ModelRow>> {
    Ok(model_rows(&models, &t.download_errors()))
}

/// A model download failed (not cancelled): `{ id, message }`. The reason also stays in models_list until the
/// next try, so a screen opened later still shows it.
pub const DOWNLOAD_FAILED: &str = "model-download-failed";

#[derive(Serialize, Clone)]
struct DownloadFailed {
    id: String,
    message: String,
}

#[derive(Serialize, Clone)]
struct DownloadProgress {
    id: String,
    progress: f32,
}

/// What to tell the person about a failed download; None when they cancelled it.
fn download_message(name: &str, e: &StoreError) -> Option<String> {
    match e {
        StoreError::Download(DownloadError::Cancelled) => None,
        StoreError::Download(DownloadError::Stalled) => Some(tr_with("download.stalled", &[("name", name)])),
        StoreError::Download(DownloadError::Offline) | StoreError::Download(DownloadError::Refused { .. }) => {
            Some(tr_with("download.network", &[("name", name), ("error", &tr("download.offline"))]))
        }
        StoreError::Download(DownloadError::Checksum { .. }) => Some(tr_with("download.corrupt", &[("name", name)])),
        StoreError::Download(DownloadError::DiskFull) => Some(tr("download.disk_full")),
        StoreError::DownloadingElsewhere => Some(tr_with("download.elsewhere", &[("name", name)])),
        other => Some(tr_with("download.save_failed", &[("error", &crate::logging::text(other.to_string()).to_string())])),
    }
}

/// Downloads a model (and the voice detection file if it's missing) in the background.
#[tauri::command]
pub fn models_download(app: AppHandle, t: State<TranscriberState>, models: State<Arc<Models>>, id: String) -> CmdResult<()> {
    let model = find_any(&id).ok_or_else(|| err(format!("No model called {id}")))?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut d = t.downloads.lock().unwrap_or_else(|e| e.into_inner());
        if d.contains_key(&id) {
            return Ok(());
        }
        d.insert(id.clone(), cancel.clone());
    }
    t.failed.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
    let _ = app.emit(MODELS_CHANGED, ());
    let models = models.inner().clone();
    std::thread::spawn(move || {
        log::info!("downloading model {id}");
        let started = Instant::now();
        let mut last = Instant::now();
        let mut report = |p: voice_engine_models::Progress| {
            if last.elapsed() >= Duration::from_millis(250) || p.fraction >= 1.0 {
                last = Instant::now();
                let _ = app.emit("model-download", DownloadProgress { id: id.clone(), progress: p.fraction });
            }
        };
        // Speech models need the voice detector too; the add-on models don't.
        let speech = model.is_speech();
        let mut result = Ok(());
        if speech && models.ready_dir(VAD).is_none() {
            result = models.store().download(VAD, &cancel, &mut |_| {}).map(|_| ());
        }
        if result.is_ok() {
            result = models.store().download(&id, &cancel, &mut report).map(|_| ());
        }
        let t = app.state::<TranscriberState>();
        t.downloads.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        let message = result.as_ref().err().and_then(|e| download_message(model.name, e));
        if let Some(m) = &message {
            t.failed.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), m.clone());
        }
        let _ = app.emit(MODELS_CHANGED, ());
        match &result {
            Ok(()) => log::info!("model {id} downloaded in {:.0}s", started.elapsed().as_secs_f32()),
            Err(StoreError::Download(DownloadError::Cancelled)) => log::info!("model {id} download cancelled"),
            Err(e) => log::warn!("model {id} download failed: {}", crate::logging::text(e)),
        }
        match result {
            Ok(()) if model.kind == Kind::Llm => super::summarizer::requeue_waiting(&app),
            Ok(()) if model.kind == Kind::Embedding => super::ask::reindex_soon(&app),
            Ok(()) if speech => requeue_waiting_for_model(&app),
            _ => {}
        }
        if let Some(message) = message {
            let _ = app.emit(DOWNLOAD_FAILED, DownloadFailed { id: id.clone(), message });
        }
    });
    Ok(())
}

/// Stops Talaan's download of a model. The engine keeps what arrived and marks it cancelled, so Download resumes.
#[tauri::command]
pub fn models_cancel(app: AppHandle, t: State<TranscriberState>, id: String) -> CmdResult<()> {
    if let Some(flag) = t.downloads.lock().unwrap_or_else(|e| e.into_inner()).get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
    let _ = app.emit(MODELS_CHANGED, ());
    Ok(())
}

/// What Delete did, for the toast: `erased` when the file is gone (or goes once nothing has it open), otherwise
/// the other apps that still use it ("Ensayo", "Parlato").
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Deleted {
    pub erased: bool,
    pub other_apps: Vec<&'static str>,
}

fn app_name(app: &App) -> Option<&'static str> {
    match app {
        App::Talaan => None,
        App::Ensayo => Some("Ensayo"),
        App::Parlato => Some("Parlato"),
    }
}

/// Talaan stops using a model. The engine erases the file once no app uses it; another app's copy stays.
/// `in_use`: whether a meeting is being transcribed with that model right now.
pub fn delete_model(models: &Models, in_use: impl Fn(&str) -> bool, id: &str) -> CmdResult<Deleted> {
    let model = find_any(id).ok_or_else(|| err(format!("No model called {id}")))?;
    if in_use(model.id) {
        return Err(err(tr("error.model_in_use")));
    }
    let released = models.store().release(model.id).map_err(|e| err(crate::i18n::tr_err("error.delete_model", e)))?;
    Ok(match released {
        Released::Erased | Released::NotUsing => Deleted { erased: true, other_apps: vec![] },
        // No other app listed: another app has the file open now, and the engine erases it when it lets go.
        Released::Kept { users } => {
            let other_apps: Vec<_> = users.iter().filter_map(app_name).collect();
            Deleted { erased: other_apps.is_empty(), other_apps }
        }
    })
}

/// Talaan stops using a model (Delete). Refused while a meeting is being transcribed with it.
#[tauri::command]
pub fn models_delete(app: AppHandle, t: State<TranscriberState>, models: State<Arc<Models>>, id: String) -> CmdResult<Deleted> {
    let deleted = delete_model(&models, |m| t.worker.is_using(m), &id)?;
    let _ = app.emit(MODELS_CHANGED, ());
    Ok(deleted)
}

/// A model's files at the catalog's exact sizes (sparse), so the engine counts it as installed.
#[cfg(test)]
pub(super) fn install_for_test(models: &Models, id: &str) {
    for f in catalog::find(id).unwrap().files {
        let path = models.root().join(id).join(f.name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::File::create(path).unwrap().set_len(f.size).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn sparse(path: &Path, size: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::File::create(path).unwrap().set_len(size).unwrap();
    }

    fn install(models: &Models, id: &str) {
        install_for_test(models, id);
    }

    fn temp() -> (tempfile::TempDir, Models) {
        let data = tempfile::tempdir().unwrap();
        let models = Models::open(Some(data.path())).unwrap();
        (data, models)
    }

    fn row<'a>(rows: &'a [ModelRow], id: &str) -> &'a ModelRow {
        rows.iter().find(|r| r.id == id).unwrap()
    }

    #[test]
    fn the_list_shows_what_is_on_disk() {
        let (_d, models) = temp();
        install(&models, "whisper-small");
        // An interrupted download: half of the file, still named .part.
        let base = catalog::find("whisper-base").unwrap();
        sparse(&models.root().join("whisper-base").join(format!("{}.part", base.files[0].name)), base.files[0].size / 2);
        let rows = model_rows(&models, &HashMap::new());

        let done = row(&rows, "whisper-small");
        assert_eq!((done.state, done.progress, done.partial, done.cancelled), ("downloaded", None, None, false));
        let part = row(&rows, "whisper-base");
        assert_eq!((part.state, part.progress, part.cancelled), ("not_downloaded", None, false));
        assert!(part.partial.is_some_and(|f| (0.4..0.6).contains(&f)), "{:?}", part.partial);
        let none = row(&rows, "whisper-tiny");
        assert_eq!((none.state, none.progress, none.partial), ("not_downloaded", None, None));
    }

    #[test]
    fn the_list_is_the_speech_catalog_in_talaans_order_then_the_add_ons() {
        let (_d, models) = temp();
        let rows = model_rows(&models, &HashMap::new());
        let ids: Vec<_> = rows.iter().map(|r| r.id).collect();
        assert_eq!(&ids[..3], ["whisper-turbo", "parakeet-v3", "whisper-small"]);
        assert_eq!(&ids[ids.len() - 2..], ["diarization", "search"]);
        assert_eq!(ids.len(), catalog::speech().count() + 2);
        assert!(!ids.contains(&"vad"));
        let small_en = row(&rows, "whisper-small-en");
        assert!(small_en.english_only && !row(&rows, "whisper-small").english_only);
        assert_eq!((row(&rows, "whisper-turbo").engine, row(&rows, "parakeet-v3").engine, row(&rows, "diarization").engine, row(&rows, "search").engine), ("whisper", "parakeet", "diarization", "search"));
        assert_eq!(row(&rows, "whisper-turbo").licence, catalog::find("whisper-turbo").unwrap().licence.spdx);
    }

    #[test]
    fn the_rows_say_why_the_last_download_failed() {
        let (_d, models) = temp();
        let failed = HashMap::from([("whisper-tiny".to_string(), "no internet".to_string())]);
        let rows = model_rows(&models, &failed);
        assert_eq!(row(&rows, "whisper-tiny").error.as_deref(), Some("no internet"));
        assert_eq!(row(&rows, "whisper-base").error, None);
    }

    #[test]
    fn the_serialized_row_keeps_the_names_the_screen_reads() {
        let (_d, models) = temp();
        let v = serde_json::to_value(model_rows(&models, &HashMap::new())).unwrap();
        let first = v[0].as_object().unwrap();
        for key in ["id", "name", "maker", "note", "engine", "sizeMb", "speed", "accuracy", "languages", "licence", "englishOnly", "state"] {
            assert!(first.contains_key(key), "{key}");
        }
        for gone in ["progress", "partial", "error", "cancelled", "onThisMac"] {
            assert!(!first.contains_key(gone), "{gone}");
        }
    }

    #[test]
    fn deleting_a_model_only_talaan_uses_erases_it() {
        let (_d, models) = temp();
        install(&models, "whisper-small");
        models.store().use_model("whisper-small").unwrap();
        let rows = model_rows(&models, &HashMap::new());
        assert_eq!(row(&rows, "whisper-small").state, "downloaded");
        assert_eq!(delete_model(&models, |_| false, "whisper-small").unwrap(), Deleted { erased: true, other_apps: vec![] });
        assert!(!models.root().join("whisper-small").exists());
        assert_eq!(row(&model_rows(&models, &HashMap::new()), "whisper-small").state, "not_downloaded");
        // Again: nothing to do, still fine.
        assert_eq!(delete_model(&models, |_| false, "whisper-small").unwrap().erased, true);
    }

    #[test]
    fn deleting_a_model_another_app_uses_keeps_the_file_and_names_the_app() {
        let (_d, models) = temp();
        install(&models, "whisper-small");
        models.store().use_model("whisper-small").unwrap();
        let users = models.root().join(".engine/users/whisper-small");
        std::fs::write(users.join("ensayo"), b"").unwrap();
        std::fs::write(users.join("parlato"), b"").unwrap();
        let d = delete_model(&models, |_| false, "whisper-small").unwrap();
        assert_eq!(d, Deleted { erased: false, other_apps: vec!["Ensayo", "Parlato"] });
        assert_eq!(models.status("whisper-small"), Status::Installed { dir: models.root().join("whisper-small"), users: vec![App::Ensayo, App::Parlato], used_by_me: false });
        assert_eq!(serde_json::to_value(&d).unwrap(), serde_json::json!({"erased": false, "otherApps": ["Ensayo", "Parlato"]}));
    }

    #[test]
    fn a_model_in_use_by_a_meeting_is_not_deleted() {
        let (_d, models) = temp();
        install(&models, "whisper-small");
        models.store().use_model("whisper-small").unwrap();
        let e = delete_model(&models, |id| id == "whisper-small", "whisper-small").unwrap_err();
        assert!(e.to_string().contains("transcribed"), "{e}");
        assert!(models.ready_dir("whisper-small").is_some());
        // Unknown ids and the voice detection file (never offered) are refused too.
        assert!(delete_model(&models, |_| false, "nope").is_err());
        assert!(delete_model(&models, |_| false, "vad").is_err());
    }

    #[test]
    fn the_privacy_tab_counts_only_installed_models_talaan_uses() {
        let (_d, models) = temp();
        install(&models, "whisper-small");
        install(&models, "whisper-base");
        install(&models, "whisper-tiny");
        models.store().use_model("whisper-small").unwrap();
        models.store().use_model("whisper-base").unwrap();
        // whisper-tiny: installed, but only another app uses it.
        sparse(&models.root().join(".engine/users/whisper-tiny/ensayo"), 0);
        let want = catalog::find("whisper-small").unwrap().size() + catalog::find("whisper-base").unwrap().size();
        assert_eq!(super::super::settings::models_bytes(&models), want);
    }

    #[test]
    fn a_failed_download_says_what_to_do() {
        use DownloadError as D;
        let msg = |e: StoreError| download_message("Whisper Small", &e);
        assert_eq!(msg(StoreError::Download(D::Cancelled)), None);
        assert!(msg(StoreError::Download(D::Stalled)).unwrap().contains("30 seconds"));
        for e in [D::Offline, D::Refused { status: 503 }] {
            let m = msg(StoreError::Download(e)).unwrap();
            assert!(m.contains("Whisper Small") && m.contains("no internet connection"), "{m}");
        }
        assert!(msg(StoreError::Download(D::Checksum { file: "f".into() })).unwrap().contains("damaged"));
        assert!(msg(StoreError::Download(D::DiskFull)).unwrap().contains("free space"));
        assert!(msg(StoreError::DownloadingElsewhere).unwrap().contains("Another app is downloading Whisper Small"));
        let other = msg(StoreError::Io("disk".into())).unwrap();
        assert!(other.starts_with("Couldn't save the model"), "{other}");
    }

    #[test]
    fn statuses_map_to_the_states_the_screen_knows() {
        let d = PathBuf::new();
        assert_eq!(state(&Status::Installed { dir: d, users: vec![], used_by_me: false }), ("downloaded", None, None, false));
        assert_eq!(state(&Status::Downloading { fraction: 0.5, bytes: 1, by_me: true }), ("downloading", Some(0.5), None, false));
        assert_eq!(state(&Status::Partial { fraction: 0.25, bytes: 1, cancelled: true }), ("not_downloaded", None, Some(0.25), true));
        assert_eq!(state(&Status::Missing), ("not_downloaded", None, None, false));
    }
}
