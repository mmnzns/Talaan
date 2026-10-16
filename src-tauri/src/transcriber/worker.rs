//! The background transcriber: one thread, one meeting at a time, queue rebuilt from disk at launch.
//! A meeting enters as `transcribing` and leaves as `ready` (M5: `summarizing`) or `failed` with a reason.

use super::engine::Engine;
use super::diarize::SpeakerSplitter;
use super::models::{find, ModelInfo, ModelStore, DEFAULT_MODEL, DIARIZATION, VAD};
use super::pipeline::{run, Detector, PipelineError};
use crate::storage::model::{MeetingStatus, ProcessingError, TranscriptionInfo};
use crate::storage::Storage;
use serde_json::Value;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub const NO_MODEL: &str = "no_model";
/// `summary_error` code for a meeting that waits for Generate summary (transcript only, or automatic
/// summaries turned off), so a sensitive call never goes to a summary engine by itself.
pub const WAITING: &str = "waiting";
const IDLE_UNLOAD: Duration = Duration::from_secs(300);

/// What the worker reads from `settings.json` for each meeting.
#[derive(Debug, Clone)]
pub struct Config {
    pub model: &'static ModelInfo,
    /// None = detect.
    pub language: Option<String>,
    pub use_gpu: bool,
    pub reuse: bool,
    /// Split the Them track into Speaker 1, 2, 3 (when the model is downloaded).
    pub separate_speakers: bool,
    /// Hand each new transcript to the summarizer (Settings › Summary AI › Summarize automatically).
    pub summarize: bool,
}

impl Config {
    pub fn from_settings(v: Option<&Value>) -> Config {
        let v = v.cloned().unwrap_or(Value::Null);
        let model = v["activeSpeechModel"].as_str().and_then(find).filter(|m| m.id != VAD.id).unwrap_or_else(|| find(DEFAULT_MODEL).unwrap());
        let language = v["meetingLanguage"].as_str().filter(|l| !l.is_empty() && *l != "auto").map(str::to_string);
        Config {
            model,
            language,
            use_gpu: v["useGpu"].as_bool().unwrap_or(true),
            reuse: v["reuseModels"].as_bool().unwrap_or(true),
            separate_speakers: v["separateSpeakers"].as_bool().unwrap_or(true),
            summarize: v["summarizeAutomatically"].as_bool().unwrap_or(true),
        }
    }
}

pub type EngineLoader = dyn Fn(&ModelInfo, &Path, bool) -> Result<Box<dyn Engine>, String> + Send + Sync;
pub type DetectorLoader = dyn Fn(&Path) -> Result<Box<dyn Detector + Send>, String> + Send + Sync;
pub type SplitterLoader = dyn Fn(&Path) -> Result<Box<dyn SpeakerSplitter>, String> + Send + Sync;

pub struct Deps {
    pub storage: Arc<Mutex<Storage>>,
    pub data_dir: PathBuf,
    /// Called whenever a meeting's status or progress changed.
    pub changed: Arc<dyn Fn() + Send + Sync>,
    pub load_engine: Box<EngineLoader>,
    pub load_detector: Box<DetectorLoader>,
    pub load_splitter: Box<SplitterLoader>,
    /// Extra folders to look for models in (tests); the real app uses the defaults.
    pub reuse_roots: Option<Vec<PathBuf>>,
    /// Called with a meeting id once its transcript is saved and it is `summarizing` (not when it waits).
    pub transcribed: Arc<dyn Fn(&str) + Send + Sync>,
}

#[derive(Default)]
struct Queue {
    ids: VecDeque<String>,
    current: Option<String>,
}

pub struct Worker {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    cancel: Arc<AtomicBool>,
}

impl Worker {
    pub fn start(deps: Deps) -> Worker {
        let queue: Arc<(Mutex<Queue>, Condvar)> = Arc::default();
        let cancel = Arc::new(AtomicBool::new(false));
        let w = Worker { queue: queue.clone(), cancel: cancel.clone() };
        std::thread::Builder::new()
            .name("transcriber".into())
            .spawn(move || {
                crate::platform::background_priority();
                worker_loop(deps, queue, cancel)
            })
            .expect("transcriber thread");
        w
    }

    /// Adds a meeting unless it is already queued or running.
    pub fn enqueue(&self, id: &str) {
        let (lock, cv) = &*self.queue;
        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
        if q.current.as_deref() != Some(id) && !q.ids.iter().any(|i| i == id) {
            q.ids.push_back(id.to_string());
            cv.notify_one();
        }
    }

