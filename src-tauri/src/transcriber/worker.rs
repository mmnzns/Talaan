//! The background transcriber: one thread, one meeting at a time, queue rebuilt from disk at launch.
//! A meeting enters as `transcribing` and leaves as `ready` (M5: `summarizing`) or `failed` with a reason.

use super::engine::Engine;
use super::diarize::SpeakerSplitter;
use crate::models::{Models, DEFAULT_SPEECH, DIARIZATION, VAD};
use voice_engine_models::{catalog, ModelHandle, ModelInfo};
use voice_engine_stt::SpeechBackend;
#[cfg(test)]
use voice_engine_stt::Fallback;
use super::pipeline::{run, Detector, PipelineError};
use crate::eta::{self, Device};
use crate::storage::model::{MeetingFile, MeetingStatus, ProcessingError, TranscriptionInfo};
use crate::storage::paths::{ME_WAV, THEM_WAV};
use crate::storage::speed::set_eta;
use crate::storage::Storage;
use serde_json::Value;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub const NO_MODEL: &str = "no_model";
/// `summary_error` code for a meeting that waits for Generate summary (transcript only, or automatic
/// summaries turned off), so a sensitive call never goes to a summary engine by itself.
pub const WAITING: &str = "waiting";
const IDLE_UNLOAD: Duration = Duration::from_secs(300);
/// Less speech than this says little about this computer's pace (start-up dominates), so it isn't learned.
const MIN_LEARN_SPEECH_MS: u64 = 60_000;

/// What the worker reads from `settings.json` for each meeting.
#[derive(Debug, Clone)]
pub struct Config {
    pub model: &'static ModelInfo,
    /// None = detect.
    pub language: Option<String>,
    pub use_gpu: bool,
    /// Split the Them track into Speaker 1, 2, 3 (when the model is downloaded).
    pub separate_speakers: bool,
    /// Hand each new transcript to the summarizer (Settings › Summary AI › Summarize automatically).
    pub summarize: bool,
}

impl Config {
    pub fn from_settings(v: Option<&Value>) -> Config {
        let v = v.cloned().unwrap_or(Value::Null);
        // Same reading as `models::used_ids`: an unknown or non-speech id ("diarization") is the default.
        let model = v["activeSpeechModel"].as_str().and_then(catalog::find).filter(|m| m.is_speech()).unwrap_or_else(|| catalog::find(DEFAULT_SPEECH).unwrap());
        let language = v["meetingLanguage"].as_str().filter(|l| !l.is_empty() && *l != "auto").map(str::to_string);
        Config {
            model,
            language,
            use_gpu: v["useGpu"].as_bool().unwrap_or(true),
            separate_speakers: v["separateSpeakers"].as_bool().unwrap_or(true),
            summarize: v["summarizeAutomatically"].as_bool().unwrap_or(true),
        }
    }
}

/// Each loader gets the model held (the engine erases nothing in use) and keeps the handle while the model is loaded.
/// The engine's `bool` is whether the card is asked for (`asks_card`), not the raw setting.
pub type EngineLoader = dyn Fn(&'static ModelInfo, ModelHandle, bool) -> Result<Box<dyn Engine>, String> + Send + Sync;
pub type DetectorLoader = dyn Fn(ModelHandle) -> Result<Box<dyn Detector + Send>, String> + Send + Sync;
pub type SplitterLoader = dyn Fn(ModelHandle) -> Result<Box<dyn SpeakerSplitter>, String> + Send + Sync;
/// Whether a model will run on the graphics chip, given the "use the GPU" setting (for the time estimate).
pub type OnGpu = dyn Fn(&ModelInfo, bool) -> bool + Send + Sync;

pub struct Deps {
    pub storage: Arc<Mutex<Storage>>,
    /// Called whenever a meeting's status or progress changed.
    pub changed: Arc<dyn Fn() + Send + Sync>,
    pub load_engine: Box<EngineLoader>,
    pub load_detector: Box<DetectorLoader>,
    pub load_splitter: Box<SplitterLoader>,
    /// Where the models are (the shared folder; a temp folder in tests).
    pub models: Arc<Models>,
    /// Called with a meeting id once its transcript is saved and it is `summarizing` (not when it waits).
    pub transcribed: Arc<dyn Fn(&str) + Send + Sync>,
    /// Called with a meeting id and title once its transcript is saved and nothing else will run on it by itself
    /// (transcript only, or automatic summaries off): the "Transcript ready" notification.
    pub transcript_ready: Arc<dyn Fn(&str, &str) + Send + Sync>,
    pub on_gpu: Arc<OnGpu>,
    /// Set when Talaan is quitting (`crate::quit`): a job that ends then is left as it is, not failed.
    pub quitting: Arc<AtomicBool>,
}

#[derive(Default)]
struct Queue {
    ids: VecDeque<String>,
    current: Option<String>,
}

pub struct Worker {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    cancel: Arc<AtomicBool>,
    /// The model ids the meeting being transcribed right now uses (speech model, voice detection, speaker model).
    busy: Arc<Mutex<Vec<&'static str>>>,
    /// The loaded engine, detector and splitter. The worker thread holds the lock for as long as a job runs, so
    /// whoever gets it (`try_lock`) knows the worker is idle.
    loaded: Arc<Mutex<Option<Loaded>>>,
    /// Drop the loaded engine before the next job (asked while a job was running, which keeps its engine).
    reload: Arc<AtomicBool>,
}

impl Worker {
    pub fn start(deps: Deps) -> Worker {
        let queue: Arc<(Mutex<Queue>, Condvar)> = Arc::default();
        let cancel = Arc::new(AtomicBool::new(false));
        let busy: Arc<Mutex<Vec<&'static str>>> = Arc::default();
        let loaded: Arc<Mutex<Option<Loaded>>> = Arc::default();
        let reload = Arc::new(AtomicBool::new(false));
        let w = Worker { queue: queue.clone(), cancel: cancel.clone(), busy: busy.clone(), loaded: loaded.clone(), reload: reload.clone() };
        std::thread::Builder::new()
            .name("transcriber".into())
            .spawn(move || {
                crate::platform::background_priority();
                worker_loop(deps, queue, cancel, busy, loaded, reload)
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

    /// Whether a meeting is being transcribed right now with this model (Delete waits until it is done).
    pub fn is_using(&self, model_id: &str) -> bool {
        self.busy.lock().unwrap_or_else(|e| e.into_inner()).contains(&model_id)
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

    /// Lets go of the loaded engine, detector and speaker model, so the next job loads them again: the helper's
    /// client was replaced, or a model file is being deleted. Right away when the worker is idle; when a job is
    /// running, before the next one.
    pub fn drop_loaded(&self) {
        self.reload.store(true, Ordering::SeqCst);
        if let Ok(mut l) = self.loaded.try_lock() {
            l.take();
            self.reload.store(false, Ordering::SeqCst);
        }
    }

    /// A model is about to be deleted: when the worker is idle and has it loaded (an idle engine keeps the model's
    /// file open for five minutes), lets go of it. A busy worker is left alone; `is_using` already says no to a
    /// model its job uses, and any other model isn't loaded.
    pub fn unload_model(&self, id: &str) {
        let mut l = match self.loaded.try_lock() {
            Ok(l) => l,
            Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return,
        };
        match l.as_mut() {
            // The engine, the voice detector and the speaker model go together: the next job loads them again.
            Some(x) if x.model_id == id || id == VAD => {
                l.take();
            }
            Some(x) if id == DIARIZATION => x.splitter = None,
            _ => {}
        }
    }

    /// Cancels the job running now, leaving its meeting as it is (Talaan is quitting).
    pub fn cancel_running(&self) {
        let (lock, _) = &*self.queue;
        let q = lock.lock().unwrap_or_else(|e| e.into_inner());
        if q.current.is_some() {
            self.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Stops everything now: nothing queued starts, the job running is cancelled (it stays as it is on disk), and
    /// once the worker is idle its loaded models are dropped, so no file or helper stays open. Waits up to `wait`
    /// for a running job to notice the cancel; after that the caller goes on (the app is about to exit).
    pub fn stop_all(&self, wait: Duration) {
        {
            let (lock, _) = &*self.queue;
            let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
            q.ids.clear();
            if q.current.is_some() {
                self.cancel.store(true, Ordering::Relaxed);
            }
        }
        let started = Instant::now();
        loop {
            match self.loaded.try_lock() {
                Ok(mut l) => return drop(l.take()),
                Err(std::sync::TryLockError::Poisoned(e)) => return drop(e.into_inner().take()),
                Err(std::sync::TryLockError::WouldBlock) if started.elapsed() >= wait => {
                    log::warn!("the transcriber was still busy when everything was stopped");
                    return;
                }
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }

    /// True while nothing is queued or running.
    #[cfg(test)]
    pub fn idle(&self) -> bool {
        let q = self.queue.0.lock().unwrap_or_else(|e| e.into_inner());
        q.ids.is_empty() && q.current.is_none()
    }
}

/// Whether the setting means the graphics card for this model on this computer: Metal for Whisper on the Mac,
/// Vulkan (Whisper) or DirectML (Parakeet) on Windows. A model with no card path here is never asked for one, so
/// the engine's `fallback` is never `NoCardPath` by design (and only `CardFailed` and `CardOutOfMemory` are failures).
pub fn asks_card(model: &ModelInfo, use_gpu: bool) -> bool {
    use_gpu
        && if cfg!(target_os = "macos") {
            model.gpu.mac_metal
        } else {
            cfg!(windows) && (model.gpu.windows_vulkan || model.gpu.windows_directml)
        }
}

struct Loaded {
    model_id: &'static str,
    use_gpu: bool,
    engine: Box<dyn Engine>,
    /// Where the model landed when it loaded (None for an engine that does not say).
    backend: Option<SpeechBackend>,
    detector: Box<dyn Detector + Send>,
    /// Loaded on first use; None when the model isn't downloaded or speaker separation is off.
    splitter: Option<Box<dyn SpeakerSplitter>>,
}

fn worker_loop(deps: Deps, queue: Arc<(Mutex<Queue>, Condvar)>, cancel: Arc<AtomicBool>, busy: Arc<Mutex<Vec<&'static str>>>, loaded: Arc<Mutex<Option<Loaded>>>, reload: Arc<AtomicBool>) {
    let (lock, cv) = &*queue;
    loop {
        let id = {
            let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
            q.current = None;
            loop {
                if let Some(id) = q.ids.pop_front() {
                    q.current = Some(id.clone());
                    // Under the queue lock, so a cancel (`stop_all`, `cancel_if_running`) can't land before it.
                    cancel.store(false, Ordering::Relaxed);
                    break id;
                }
                let (next, timeout) = cv.wait_timeout(q, IDLE_UNLOAD).unwrap_or_else(|e| e.into_inner());
                q = next;
                if timeout.timed_out() && q.ids.is_empty() {
                    // Free the model's memory when nothing has happened for a while.
                    loaded.lock().unwrap_or_else(|e| e.into_inner()).take();
                }
            }
        };
        let mut guard = loaded.lock().unwrap_or_else(|e| e.into_inner());
        if reload.swap(false, Ordering::SeqCst) {
            guard.take();
        }
        process(&deps, &id, &mut guard, &cancel, &busy);
        drop(guard);
        busy.lock().unwrap_or_else(|e| e.into_inner()).clear();
        (deps.changed)();
    }
}

fn lock(storage: &Mutex<Storage>) -> std::sync::MutexGuard<'_, Storage> {
    storage.lock().unwrap_or_else(|e| e.into_inner())
}

fn fail(deps: &Deps, id: &str, code: &str, message: String) {
    // Talaan is quitting and the helpers are going: the job ended because of that. The meeting stays `transcribing`
    // and is picked up again at the next launch.
    if deps.quitting.load(Ordering::SeqCst) {
        log::info!("transcription interrupted by quitting: meeting {id} stays as it is");
        return;
    }
    log::warn!("transcription failed: meeting {id} ({code}): {}", crate::logging::text(&message));
    let _ = lock(&deps.storage).update_meeting_file(id, |f| {
        f.status = MeetingStatus::Failed;
        f.progress = None;
        f.eta = None;
        f.error = Some(ProcessingError { code: code.into(), message, detail: None });
    });
}

/// The settings for one meeting: "Transcribe again" with another model applies to this meeting only.
fn config_for(s: &Storage, file: &MeetingFile) -> Config {
    let mut config = Config::from_settings(s.settings_get().ok().flatten().as_ref());
    if let Some(m) = file.retranscribe_model.as_deref().and_then(catalog::find).filter(|m| m.is_speech()) {
        config.model = m;
    }
    config
}

/// Seconds of recording: the meeting's length, or for one without it (an older import) the longer track.
fn audio_secs(dir: &Path, file: &MeetingFile) -> f64 {
    if file.duration_ms > 0 {
        return file.duration_ms as f64 / 1000.0;
    }
    [ME_WAV, THEM_WAV].iter().filter_map(|n| crate::recorder::wav::wav_duration_ms(&dir.join(n))).max().unwrap_or(0) as f64 / 1000.0
}

/// Puts a first estimate on a meeting that is about to be transcribed, so Stop can say how long it will take.
/// Meetings already waiting or being transcribed go first: this one starts when the last of them should be done.
pub fn stamp_estimate(s: &Storage, id: &str, on_gpu: &OnGpu) {
    let (Ok(file), Ok(dir)) = (s.read_meeting_file(id), s.dir(id)) else { return };
    if file.status != MeetingStatus::Transcribing {
        return;
    }
    let config = config_for(s, &file);
    let profile = eta::stt_profile(config.model, on_gpu(config.model, config.use_gpu));
    let total = profile.total(s.speed_rate(&profile.key), audio_secs(&dir, &file));
    let now = chrono::Local::now();
    let ahead = s
        .ids_with_status(MeetingStatus::Transcribing)
        .unwrap_or_default()
        .into_iter()
        .filter(|other| other != id)
        .filter_map(|other| s.read_meeting_file(&other).ok()?.eta)
        .filter_map(|e| chrono::DateTime::parse_from_rfc3339(&e.finish_at).ok())
        .map(|finish| (finish.with_timezone(&chrono::Local) - now).num_milliseconds() as f64 / 1000.0)
        .fold(0.0_f64, f64::max);
    let _ = s.update_meeting_file(id, |f| set_eta(f, ahead + total, profile.device == Device::Cpu));
}

/// Meetings re-queued at launch: estimates from before the restart mean nothing now, so each gets a fresh one at
/// this computer's pace, chained in queue order. A Stop made before the worker picks them up then waits for them.
pub fn restamp_queue(s: &Storage, ids: &[String], on_gpu: &OnGpu) {
    for id in ids {
        let _ = s.update_meeting_file(id, |f| f.eta = None);
    }
    for id in ids {
        stamp_estimate(s, id, on_gpu);
    }
}

fn process(deps: &Deps, id: &str, loaded: &mut Option<Loaded>, cancel: &AtomicBool, busy: &Mutex<Vec<&'static str>>) {
    if deps.quitting.load(Ordering::SeqCst) {
        return;
    }
    let (dir, config, transcript_only, audio) = {
        let s = lock(&deps.storage);
        let Ok(file) = s.read_meeting_file(id) else { return };
        if file.status != MeetingStatus::Transcribing {
            return;
        }
        let Ok(dir) = s.dir(id) else { return };
        let config = config_for(&s, &file);
        let audio = audio_secs(&dir, &file);
        (dir, config, file.transcript_only, audio)
    };
    let summarize = config.summarize && !transcript_only;
    log::info!(
        "transcribing meeting {id}: model {}, GPU {}, language {}, speakers {}",
        config.model.id,
        if config.use_gpu { "on" } else { "off" },
        config.language.as_deref().unwrap_or("auto"),
        if config.separate_speakers { "separated" } else { "not separated" }
    );

    {
        let mut b = busy.lock().unwrap_or_else(|e| e.into_inner());
        *b = vec![config.model.id, VAD];
        if config.separate_speakers {
            b.push(DIARIZATION);
        }
    }
    let no_model = || "Download a speech model in Settings › Speech models to transcribe this call.".to_string();
    let (Ok(model_handle), Ok(vad_handle)) = (deps.models.hold(config.model.id), deps.models.hold(VAD)) else {
        fail(deps, id, NO_MODEL, no_model());
        return;
    };

    let fresh = !matches!(loaded, Some(l) if l.model_id == config.model.id && l.use_gpu == config.use_gpu);
    // The time estimate: loading the model (when it isn't loaded yet), then the audio at this computer's pace.
    let profile = eta::stt_profile(config.model, (deps.on_gpu)(config.model, config.use_gpu));
    let on_cpu = profile.device == Device::Cpu;
    let estimate = {
        let s = lock(&deps.storage);
        let total = profile.total(s.speed_rate(&profile.key), audio) - if fresh { 0.0 } else { profile.overhead };
        let _ = s.update_meeting_file(id, |f| set_eta(f, total, on_cpu));
        total
    };
    (deps.changed)();
    let job = Instant::now();
    if fresh {
        *loaded = None;
        let load = Instant::now();
        let engine = match (deps.load_engine)(config.model, model_handle, asks_card(config.model, config.use_gpu)) {
            Ok(e) => e,
            Err(e) => return fail(deps, id, "failed", e),
        };
        let detector = match (deps.load_detector)(vad_handle) {
            Ok(d) => d,
            Err(e) => return fail(deps, id, "failed", e),
        };
        let backend = engine.backend();
        match &backend {
            Some(b) => log::info!("speech engine loaded in {:.1}s: {} on {}{}", load.elapsed().as_secs_f32(), engine.label(), b.backend, if crate::gpu::is_card_failure(b.fallback) { " (the graphics card failed)" } else { "" }),
            None => log::info!("speech engine loaded in {:.1}s: {}", load.elapsed().as_secs_f32(), engine.label()),
        }
        *loaded = Some(Loaded { model_id: config.model.id, use_gpu: config.use_gpu, engine, backend, detector, splitter: None });
    }
    let l = loaded.as_mut().expect("loaded above");
    if config.separate_speakers && l.splitter.is_none() {
        if let Ok(handle) = deps.models.hold(DIARIZATION) {
            // A broken speaker model shouldn't cost the transcript: carry on without it.
            l.splitter = (deps.load_splitter)(handle).inspect_err(|e| log::warn!("speaker separation unavailable: {}", crate::logging::text(e))).ok();
        }
    }
    let splitter: Option<&mut dyn SpeakerSplitter> = match (&mut l.splitter, config.separate_speakers) {
        (Some(s), true) => Some(s.as_mut()),
        _ => None,
    };

    let started = Instant::now();
    let mut last_report = Instant::now() - Duration::from_secs(10);
    let mut first_report: Option<Instant> = None;
    let mut report = |p: f32| {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let running = first_report.get_or_insert_with(Instant::now).elapsed().as_secs_f64();
        if last_report.elapsed() >= Duration::from_secs(1) || p >= 1.0 {
            last_report = Instant::now();
            let left = eta::remaining(estimate, job.elapsed().as_secs_f64(), p as f64, running);
            let _ = lock(&deps.storage).update_meeting_file(id, |f| {
                f.progress = Some(p as f64);
                set_eta(f, left, on_cpu);
            });
            (deps.changed)();
        }
    };
    let result = run(&dir, l.engine.as_mut(), l.detector.as_mut(), config.language.as_deref(), splitter, cancel, &mut report);
    // Where the work ended up (the engine knows after the run: the speech helper can fall back to the processor mid-way).
    let backend = l.engine.backend().or_else(|| l.backend.clone());
    if let Some(b) = backend.as_ref().filter(|b| crate::gpu::is_card_failure(b.fallback)) {
        crate::gpu::note_stt_fallback(&b.backend);
    }

    match result {
        Ok(out) => {
            let (count, voices) = (out.lines.len(), out.speakers);
            log::info!(
                "transcribed meeting {id} in {:.1}s: engine {}, {count} lines, {:.0}s of speech, {} echo lines removed, language {}, {} speakers",
                started.elapsed().as_secs_f32(),
                out.engine,
                out.speech_ms as f32 / 1000.0,
                out.echo_removed,
                out.language.as_deref().unwrap_or("?"),
                voices
            );
            let s = lock(&deps.storage);
            if s.save_transcript(id, &out.lines).is_err() {
                drop(s);
                return fail(deps, id, "failed", crate::i18n::tr("error.save_transcript"));
            }
            // A Whisper meeting that fell back to the processor midway has already moved the GPU plan (gpu.json), so
            // the profile worked out again differs: that run's time belongs to neither device, and isn't learned.
            let same_device = eta::stt_profile(config.model, (deps.on_gpu)(config.model, config.use_gpu)).key == profile.key;
            if same_device && out.speech_ms >= MIN_LEARN_SPEECH_MS {
                s.learn_speed(&profile.key, started.elapsed().as_secs_f64(), audio);
            }
            let mut title = String::new();
            let _ = s.update_meeting_file(id, |f| {
                f.eta = None;
                title = f.title.clone();
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
                    gpu_fell_back: backend.as_ref().is_some_and(|b| crate::gpu::is_card_failure(b.fallback)),
                    device: backend.as_ref().and_then(|b| b.device.clone()),
                });
            });
            drop(s);
            if summarize {
                (deps.transcribed)(id);
            } else {
                (deps.transcript_ready)(id, &title);
            }
        }
        // Cancelled means the meeting is being deleted; leave it alone.
        Err(PipelineError::Cancelled) => log::info!("transcription cancelled: meeting {id} (being deleted)"),
        // The meeting was deleted (or everything was stopped) and the engine said so with an error instead of
        // returning: same as cancelled.
        Err(PipelineError::Failed(_)) if cancel.load(Ordering::Relaxed) => log::info!("transcription cancelled: meeting {id}"),
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
        let (d, s, id, w, _) = setup_with(with_models, text);
        (d, s, id, w)
    }

    /// Also returns the titles "Transcript ready" was posted for.
    fn setup_with(with_models: bool, text: &'static str) -> (tempfile::TempDir, Arc<Mutex<Storage>>, String, Worker, Arc<Mutex<Vec<String>>>) {
        setup_full(with_models, 1, Box::new(move |_, _, _| Ok(Box::new(Echoes(text)))), Arc::new(|_, _| false))
    }

    /// `secs` of tone on the Them track, the given engine and graphics-chip answer.
    fn setup_full(with_models: bool, secs: usize, load_engine: Box<EngineLoader>, on_gpu: Arc<OnGpu>) -> (tempfile::TempDir, Arc<Mutex<Storage>>, String, Worker, Arc<Mutex<Vec<String>>>) {
        setup_quitting(with_models, secs, load_engine, on_gpu, Arc::default())
    }

    /// `setup_full` with the "Talaan is quitting" flag the test holds.
    fn setup_quitting(with_models: bool, secs: usize, load_engine: Box<EngineLoader>, on_gpu: Arc<OnGpu>, quitting: Arc<AtomicBool>) -> (tempfile::TempDir, Arc<Mutex<Storage>>, String, Worker, Arc<Mutex<Vec<String>>>) {
        let ready = Arc::new(Mutex::new(vec![]));
        let r2 = ready.clone();
        let dir = tempfile::tempdir().unwrap();
        let mut storage = Storage::new(dir.path().to_path_buf()).unwrap();
        let f = storage.create_meeting("zoom", "Zoom", chrono::DateTime::parse_from_rfc3339("2026-10-14T14:30:00-07:00").unwrap()).unwrap();
        let mdir = storage.dir(&f.id).unwrap();
        let spec = hound::WavSpec { channels: 1, sample_rate: 16000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(mdir.join("them.wav"), spec).unwrap();
        for _ in 0..16000 * secs {
            w.write_sample(8000i16).unwrap();
        }
        w.finalize().unwrap();
        storage.update_meeting_file(&f.id, |m| m.status = MeetingStatus::Transcribing).unwrap();
        storage.settings_set(json!({ "activeSpeechModel": "whisper-small" })).unwrap();
        let models = Arc::new(Models::open(Some(dir.path())).unwrap());
        if with_models {
            for id in ["whisper-small", VAD] {
                for file in catalog::find(id).unwrap().files {
                    sparse(&models.root().join(id).join(file.name), file.size);
                }
            }
        }
        let storage = Arc::new(Mutex::new(storage));
        let worker = Worker::start(Deps {
            storage: storage.clone(),
            changed: Arc::new(|| {}),
            load_engine,
            load_detector: Box::new(|_| Ok(Box::new(WholeTrack))),
            load_splitter: Box::new(|_| Err("no model".into())),
            models,
            transcribed: Arc::new(|_| {}),
            transcript_ready: Arc::new(move |_, t| r2.lock().unwrap().push(t.to_string())),
            on_gpu,
            quitting,
        });
        (dir, storage, f.id, worker, ready)
    }

    fn wait(worker: &Worker) {
        let t = Instant::now();
        std::thread::sleep(Duration::from_millis(20));
        while !worker.idle() {
            assert!(t.elapsed() < Duration::from_secs(10), "worker stuck");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Says where it ran, like the engine's.
    struct Placed(SpeechBackend);
    impl Engine for Placed {
        fn transcribe(&mut self, s: &[f32], _l: Option<&str>) -> Result<ChunkResult, String> {
            Echoes("Hello.").transcribe(s, None)
        }
        fn label(&self) -> &str {
            "Placed"
        }
        fn chunking(&self) -> Shape {
            Shape { pad: 0, ..Shape::default() }
        }
        fn backend(&self) -> Option<SpeechBackend> {
            Some(self.0.clone())
        }
    }

    fn info_after(backend: SpeechBackend) -> TranscriptionInfo {
        let (_d, storage, id, worker, _) = setup_full(true, 1, Box::new(move |_, _, _| Ok(Box::new(Placed(backend.clone())))), Arc::new(|_, _| false));
        worker.enqueue(&id);
        wait(&worker);
        let f = storage.lock().unwrap().read_meeting_file(&id).unwrap();
        f.transcription.unwrap()
    }

    #[test]
    fn the_meeting_records_where_the_model_ran_and_a_real_fallback() {
        let card = info_after(SpeechBackend { backend: "metal".into(), device: Some("Apple M3".into()), fallback: None });
        assert!(!card.gpu_fell_back);
        assert_eq!(card.device.as_deref(), Some("Apple M3"));
        let fell = info_after(SpeechBackend { backend: "cpu".into(), device: None, fallback: Some(Fallback::CardFailed) });
        assert!(fell.gpu_fell_back);
        assert_eq!(fell.device, None);
        // Both fields are left out of the file when they say nothing.
        let plain = serde_json::to_value(info_after(SpeechBackend { backend: "cpu".into(), device: None, fallback: None })).unwrap();
        assert!(plain.get("gpuFellBack").is_none() && plain.get("device").is_none());
        assert_eq!(serde_json::to_value(fell).unwrap()["gpuFellBack"], true);
    }

    #[test]
    fn the_meeting_says_the_card_failed_only_for_a_card_failure() {
        let says = |fallback| info_after(SpeechBackend { backend: "cpu".into(), device: None, fallback: Some(fallback) }).gpu_fell_back;
        assert!(says(Fallback::CardFailed));
        assert!(says(Fallback::CardOutOfMemory));
        assert!(!says(Fallback::RememberedFailure));
        assert!(!says(Fallback::NoCardHelper));
        assert!(!says(Fallback::NoCardPath));
    }

    #[test]
    fn the_card_is_only_asked_for_where_the_model_has_a_card_path() {
        let turbo = catalog::find("whisper-turbo").unwrap();
        let parakeet = catalog::find("parakeet-ultra").unwrap();
        assert!(!asks_card(turbo, false));
        assert_eq!(asks_card(turbo, true), cfg!(target_os = "macos") || (cfg!(windows) && (turbo.gpu.windows_vulkan || turbo.gpu.windows_directml)));
        if cfg!(target_os = "macos") {
            assert!(asks_card(turbo, true));
            assert!(!asks_card(parakeet, true), "Parakeet has no card path on the Mac, so `fallback` is never a failure by design");
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
    fn stop_gets_a_first_estimate_and_the_finished_transcript_drops_it() {
        let (_d, storage, id, worker, ready) = setup_full(true, 61, Box::new(|_, _, _| Ok(Box::new(Echoes("Hello.")))), Arc::new(|_, _| false));
        {
            let s = storage.lock().unwrap();
            s.update_meeting_file(&id, |f| f.duration_ms = 3_600_000).unwrap();
            stamp_estimate(&s, &id, &|_, _| false);
            let item = &s.list_meetings(&Default::default()).unwrap()[0];
            let eta = item.eta.clone().expect("an estimate");
            assert!(eta.on_cpu);
            // An hour of Whisper Small on the processor: about 11 min by default.
            let finish = chrono::DateTime::parse_from_rfc3339(&eta.finish_at).unwrap();
            let mins = (finish.with_timezone(&chrono::Local) - chrono::Local::now()).num_minutes();
            assert!((8..=15).contains(&mins), "{mins}");
        }
        worker.enqueue(&id);
        wait(&worker);
        let s = storage.lock().unwrap();
        assert_eq!(s.read_meeting_file(&id).unwrap().eta, None);
        // It goes on to the summary, so no "Transcript ready".
        assert!(ready.lock().unwrap().is_empty());
        // This computer's pace is kept for the next estimate (the fake engine is instant).
        assert!(s.speed_rate("stt:whisper-small:cpu").is_some_and(|r| r < 0.01));
    }

    #[test]
    fn a_short_call_teaches_nothing() {
        let (_d, storage, id, worker, _) = setup_with(true, "Hello.");
        storage.lock().unwrap().update_meeting_file(&id, |f| f.duration_ms = 3_600_000).unwrap();
        worker.enqueue(&id);
        wait(&worker);
        // An hour-long meeting with one second of speech says nothing about the pace.
        assert_eq!(storage.lock().unwrap().speed_rate("stt:whisper-small:cpu"), None);
    }

    /// Says it runs on the graphics chip until its first chunk, then "falls back" (as the GPU helper does when it
    /// fails: gpu.json then says to use the processor).
    struct FallsBack(Arc<AtomicBool>);
    impl Engine for FallsBack {
        fn transcribe(&mut self, s: &[f32], _l: Option<&str>) -> Result<ChunkResult, String> {
            self.0.store(false, Ordering::Relaxed);
            Ok(ChunkResult { pieces: vec![Piece { start_ms: 0, end_ms: s.len() as u64 / 16, text: "Hello.".into() }], language: None })
        }
        fn label(&self) -> &str {
            "FallsBack"
        }
        fn chunking(&self) -> Shape {
            Shape { pad: 0, ..Shape::default() }
        }
    }

    #[test]
    fn a_fallback_to_the_processor_mid_job_teaches_neither_speed() {
        let gpu = Arc::new(AtomicBool::new(true));
        let (g1, g2) = (gpu.clone(), gpu.clone());
        let (_d, storage, id, worker, _) = setup_full(
            true,
            61,
            Box::new(move |_, _, _| Ok(Box::new(FallsBack(g1.clone())))),
            Arc::new(move |m, use_gpu| use_gpu && m.kind == voice_engine_models::Kind::Whisper && g2.load(Ordering::Relaxed)),
        );
        worker.enqueue(&id);
        wait(&worker);
        let s = storage.lock().unwrap();
        assert_eq!(s.read_meeting_file(&id).unwrap().status, MeetingStatus::Summarizing);
        assert_eq!(s.speed_rate("stt:whisper-small:gpu"), None, "processor time under the graphics chip's name");
        assert_eq!(s.speed_rate("stt:whisper-small:cpu"), None);
    }

    #[test]
    fn a_meeting_behind_others_waits_for_them_in_its_estimate() {
        let (_d, storage, first, _worker) = setup(true, "x");
        let s = storage.lock().unwrap();
        let second = s.create_meeting("zoom", "Zoom", chrono::DateTime::parse_from_rfc3339("2026-10-14T15:30:00-07:00").unwrap()).unwrap().id;
        let minutes_left = |id: &str| {
            let eta = s.read_meeting_file(id).unwrap().eta.unwrap();
            let finish = chrono::DateTime::parse_from_rfc3339(&eta.finish_at).unwrap();
            (finish.with_timezone(&chrono::Local) - chrono::Local::now()).num_seconds() as f64 / 60.0
        };
        for id in [&first, &second] {
            s.update_meeting_file(id, |f| {
                f.status = MeetingStatus::Transcribing;
                f.duration_ms = 3_600_000;
            })
            .unwrap();
        }
        stamp_estimate(&s, &first, &|_, _| false);
        stamp_estimate(&s, &second, &|_, _| false);
        let (a, b) = (minutes_left(&first), minutes_left(&second));
        // Each is about 11 min of work; the second starts when the first is done.
        assert!((b - 2.0 * a).abs() < 0.5, "{a} then {b}");
    }

    #[test]
    fn meetings_requeued_at_launch_get_chained_estimates_that_a_later_stop_counts() {
        let (_d, storage, first, _worker) = setup(true, "x");
        let s = storage.lock().unwrap();
        let at = |h: u32| chrono::DateTime::parse_from_rfc3339(&format!("2026-10-14T{h}:30:00-07:00")).unwrap();
        let second = s.create_meeting("zoom", "Zoom", at(15)).unwrap().id;
        let third = s.create_meeting("zoom", "Zoom", at(16)).unwrap().id;
        let minutes_left = |id: &str| {
            let eta = s.read_meeting_file(id).unwrap().eta.unwrap();
            let finish = chrono::DateTime::parse_from_rfc3339(&eta.finish_at).unwrap();
            (finish.with_timezone(&chrono::Local) - chrono::Local::now()).num_seconds() as f64 / 60.0
        };
        for id in [&first, &second] {
            s.update_meeting_file(id, |f| {
                f.status = MeetingStatus::Transcribing;
                f.duration_ms = 3_600_000;
                // A stale estimate from before the restart (hours away): it must not count.
                set_eta(f, 5.0 * 3600.0, true);
            })
            .unwrap();
        }
        restamp_queue(&s, &[first.clone(), second.clone()], &|_, _| false);
        let (a, b) = (minutes_left(&first), minutes_left(&second));
        assert!((5.0..30.0).contains(&a), "fresh, not stale: {a}");
        assert!((b - 2.0 * a).abs() < 0.5, "chained in queue order: {a} then {b}");
        // A Stop made now, before the worker has picked them up, waits for both.
        s.update_meeting_file(&third, |f| {
            f.status = MeetingStatus::Transcribing;
            f.duration_ms = 3_600_000;
        })
        .unwrap();
        stamp_estimate(&s, &third, &|_, _| false);
        let c = minutes_left(&third);
        assert!((c - 3.0 * a).abs() < 0.5, "{a}, {b} then {c}");
    }

    #[test]
    fn a_finished_meeting_has_no_estimate_in_the_list() {
        let (_d, storage, id, _worker) = setup(true, "x");
        let s = storage.lock().unwrap();
        stamp_estimate(&s, &id, &|_, _| true);
        s.update_meeting_file(&id, |f| f.status = MeetingStatus::Ready).unwrap();
        assert!(s.list_meetings(&Default::default()).unwrap()[0].eta.is_none());
    }

    #[test]
    fn a_transcript_only_meeting_says_transcript_ready() {
        let (_d, storage, id, worker, ready) = setup_with(true, "Salary numbers for next year.");
        storage.lock().unwrap().update_meeting_file(&id, |f| f.transcript_only = true).unwrap();
        worker.enqueue(&id);
        wait(&worker);
        assert_eq!(ready.lock().unwrap().len(), 1);
        assert!(ready.lock().unwrap()[0].starts_with("Call, Zoom"));
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
    fn the_models_of_the_meeting_being_transcribed_are_in_use_until_it_is_done() {
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        let (started_tx, go_rx) = (Mutex::new(started_tx), Mutex::new(go_rx));
        let load: Box<EngineLoader> = Box::new(move |_, _, _| {
            started_tx.lock().unwrap().send(()).unwrap();
            go_rx.lock().unwrap().recv().unwrap();
            Ok(Box::new(Echoes("Hi.")))
        });
        let (_d, _storage, id, worker, _) = setup_full(true, 1, load, Arc::new(|_, _| false));
        assert!(!worker.is_using("whisper-small"));
        worker.enqueue(&id);
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        // The speech model, the voice detector and (speakers are separated by default) the speaker model.
        for id in ["whisper-small", "vad", "diarization"] {
            assert!(worker.is_using(id), "{id}");
        }
        assert!(!worker.is_using("parakeet-v3"));
        go_tx.send(()).unwrap();
        wait(&worker);
        assert!(!worker.is_using("whisper-small"));
    }

    /// An engine that says when it is dropped (the model let go).
    struct Tracked(Arc<AtomicBool>);
    impl Engine for Tracked {
        fn transcribe(&mut self, s: &[f32], l: Option<&str>) -> Result<ChunkResult, String> {
            Echoes("Hi.").transcribe(s, l)
        }
        fn label(&self) -> &str {
            "Tracked"
        }
        fn chunking(&self) -> Shape {
            Shape { pad: 0, ..Shape::default() }
        }
    }
    impl Drop for Tracked {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn stopping_everything_lets_go_of_the_loaded_engine() {
        let dropped = Arc::new(AtomicBool::new(false));
        let d2 = dropped.clone();
        let (_d, _storage, id, worker, _) = setup_full(true, 1, Box::new(move |_, _, _| Ok(Box::new(Tracked(d2.clone())))), Arc::new(|_, _| false));
        worker.enqueue(&id);
        wait(&worker);
        // Idle, but the engine stays loaded for the next meeting.
        assert!(!dropped.load(Ordering::SeqCst));
        worker.stop_all(Duration::from_secs(5));
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[test]
    fn dropping_the_loaded_engine_makes_the_next_meeting_load_again() {
        let loads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let (l2, d2) = (loads.clone(), dropped.clone());
        let (_d, storage, id, worker, _) = setup_full(true, 1, Box::new(move |_, _, _| {
            l2.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Tracked(d2.clone())))
        }), Arc::new(|_, _| false));
        worker.enqueue(&id);
        wait(&worker);
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        // Idle: let go at once.
        worker.drop_loaded();
        assert!(dropped.load(Ordering::SeqCst));
        let again = |storage: &Arc<Mutex<Storage>>| {
            storage.lock().unwrap().update_meeting_file(&id, |f| f.status = MeetingStatus::Transcribing).unwrap();
            worker.enqueue(&id);
            wait(&worker);
        };
        again(&storage);
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        // Without the call the engine is kept.
        again(&storage);
        assert_eq!(loads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn deleting_a_model_unloads_it_only_when_it_is_the_loaded_one() {
        let dropped = Arc::new(AtomicBool::new(false));
        let d2 = dropped.clone();
        let (_d, _storage, id, worker, _) = setup_full(true, 1, Box::new(move |_, _, _| Ok(Box::new(Tracked(d2.clone())))), Arc::new(|_, _| false));
        worker.enqueue(&id);
        wait(&worker);
        // The loaded model is whisper-small; another model's delete leaves it be.
        worker.unload_model("parakeet-v3");
        assert!(!dropped.load(Ordering::SeqCst));
        worker.unload_model("whisper-small");
        assert!(dropped.load(Ordering::SeqCst));
        worker.unload_model("whisper-small"); // nothing loaded: fine
    }

    #[test]
    fn stopping_everything_cancels_the_running_job_and_drops_the_queue() {
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        let (started_tx, go_rx) = (Mutex::new(started_tx), Mutex::new(go_rx));
        let dropped = Arc::new(AtomicBool::new(false));
        let d2 = dropped.clone();
        let load: Box<EngineLoader> = Box::new(move |_, _, _| {
            started_tx.lock().unwrap().send(()).unwrap();
            go_rx.lock().unwrap().recv().unwrap();
            Ok(Box::new(Tracked(d2.clone())))
        });
        let (_d, storage, id, worker, _) = setup_full(true, 1, load, Arc::new(|_, _| false));
        let second = {
            let s = storage.lock().unwrap();
            let second = s.create_meeting("zoom", "Zoom", chrono::DateTime::parse_from_rfc3339("2026-10-14T15:30:00-07:00").unwrap()).unwrap().id;
            s.update_meeting_file(&second, |f| f.status = MeetingStatus::Transcribing).unwrap();
            second
        };
        worker.enqueue(&id);
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        worker.enqueue(&second);
        // The job is stuck loading: stop_all gives up waiting, having cancelled it and emptied the queue.
        worker.stop_all(Duration::from_millis(100));
        assert!(!dropped.load(Ordering::SeqCst));
        go_tx.send(()).unwrap();
        wait(&worker);
        // The cancelled job left the meeting as it was; the queued one never ran; the engine was let go.
        let s = storage.lock().unwrap();
        assert_eq!(s.read_meeting_file(&id).unwrap().status, MeetingStatus::Transcribing);
        assert_eq!(s.read_meeting_file(&second).unwrap().status, MeetingStatus::Transcribing);
        assert!(s.get_meeting(&id).unwrap().transcript.is_empty());
        assert!(s.get_meeting(&second).unwrap().transcript.is_empty());
    }

    /// Errors as soon as the first chunk is asked for, after setting the quitting flag (what the helper's
    /// shutdown looks like from the engine: the request fails).
    struct DiesWhenQuitting(Arc<AtomicBool>);
    impl Engine for DiesWhenQuitting {
        fn transcribe(&mut self, _s: &[f32], _l: Option<&str>) -> Result<ChunkResult, String> {
            self.0.store(true, Ordering::SeqCst);
            Err("the helper went away".into())
        }
        fn label(&self) -> &str {
            "Dies"
        }
        fn chunking(&self) -> Shape {
            Shape { pad: 0, ..Shape::default() }
        }
    }

    #[test]
    fn a_job_that_ends_because_talaan_is_quitting_leaves_the_meeting_transcribing() {
        let quitting = Arc::new(AtomicBool::new(false));
        let q2 = quitting.clone();
        let (_d, storage, id, worker, _) = setup_quitting(true, 1, Box::new(move |_, _, _| Ok(Box::new(DiesWhenQuitting(q2.clone())))), Arc::new(|_, _| false), quitting);
        worker.enqueue(&id);
        wait(&worker);
        let f = storage.lock().unwrap().read_meeting_file(&id).unwrap();
        assert_eq!(f.status, MeetingStatus::Transcribing);
        assert!(f.error.is_none());
    }

    #[test]
    fn the_same_error_without_quitting_still_fails_the_meeting() {
        let (_d, storage, id, worker, _) = setup_quitting(true, 1, Box::new(|_, _, _| Ok(Box::new(DiesWhenQuitting(Arc::default())))), Arc::new(|_, _| false), Arc::default());
        worker.enqueue(&id);
        wait(&worker);
        assert_eq!(storage.lock().unwrap().read_meeting_file(&id).unwrap().status, MeetingStatus::Failed);
    }

    #[test]
    fn nothing_starts_once_talaan_is_quitting() {
        let (_d, storage, id, worker, _) = setup_quitting(true, 1, Box::new(|_, _, _| Ok(Box::new(Echoes("Hi.")))), Arc::new(|_, _| false), Arc::new(AtomicBool::new(true)));
        worker.enqueue(&id);
        wait(&worker);
        assert_eq!(storage.lock().unwrap().read_meeting_file(&id).unwrap().status, MeetingStatus::Transcribing);
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
        assert_eq!((c.model.id, c.language.as_deref(), c.use_gpu), ("parakeet-v3", Some("es"), false));
        let d = Config::from_settings(Some(&json!({ "activeSpeechModel": "parakeet", "meetingLanguage": "auto" })));
        assert_eq!((d.model.id, d.language), (DEFAULT_SPEECH, None));
        assert_eq!(Config::from_settings(None).model.id, DEFAULT_SPEECH);
        // A model that isn't a speech model (voice detection, speaker separation) is the default too.
        for id in ["vad", "diarization", "qwen3.5-4b"] {
            assert_eq!(Config::from_settings(Some(&json!({ "activeSpeechModel": id }))).model.id, DEFAULT_SPEECH, "{id}");
        }
        // The engine's newer ids are speech models like the rest.
        assert_eq!(Config::from_settings(Some(&json!({ "activeSpeechModel": "whisper-small-en" }))).model.id, "whisper-small-en");
    }
}