    /// Stops the meeting being transcribed now (it is being deleted).
    pub fn cancel_if_running(&self, id: &str) {
        let (lock, _) = &*self.queue;
        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
        q.ids.retain(|i| i != id);
        if q.current.as_deref() == Some(id) {
            self.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// True while nothing is queued or running.
    #[cfg(test)]
    pub fn idle(&self) -> bool {
        let q = self.queue.0.lock().unwrap_or_else(|e| e.into_inner());
        q.ids.is_empty() && q.current.is_none()
    }
}

struct Loaded {
    model_id: &'static str,
    use_gpu: bool,
    engine: Box<dyn Engine>,
    detector: Box<dyn Detector + Send>,
    /// Loaded on first use; None when the model isn't downloaded or speaker separation is off.
    splitter: Option<Box<dyn SpeakerSplitter>>,
}

fn worker_loop(deps: Deps, queue: Arc<(Mutex<Queue>, Condvar)>, cancel: Arc<AtomicBool>) {
    let mut loaded: Option<Loaded> = None;
    let (lock, cv) = &*queue;
    loop {
        let id = {
            let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
            q.current = None;
            loop {
                if let Some(id) = q.ids.pop_front() {
                    q.current = Some(id.clone());
                    break id;
                }
                let (next, timeout) = cv.wait_timeout(q, IDLE_UNLOAD).unwrap_or_else(|e| e.into_inner());
                q = next;
                if timeout.timed_out() && q.ids.is_empty() {
                    // Free the model's memory when nothing has happened for a while.
                    loaded = None;
                }
            }
        };
        cancel.store(false, Ordering::Relaxed);
        process(&deps, &id, &mut loaded, &cancel);
        (deps.changed)();
    }
}

fn lock(storage: &Mutex<Storage>) -> std::sync::MutexGuard<'_, Storage> {
    storage.lock().unwrap_or_else(|e| e.into_inner())
}

fn fail(deps: &Deps, id: &str, code: &str, message: String) {
    let _ = lock(&deps.storage).update_meeting_file(id, |f| {
        f.status = MeetingStatus::Failed;
        f.progress = None;
        f.error = Some(ProcessingError { code: code.into(), message, detail: None });
    });
}

fn process(deps: &Deps, id: &str, loaded: &mut Option<Loaded>, cancel: &AtomicBool) {
    let (dir, config, transcript_only) = {
        let s = lock(&deps.storage);
        let Ok(file) = s.read_meeting_file(id) else { return };
        if file.status != MeetingStatus::Transcribing {
            return;
        }
        let Ok(dir) = s.dir(id) else { return };
        let mut config = Config::from_settings(s.settings_get().ok().flatten().as_ref());
        // Transcribe again with another model: this meeting only.
        if let Some(m) = file.retranscribe_model.as_deref().and_then(find).filter(|m| m.id != VAD.id) {
            config.model = m;
        }
        (dir, config, file.transcript_only)
    };
    let summarize = config.summarize && !transcript_only;

    let mut store = ModelStore::new(&deps.data_dir, config.reuse);
    if let Some(roots) = &deps.reuse_roots {
        store.reuse_roots = roots.clone();
    }
    let (Some(model_dir), Some(vad_dir)) = (store.locate(config.model).dir().map(Path::to_path_buf), store.locate(&VAD).dir().map(Path::to_path_buf)) else {
        fail(deps, id, NO_MODEL, "Download a speech model in Settings › Speech models to transcribe this call.".into());
        return;
    };

    let fresh = !matches!(loaded, Some(l) if l.model_id == config.model.id && l.use_gpu == config.use_gpu);
    if fresh {
        *loaded = None;
        let engine = match (deps.load_engine)(config.model, &model_dir, config.use_gpu) {
            Ok(e) => e,
            Err(e) => return fail(deps, id, "failed", e),
        };
        let detector = match (deps.load_detector)(&vad_dir.join(VAD.files[0].name)) {
            Ok(d) => d,
            Err(e) => return fail(deps, id, "failed", e),
        };
        *loaded = Some(Loaded { model_id: config.model.id, use_gpu: config.use_gpu, engine, detector, splitter: None });
    }
    let l = loaded.as_mut().expect("loaded above");
    if config.separate_speakers && l.splitter.is_none() {
        if let Some(dir) = store.locate(&DIARIZATION).dir() {
            // A broken speaker model shouldn't cost the transcript: carry on without it.
            l.splitter = (deps.load_splitter)(&dir.join(DIARIZATION.files[0].name)).ok();
        }
    }
    let splitter: Option<&mut dyn SpeakerSplitter> = match (&mut l.splitter, config.separate_speakers) {
        (Some(s), true) => Some(s.as_mut()),
        _ => None,
    };

    let started = Instant::now();
    let mut last_report = Instant::now() - Duration::from_secs(10);
    let mut report = |p: f32| {
        if last_report.elapsed() >= Duration::from_secs(1) || p >= 1.0 {
            last_report = Instant::now();
            let _ = lock(&deps.storage).update_meeting_file(id, |f| f.progress = Some(p as f64));
            (deps.changed)();
        }
    };
    let result = run(&dir, l.engine.as_mut(), l.detector.as_mut(), config.language.as_deref(), splitter, cancel, &mut report);

    match result {
        Ok(out) => {
            let s = lock(&deps.storage);
            if s.save_transcript(id, &out.lines).is_err() {
                drop(s);
                return fail(deps, id, "failed", crate::i18n::tr("error.save_transcript"));
            }
            let _ = s.update_meeting_file(id, |f| {
                if summarize {
                    f.status = MeetingStatus::Summarizing;
                    f.progress = Some(0.0);
                } else {
                    f.status = MeetingStatus::Ready;
                    f.progress = None;
                    f.summary_error = Some(ProcessingError {
                        code: WAITING.into(),
                        message: "This call is transcript only. Generate a summary when you want one.".into(),
                        detail: None,
                    });
                }
                f.error = None;
                f.retranscribe_model = None;
                if let Some(lang) = &out.language {
                    f.language = lang.clone();
                }
                f.engine = Some(out.engine.clone());
                if out.speakers >= 2 {
                    f.people_count = out.speakers as u32 + 1;
                }
                f.transcription = Some(TranscriptionInfo {
                    model: config.model.id.into(),
                    echo_lines_removed: out.echo_removed,
                    speech_ms: out.speech_ms,
                    took_ms: started.elapsed().as_millis() as u64,
                    finished_at: chrono::Local::now().fixed_offset().to_rfc3339(),
                });
            });
            drop(s);
            if summarize {
                (deps.transcribed)(id);
            }
        }
        // Cancelled means the meeting is being deleted; leave it alone.
        Err(PipelineError::Cancelled) => {}
        Err(PipelineError::Failed(e)) => fail(deps, id, "failed", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcriber::engine::{ChunkResult, Piece};
    use crate::transcriber::vad::{Shape, Span};
    use serde_json::json;

    struct WholeTrack;
    impl Detector for WholeTrack {
        fn detect(&mut self, s: &[f32]) -> Result<Vec<Span>, String> {
            Ok(if s.iter().any(|x| *x != 0.0) { vec![Span { start: 0, end: s.len() }] } else { vec![] })
        }
    }

    struct Echoes(&'static str);
    impl Engine for Echoes {
        fn transcribe(&mut self, s: &[f32], _l: Option<&str>) -> Result<ChunkResult, String> {
            if self.0 == "fail" {
                return Err("engine broke".into());
            }
            Ok(ChunkResult { pieces: vec![Piece { start_ms: 0, end_ms: s.len() as u64 / 16, text: self.0.into() }], language: None })
        }
        fn label(&self) -> &str {
            "Echoes"
        }
        fn chunking(&self) -> Shape {
            Shape { pad: 0, ..Shape::default() }
        }
    }

    fn sparse(path: &Path, size: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::File::create(path).unwrap().set_len(size).unwrap();
    }

    /// Storage with one meeting waiting for transcription (a second of tone on the Them track).
    fn setup(with_models: bool, text: &'static str) -> (tempfile::TempDir, Arc<Mutex<Storage>>, String, Worker) {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = Storage::new(dir.path().to_path_buf()).unwrap();
        let f = storage.create_meeting("zoom", "Zoom", chrono::DateTime::parse_from_rfc3339("2026-10-14T14:30:00-07:00").unwrap()).unwrap();
        let mdir = storage.dir(&f.id).unwrap();
        let spec = hound::WavSpec { channels: 1, sample_rate: 16000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(mdir.join("them.wav"), spec).unwrap();
        for _ in 0..16000 {
            w.write_sample(8000i16).unwrap();
        }
        w.finalize().unwrap();
        storage.update_meeting_file(&f.id, |m| m.status = MeetingStatus::Transcribing).unwrap();
        storage.settings_set(json!({ "activeSpeechModel": "whisper-small" })).unwrap();
        if with_models {
            for m in [find("whisper-small").unwrap(), &VAD] {
                for file in m.files {
                    sparse(&dir.path().join("models").join(m.id).join(file.name), file.size);
                }
            }
        }
        let storage = Arc::new(Mutex::new(storage));
        let worker = Worker::start(Deps {
            storage: storage.clone(),
            data_dir: dir.path().to_path_buf(),
            changed: Arc::new(|| {}),
            load_engine: Box::new(move |_, _, _| Ok(Box::new(Echoes(text)))),
            load_detector: Box::new(|_| Ok(Box::new(WholeTrack))),
            load_splitter: Box::new(|_| Err("no model".into())),
            reuse_roots: Some(vec![]),
            transcribed: Arc::new(|_| {}),
        });
        (dir, storage, f.id, worker)
    }

    fn wait(worker: &Worker) {
        let t = Instant::now();
        std::thread::sleep(Duration::from_millis(20));
        while !worker.idle() {
            assert!(t.elapsed() < Duration::from_secs(10), "worker stuck");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_waiting_meeting_gets_its_transcript() {
        let (_d, storage, id, worker) = setup(true, "Hello from the other side.");
        worker.enqueue(&id);
        wait(&worker);
        let s = storage.lock().unwrap();
        let m = s.get_meeting(&id).unwrap();
        assert_eq!(m.item.status, MeetingStatus::Summarizing);
        assert_eq!(m.transcript.len(), 1);
        assert_eq!(m.transcript[0].speaker, "them:0");
        assert_eq!(m.transcript[0].text, "Hello from the other side.");
        let f = s.read_meeting_file(&id).unwrap();
        assert_eq!(f.engine.as_deref(), Some("Echoes"));
        assert_eq!(f.transcription.unwrap().model, "whisper-small");
        assert_eq!(f.language, "en");
    }

    #[test]
    fn a_transcript_only_meeting_waits_for_generate_summary() {
        for (settings, flag) in [(json!({ "activeSpeechModel": "whisper-small", "summarizeAutomatically": false }), false), (json!({ "activeSpeechModel": "whisper-small" }), true)] {
            let (_d, storage, id, worker) = setup(true, "Salary numbers for next year.");
            storage.lock().unwrap().settings_set(settings).unwrap();
            storage.lock().unwrap().update_meeting_file(&id, |f| f.transcript_only = flag).unwrap();
            worker.enqueue(&id);
            wait(&worker);
            let m = storage.lock().unwrap().get_meeting(&id).unwrap();
            assert_eq!(m.item.status, MeetingStatus::Ready);
            assert_eq!(m.transcript.len(), 1);
            assert_eq!(m.summary_error.unwrap().code, WAITING);
        }
    }

    #[test]
    fn transcribe_again_uses_the_chosen_model_once() {
        let (_d, storage, id, worker) = setup(true, "Hello again.");
        // The default is a model that isn't downloaded; this meeting asks for one that is.
        storage.lock().unwrap().settings_set(json!({ "activeSpeechModel": "parakeet-v3" })).unwrap();
        storage.lock().unwrap().update_meeting_file(&id, |f| f.retranscribe_model = Some("whisper-small".into())).unwrap();
        worker.enqueue(&id);
        wait(&worker);
        let f = storage.lock().unwrap().read_meeting_file(&id).unwrap();
        assert_eq!(f.status, MeetingStatus::Summarizing);
        assert_eq!(f.transcription.unwrap().model, "whisper-small");
        assert_eq!(f.retranscribe_model, None);
    }

    #[test]
    fn without_a_model_it_fails_with_a_clear_reason() {
        let (_d, storage, id, worker) = setup(false, "x");
        worker.enqueue(&id);
        wait(&worker);
        let f = storage.lock().unwrap().read_meeting_file(&id).unwrap();
        assert_eq!(f.status, MeetingStatus::Failed);
        assert_eq!(f.error.unwrap().code, NO_MODEL);
    }

    #[test]
    fn an_engine_error_is_kept_for_the_banner() {
        let (_d, storage, id, worker) = setup(true, "fail");
        worker.enqueue(&id);
        wait(&worker);
        let m = storage.lock().unwrap().get_meeting(&id).unwrap();
        assert_eq!(m.item.status, MeetingStatus::Failed);
        assert_eq!(m.error.unwrap().message, "engine broke");
    }

    #[test]
    fn meetings_not_waiting_are_left_alone() {
        let (_d, storage, id, worker) = setup(true, "x");
        storage.lock().unwrap().update_meeting_file(&id, |f| f.status = MeetingStatus::Ready).unwrap();
        worker.enqueue(&id);
        wait(&worker);
        assert!(storage.lock().unwrap().get_meeting(&id).unwrap().transcript.is_empty());
    }

    #[test]
    fn settings_pick_the_model_language_and_gpu() {
        let c = Config::from_settings(Some(&json!({ "activeSpeechModel": "parakeet-v3", "meetingLanguage": "es", "useGpu": false })));
        assert_eq!((c.model.id, c.language.as_deref(), c.use_gpu, c.reuse), ("parakeet-v3", Some("es"), false, true));
        let d = Config::from_settings(Some(&json!({ "activeSpeechModel": "parakeet", "meetingLanguage": "auto" })));
        assert_eq!((d.model.id, d.language), (DEFAULT_MODEL, None));
        assert_eq!(Config::from_settings(None).model.id, DEFAULT_MODEL);
    }
}
