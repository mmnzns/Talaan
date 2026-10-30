//! The background summarizer: one thread, one meeting at a time, queue rebuilt from `summarizing` at launch.
//! A meeting enters `summarizing` and leaves `ready`, with a summary or with `summary_error` saying why not.

use super::prompt::Options;
use super::summarize::{summarize, too_short, EngineError, SummarizeError, SummaryEngine};
use super::templates;
use crate::eta::{self, Device, Profile};
use crate::storage::model::{MeetingStatus, ProcessingError};
use crate::storage::speed::set_eta;
use crate::storage::Storage;
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// `summary_error` codes. `no_engine`: nothing set up (or not reachable); `failed`: the AI's answer was
/// unusable or the call failed. Both leave the transcript-only meeting with a Generate summary button.
pub const NO_ENGINE: &str = "no_engine";
pub const FAILED: &str = "failed";
/// Too little speech to summarize (a test call, a cough): transcript only, and Generate summary says the same.
pub const TOO_SHORT: &str = "too_short";

/// What the worker reads from `settings.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// "local" | "ollama" | "byok"
    pub engine: String,
    /// "meeting" = the meeting's language, or an ISO code.
    pub language: String,
    /// Settings › Summary AI › Default meeting type (a `templates` id).
    pub template: String,
    /// Settings › Summary AI › Custom instructions.
    pub instructions: String,
    pub settings: Value,
}

impl Config {
    pub fn from_settings(v: Option<&Value>) -> Config {
        let v = v.cloned().unwrap_or(Value::Null);
        Config {
            engine: v["summaryEngine"].as_str().unwrap_or("local").to_string(),
            language: v["summaryLanguage"].as_str().filter(|l| !l.is_empty()).unwrap_or("meeting").to_string(),
            template: v["summaryTemplate"].as_str().filter(|t| templates::exists(t)).unwrap_or(templates::GENERAL).to_string(),
            instructions: v["summaryInstructions"].as_str().unwrap_or("").to_string(),
            settings: v,
        }
    }

    pub fn language_for(&self, meeting_language: &str) -> String {
        if self.language == "meeting" { meeting_language.to_string() } else { self.language.clone() }
    }

    /// How a meeting is written: its own meeting type when one was chosen for it, else the default, and the
    /// custom instructions.
    pub fn options_for(&self, meeting_template: Option<&str>) -> Options {
        let template = meeting_template.filter(|t| templates::exists(t)).unwrap_or(&self.template).to_string();
        Options { template, instructions: self.instructions.clone() }
    }
}

pub type EngineFactory = dyn Fn(&Config) -> Result<Box<dyn SummaryEngine>, EngineError> + Send + Sync;

pub struct Deps {
    pub storage: Arc<Mutex<Storage>>,
    pub changed: Arc<dyn Fn() + Send + Sync>,
    /// Builds the engine the settings ask for, or says why it can't.
    pub make_engine: Box<EngineFactory>,
    /// Called with a meeting id when its summary is ready (for the notification).
    pub summarized: Arc<dyn Fn(&str, &str) + Send + Sync>,
    /// Called with a meeting id and title when it ends without a summary (no engine, too short, the AI failed):
    /// the transcript is all there is, so the "Transcript ready" notification says the work is done.
    pub unsummarized: Arc<dyn Fn(&str, &str) + Send + Sync>,
    /// How a summary with these settings is timed on this computer (`eta::summary_profile`).
    pub profile: Box<dyn Fn(&Config) -> Profile + Send + Sync>,
}

/// Other work that needs the local AI (a follow-up draft): run on this thread between summaries, so only one
/// model is ever loaded.
pub type Job = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct Queue {
    /// Meeting id, and whether it came straight from the transcriber (nobody asked for it on screen).
    ids: VecDeque<(String, bool)>,
    /// Short jobs go before the next summary, each with its caller's tag (a follow-up's request id).
    jobs: VecDeque<(String, Job)>,
    current: Option<String>,
    /// The tag of the job running now.
    running_job: Option<String>,
}

/// What a job with a given tag waits for.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ahead {
    /// The meeting whose summary is being written.
    pub summary: Option<String>,
    /// Other jobs running or queued before it.
    pub jobs: usize,
}

impl Ahead {
    pub fn waiting(&self) -> bool {
        self.summary.is_some() || self.jobs > 0
    }
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
            .name("summarizer".into())
            .spawn(move || {
                crate::platform::background_priority();
                worker_loop(deps, queue, cancel)
            })
            .expect("summarizer thread");
        w
    }

    /// Someone asked for this summary (Generate summary, a model finished downloading, a restart).
    pub fn enqueue(&self, id: &str) {
        self.push(id, false);
    }

    /// The transcriber handed this meeting over by itself: when it ends without a summary, "Transcript ready"
    /// says the work is done. A summary someone asked for that fails shows its reason on screen instead.
    pub fn enqueue_auto(&self, id: &str) {
        self.push(id, true);
    }

    fn push(&self, id: &str, auto: bool) {
        let (lock, cv) = &*self.queue;
        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
        if q.current.as_deref() != Some(id) && !q.ids.iter().any(|(i, _)| i == id) {
            q.ids.push_back((id.to_string(), auto));
            cv.notify_one();
        }
    }

    /// Runs `job` on the summarizer thread once the summary in progress (if any) is done, before the next one.
    pub fn run_between(&self, tag: &str, job: Job) {
        let (lock, cv) = &*self.queue;
        lock.lock().unwrap_or_else(|e| e.into_inner()).jobs.push_back((tag.to_string(), job));
        cv.notify_one();
    }

    /// Takes the job tagged `tag` out of the queue if it hasn't started (a cancelled draft). True when it was there.
    pub fn drop_job(&self, tag: &str) -> bool {
        let mut q = self.queue.0.lock().unwrap_or_else(|e| e.into_inner());
        let before = q.jobs.len();
        q.jobs.retain(|(t, _)| t != tag);
        q.jobs.len() < before
    }

    /// What the job tagged `tag` waits for now (or would, queued now): the summary being written, the job running
    /// and the jobs queued before it.
    pub fn ahead_of(&self, tag: &str) -> Ahead {
        let q = self.queue.0.lock().unwrap_or_else(|e| e.into_inner());
        if q.running_job.as_deref() == Some(tag) {
            return Ahead::default();
        }
        let before = q.jobs.iter().position(|(t, _)| t == tag).unwrap_or(q.jobs.len());
        Ahead { summary: q.current.clone(), jobs: before + usize::from(q.running_job.is_some()) }
    }

    /// The meeting whose summary is being written right now.
    #[cfg(test)]
    pub fn current(&self) -> Option<String> {
        self.queue.0.lock().unwrap_or_else(|e| e.into_inner()).current.clone()
    }

    pub fn cancel_if_running(&self, id: &str) {
        let (lock, _) = &*self.queue;
        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
        q.ids.retain(|(i, _)| i != id);
        if q.current.as_deref() == Some(id) {
            self.cancel.store(true, Ordering::Relaxed);
        }
    }

    #[cfg(test)]
    pub fn idle(&self) -> bool {
        let q = self.queue.0.lock().unwrap_or_else(|e| e.into_inner());
        q.ids.is_empty() && q.current.is_none()
    }
}

fn worker_loop(deps: Deps, queue: Arc<(Mutex<Queue>, Condvar)>, cancel: Arc<AtomicBool>) {
    let (lock, cv) = &*queue;
    loop {
        enum Next {
            Job(Job),
            Summary(String, bool),
        }
        let next = {
            let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
            q.current = None;
            q.running_job = None;
            loop {
                if let Some((tag, job)) = q.jobs.pop_front() {
                    q.running_job = Some(tag);
                    break Next::Job(job);
                }
                if let Some((id, auto)) = q.ids.pop_front() {
                    q.current = Some(id.clone());
                    break Next::Summary(id, auto);
                }
                q = cv.wait(q).unwrap_or_else(|e| e.into_inner());
            }
        };
        // A bug in one job must not stop every later summary: a panic ends that job only. A job reports its own
        // failure (its result channel closes); a summary is left transcript-only with a message.
        let (id, auto) = match next {
            Next::Job(job) => {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
                continue;
            }
            Next::Summary(id, auto) => (id, auto),
        };
        cancel.store(false, Ordering::Relaxed);
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| process(&deps, &id, auto, &cancel))).is_err() {
            without_summary(&deps, &id, auto, FAILED, crate::i18n::tr("error.summary_internal"));
        }
        (deps.changed)();
    }
}

fn lock(storage: &Mutex<Storage>) -> std::sync::MutexGuard<'_, Storage> {
    storage.lock().unwrap_or_else(|e| e.into_inner())
}

/// Leaves the meeting transcript-only, saying why. `auto`: the job came from the transcriber (see `enqueue_auto`).
fn without_summary(deps: &Deps, id: &str, auto: bool, code: &str, message: String) {
    without_summary_detail(deps, id, auto, code, message, None)
}

fn without_summary_detail(deps: &Deps, id: &str, auto: bool, code: &str, message: String, detail: Option<String>) {
    match &detail {
        Some(d) => log::warn!("no summary for meeting {id} ({code}): {} | helper: {}", crate::logging::text(&message), crate::logging::text(d)),
        None if code == TOO_SHORT => log::info!("no summary for meeting {id}: too little speech"),
        None => log::warn!("no summary for meeting {id} ({code}): {}", crate::logging::text(&message)),
    }
    let mut title = String::new();
    let saved = lock(&deps.storage).update_meeting_file(id, |f| {
        f.status = MeetingStatus::Ready;
        f.progress = None;
        f.eta = None;
        f.summary_error = Some(ProcessingError { code: code.into(), message, detail });
        title = f.title.clone();
    });
    if saved.is_ok() && auto {
        (deps.unsummarized)(id, &title);
    }
}

fn process(deps: &Deps, id: &str, auto: bool, cancel: &AtomicBool) {
    let (meeting, config, speech) = {
        let s = lock(&deps.storage);
        let Ok(file) = s.read_meeting_file(id) else { return };
        if file.status != MeetingStatus::Summarizing {
            return;
        }
        let Ok(meeting) = s.get_meeting(id) else { return };
        // The summary's work grows with what was said; a meeting from before speech was measured uses its length.
        let speech = file.transcription.as_ref().map(|t| t.speech_ms).filter(|ms| *ms > 0).unwrap_or(file.duration_ms) as f64 / 1000.0;
        (meeting, Config::from_settings(s.settings_get().ok().flatten().as_ref()), speech)
    };

    // Checked before the engine, so a test call doesn't wait on (or fail for) a model it never needed.
    if too_short(&meeting.transcript) {
        return without_summary(deps, id, auto, TOO_SHORT, SummarizeError::TooShort.message());
    }
    log::info!("summarizing meeting {id} with the {} engine", config.engine);
    let profile = (deps.profile)(&config);
    let on_cpu = profile.device == Device::Cpu;
    let job = std::time::Instant::now();
    let estimate = {
        let s = lock(&deps.storage);
        let total = profile.total(s.speed_rate(&profile.key), speech);
        let _ = s.update_meeting_file(id, |f| set_eta(f, total, on_cpu));
        total
    };
    (deps.changed)();
    let mut engine = match (deps.make_engine)(&config) {
        Ok(e) => e,
        Err(EngineError::Unavailable(m)) => return without_summary(deps, id, auto, NO_ENGINE, m),
        Err(EngineError::Failed(m)) => return without_summary(deps, id, auto, FAILED, m),
        Err(EngineError::Cancelled) => return,
    };
    log::info!("summary engine ready in {:.1}s: {}", job.elapsed().as_secs_f32(), engine.label());

    let language = config.language_for(&meeting.language);
    let opts = config.options_for(meeting.template.as_deref());
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

    match summarize(&meeting, engine.as_mut(), &language, &opts, cancel, &mut report) {
        Ok(out) => {
            log::info!("summarized meeting {id} in {:.1}s ({} parts)", job.elapsed().as_secs_f32(), out.parts);
            let s = lock(&deps.storage);
            if s.save_summary(id, &out.summary).is_err() {
                drop(s);
                return without_summary(deps, id, auto, FAILED, crate::i18n::tr("error.save_summary"));
            }
            // The start-up cost is part of the estimate, not of the pace (but a job quicker than the start-up
            // estimate still counts half its time, so fast engines are learned too). A local summary that fell back from the
            // graphics chip to the processor has moved the GPU plan (gpu.json), so the profile worked out again
            // differs: that run's time belongs to neither device, and isn't learned.
            if (deps.profile)(&config).key == profile.key {
                let took = job.elapsed().as_secs_f64();
                s.learn_speed(&profile.key, (took - profile.overhead).max(took / 2.0), speech);
            }
            let mut title = String::new();
            let _ = s.update_meeting_file(id, |f| {
                f.status = MeetingStatus::Ready;
                f.progress = None;
                f.eta = None;
                f.summary_error = None;
                if let Some(t) = out.title.as_ref().filter(|_| f.title_is_placeholder) {
                    f.title = t.clone();
                    f.title_is_placeholder = false;
                }
                title = f.title.clone();
            });
            // The .md files carry the title, which may have just changed.
            let _ = s.regenerate_markdown(id);
            drop(s);
            (deps.summarized)(id, &title);
        }
        Err(SummarizeError::Engine(EngineError::Cancelled)) => log::info!("summary cancelled: meeting {id}"),
        Err(e @ SummarizeError::TooShort) => without_summary(deps, id, auto, TOO_SHORT, e.message()),
        Err(SummarizeError::Engine(EngineError::Unavailable(m))) => without_summary(deps, id, auto, NO_ENGINE, m),
        Err(e) => without_summary_detail(deps, id, auto, FAILED, e.message(), engine.failure_detail()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::model::TranscriptLine;
    use crate::summarizer::summarize::tests::{reply, Scripted};
    use serde_json::json;

    fn setup(make: Box<EngineFactory>) -> (tempfile::TempDir, Arc<Mutex<Storage>>, String, Worker, Arc<Mutex<Vec<String>>>) {
        setup_profile(make, Box::new(|c| eta::summary_profile(&c.engine, "test", false)))
    }

    fn setup_profile(make: Box<EngineFactory>, profile: Box<dyn Fn(&Config) -> Profile + Send + Sync>) -> (tempfile::TempDir, Arc<Mutex<Storage>>, String, Worker, Arc<Mutex<Vec<String>>>) {
        let dir = tempfile::tempdir().unwrap();
        let mut storage = Storage::new(dir.path().to_path_buf()).unwrap();
        let f = storage.create_meeting("zoom", "Zoom", chrono::DateTime::parse_from_rfc3339("2026-10-14T14:30:00-07:00").unwrap()).unwrap();
        let line = |id: &str, at: u64, speaker: &str, text: &str| TranscriptLine { id: id.into(), at, end: at + 4_000, speaker: speaker.into(), text: text.into(), edited: false };
        storage.save_transcript(&f.id, &[line("l0", 60_000, "me", "Onboarding first."), line("l1", 65_000, "them:0", "Agreed, pricing waits."), line("l2", 70_000, "me", "I'll send the plan Friday.")]).unwrap();
        storage.update_meeting_file(&f.id, |m| m.status = MeetingStatus::Summarizing).unwrap();
        storage.settings_set(json!({ "summaryEngine": "local" })).unwrap();
        let storage = Arc::new(Mutex::new(storage));
        let done = Arc::new(Mutex::new(vec![]));
        let d2 = done.clone();
        let d3 = done.clone();
        let worker = Worker::start(Deps {
            storage: storage.clone(),
            changed: Arc::new(|| {}),
            make_engine: make,
            summarized: Arc::new(move |_, t| d2.lock().unwrap().push(t.to_string())),
            // "Transcript ready" is told apart from "Summary ready" by a prefix.
            unsummarized: Arc::new(move |_, t| d3.lock().unwrap().push(format!("transcript: {t}"))),
            profile,
        });
        (dir, storage, f.id, worker, done)
    }

    fn wait(w: &Worker) {
        let t = Instant::now();
        std::thread::sleep(Duration::from_millis(20));
        while !w.idle() {
            assert!(t.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_meeting_gets_its_summary_and_title() {
        let (_d, storage, id, w, done) = setup(Box::new(|_| Ok(Box::new(Scripted { replies: vec![Ok(reply("Onboarding plan", "We chose onboarding."))], prompts: vec![], context: 16_384 }))));
        w.enqueue(&id);
        wait(&w);
        let m = storage.lock().unwrap().get_meeting(&id).unwrap();
        assert_eq!(m.item.status, MeetingStatus::Ready);
        assert_eq!(m.summary.unwrap().recap, "We chose onboarding.");
        assert_eq!(m.item.title, "Onboarding plan");
        assert_eq!(m.summary_error, None);
        assert_eq!(*done.lock().unwrap(), vec!["Onboarding plan".to_string()]);
        let f = storage.lock().unwrap().read_meeting_file(&id).unwrap();
        assert_eq!(f.eta, None, "the estimate goes once the summary is there");
    }

    #[test]
    fn a_fallback_to_the_processor_mid_summary_teaches_neither_speed() {
        // The engine "falls back" while it works, as the local AI does when the GPU helper fails.
        struct FallsBack(Arc<AtomicBool>);
        impl SummaryEngine for FallsBack {
            fn complete(&mut self, _p: &crate::summarizer::prompt::Prompt, _s: &str, _m: u32, _c: &AtomicBool) -> Result<String, EngineError> {
                self.0.store(false, Ordering::Relaxed);
                Ok(reply("Plan", "R."))
            }
            fn context_tokens(&self) -> usize {
                16_384
            }
            fn label(&self) -> String {
                "FallsBack".into()
            }
        }
        for falls_back in [true, false] {
            let gpu = Arc::new(AtomicBool::new(true));
            let g1 = gpu.clone();
            let (_d, storage, id, w, _) = setup_profile(
                Box::new(move |_| Ok(Box::new(FallsBack(if falls_back { g1.clone() } else { Arc::new(AtomicBool::new(true)) })))),
                Box::new(move |c| eta::summary_profile(&c.engine, "qwen3.5-4b", gpu.load(Ordering::Relaxed))),
            );
            storage.lock().unwrap().update_meeting_file(&id, |f| {
                f.transcription = Some(crate::storage::model::TranscriptionInfo { model: "whisper-turbo".into(), echo_lines_removed: 0, speech_ms: 600_000, took_ms: 1, finished_at: String::new(), gpu_fell_back: false, device: None });
            }).unwrap();
            w.enqueue(&id);
            wait(&w);
            let s = storage.lock().unwrap();
            assert!(s.get_meeting(&id).unwrap().summary.is_some());
            assert_eq!(s.speed_rate("summary:local:qwen3.5-4b:cpu"), None);
            assert_eq!(s.speed_rate("summary:local:qwen3.5-4b:gpu").is_some(), !falls_back, "falls back: {falls_back}");
        }
    }

    #[test]
    fn a_running_summary_has_an_estimate_from_the_profile() {
        // An engine that waits until the test has looked at the estimate.
        struct Slow(Arc<Mutex<bool>>);
        impl SummaryEngine for Slow {
            fn complete(&mut self, _p: &crate::summarizer::prompt::Prompt, _s: &str, _m: u32, _c: &AtomicBool) -> Result<String, EngineError> {
                while !*self.0.lock().unwrap() {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(reply("Plan", "R."))
            }
            fn context_tokens(&self) -> usize {
                16_384
            }
            fn label(&self) -> String {
                "Slow".into()
            }
        }
        let go = Arc::new(Mutex::new(false));
        let g2 = go.clone();
        let (_d, storage, id, w, _) = setup(Box::new(move |_| Ok(Box::new(Slow(g2.clone())))));
        storage.lock().unwrap().settings_set(json!({ "summaryEngine": "local", "localModel": "recommended" })).unwrap();
        w.enqueue(&id);
        let t = Instant::now();
        let eta = loop {
            if let Some(e) = storage.lock().unwrap().list_meetings(&Default::default()).unwrap()[0].eta.clone() {
                break e;
            }
            assert!(t.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(5));
        };
        // The test profile runs on the processor; the estimate is the start-up cost plus a little speech.
        assert!(eta.on_cpu);
        let finish = chrono::DateTime::parse_from_rfc3339(&eta.finish_at).unwrap();
        let left = (finish.with_timezone(&chrono::Local) - chrono::Local::now()).num_seconds();
        assert!((5..=60).contains(&left), "{left}");
        *go.lock().unwrap() = true;
        wait(&w);
        assert_eq!(storage.lock().unwrap().read_meeting_file(&id).unwrap().eta, None);
    }

    #[test]
    fn a_title_the_user_typed_is_kept() {
        let (_d, storage, id, w, _) = setup(Box::new(|_| Ok(Box::new(Scripted { replies: vec![Ok(reply("Onboarding plan", "R."))], prompts: vec![], context: 16_384 }))));
        storage.lock().unwrap().update_meeting_file(&id, |f| {
            f.title = "Dana 1:1".into();
            f.title_is_placeholder = false;
        }).unwrap();
        w.enqueue(&id);
        wait(&w);
        assert_eq!(storage.lock().unwrap().get_meeting(&id).unwrap().item.title, "Dana 1:1");
    }

    #[test]
    fn no_engine_leaves_a_transcript_only_meeting_with_the_reason() {
        let (_d, storage, id, w, done) = setup(Box::new(|_| Err(EngineError::Unavailable("Download the local AI model in Settings › Summary AI.".into()))));
        w.enqueue_auto(&id);
        wait(&w);
        let m = storage.lock().unwrap().get_meeting(&id).unwrap();
        assert_eq!(m.item.status, MeetingStatus::Ready);
        assert!(m.summary.is_none());
        assert_eq!(m.summary_error.unwrap().code, NO_ENGINE);
        assert!(m.item.eta.is_none());
        // No summary, but the work is done: "Transcript ready".
        let done = done.lock().unwrap();
        assert_eq!(done.len(), 1);
        assert!(done[0].starts_with("transcript: Call, Zoom"), "{done:?}");
    }

    #[test]
    fn a_failed_summary_someone_asked_for_posts_nothing() {
        let (_d, storage, id, w, done) = setup(Box::new(|_| Ok(Box::new(Dies))));
        w.enqueue(&id);
        wait(&w);
        assert_eq!(storage.lock().unwrap().get_meeting(&id).unwrap().summary_error.unwrap().code, FAILED);
        assert!(done.lock().unwrap().is_empty(), "the reason is on screen; no notification");
    }

    /// An engine that fails the way the local AI does when its helper dies.
    struct Dies;
    impl SummaryEngine for Dies {
        fn complete(&mut self, _p: &crate::summarizer::prompt::Prompt, _s: &str, _m: u32, _c: &AtomicBool) -> Result<String, EngineError> {
            Err(EngineError::Failed("The local AI stopped unexpectedly. Try again.".into()))
        }
        fn context_tokens(&self) -> usize {
            16_384
        }
        fn label(&self) -> String {
            "Dies".into()
        }
        fn failure_detail(&mut self) -> Option<String> {
            Some("local AI helper stopped: exit code -1073741510 (0xC000013A)".into())
        }
    }

    #[test]
    fn a_failure_keeps_the_engines_detail_beside_the_message() {
        let (_d, storage, id, w, _) = setup(Box::new(|_| Ok(Box::new(Dies))));
        w.enqueue(&id);
        wait(&w);
        let e = storage.lock().unwrap().get_meeting(&id).unwrap().summary_error.unwrap();
        assert_eq!(e.code, FAILED);
        assert_eq!(e.message, "The local AI stopped unexpectedly. Try again.");
        assert!(e.detail.unwrap().contains("0xC000013A"));
    }

    #[test]
    fn unusable_answers_are_reported_not_saved() {
        let (_d, storage, id, w, _) = setup(Box::new(|_| Ok(Box::new(Scripted { replies: vec![Ok("nope".into()), Ok("still nope".into())], prompts: vec![], context: 16_384 }))));
        w.enqueue(&id);
        wait(&w);
        let m = storage.lock().unwrap().get_meeting(&id).unwrap();
        assert!(m.summary.is_none());
        let e = m.summary_error.unwrap();
        assert_eq!(e.code, FAILED);
        assert!(e.message.contains("couldn't use"));
    }

    #[test]
    fn a_too_short_recording_is_saved_without_a_summary() {
        let (_d, storage, id, w, done) = setup(Box::new(|_| Ok(Box::new(Scripted { replies: vec![Ok(reply("Invented", "Invented."))], prompts: vec![], context: 16_384 }))));
        storage.lock().unwrap().save_transcript(&id, &[TranscriptLine { id: "l0".into(), at: 0, end: 2_000, speaker: "me".into(), text: "Can you hear me?".into(), edited: false }]).unwrap();
        w.enqueue(&id);
        wait(&w);
        let m = storage.lock().unwrap().get_meeting(&id).unwrap();
        assert_eq!(m.item.status, MeetingStatus::Ready);
        assert!(m.summary.is_none());
        assert_eq!(m.summary_error.unwrap().code, TOO_SHORT);
        assert!(m.item.title.starts_with("Call, Zoom"));
        assert!(done.lock().unwrap().iter().all(|d| d.starts_with("transcript: ")));
    }

    #[test]
    fn a_running_job_counts_as_ahead_of_the_next_one() {
        let (_d, _storage, _id, w, _) = setup(Box::new(|_| Ok(Box::new(Scripted { replies: vec![], prompts: vec![], context: 16_384 }))));
        let go = Arc::new(Mutex::new(false));
        let g = go.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        w.run_between("one", Box::new(move || {
            tx.send(()).unwrap();
            while !*g.lock().unwrap() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }));
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(w.ahead_of("one"), Ahead::default());
        let two = w.ahead_of("two");
        assert!(two.waiting() && two.summary.is_none() && two.jobs == 1);
        *go.lock().unwrap() = true;
    }

    #[test]
    fn a_panic_in_a_job_or_a_summary_leaves_the_worker_running() {
        struct Panics;
        impl SummaryEngine for Panics {
            fn complete(&mut self, _p: &crate::summarizer::prompt::Prompt, _s: &str, _m: u32, _c: &AtomicBool) -> Result<String, EngineError> {
                panic!("a bug in an engine");
            }
            fn context_tokens(&self) -> usize {
                16_384
            }
            fn label(&self) -> String {
                "Panics".into()
            }
        }
        let (_d, storage, id, w, _) = setup(Box::new(|_| Ok(Box::new(Panics))));
        w.run_between("a", Box::new(|| panic!("a bug in a draft")));
        w.enqueue(&id);
        wait(&w);
        let m = storage.lock().unwrap().get_meeting(&id).unwrap();
        assert_eq!(m.item.status, MeetingStatus::Ready);
        assert_eq!(m.summary_error.unwrap().code, FAILED);
        // Still working afterwards.
        let (tx, rx) = std::sync::mpsc::channel();
        w.run_between("b", Box::new(move || tx.send(()).unwrap()));
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn a_job_waits_for_the_summary_in_progress_and_never_runs_beside_it() {
        // An engine that holds the summary until told, and notes when it runs.
        struct Held(Arc<Mutex<bool>>, Arc<Mutex<Vec<&'static str>>>);
        impl SummaryEngine for Held {
            fn complete(&mut self, _p: &crate::summarizer::prompt::Prompt, _s: &str, _m: u32, _c: &AtomicBool) -> Result<String, EngineError> {
                self.1.lock().unwrap().push("summary starts");
                while !*self.0.lock().unwrap() {
                    std::thread::sleep(Duration::from_millis(5));
                }
                self.1.lock().unwrap().push("summary ends");
                Ok(reply("Plan", "R."))
            }
            fn context_tokens(&self) -> usize {
                16_384
            }
            fn label(&self) -> String {
                "Held".into()
            }
        }
        let go = Arc::new(Mutex::new(false));
        let log: Arc<Mutex<Vec<&'static str>>> = Arc::default();
        let (g, l) = (go.clone(), log.clone());
        let (_d, _storage, id, w, _) = setup(Box::new(move |_| Ok(Box::new(Held(g.clone(), l.clone())))));
        w.enqueue(&id);
        let t = Instant::now();
        while log.lock().unwrap().is_empty() {
            assert!(t.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(w.current().as_deref(), Some(id.as_str()));
        let l = log.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        w.run_between("first", Box::new(move || {
            l.lock().unwrap().push("job");
            tx.send(()).unwrap();
        }));
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(*log.lock().unwrap(), vec!["summary starts"], "the job waits");
        // Each job knows what is ahead of it: the summary, and for the second, the first job too.
        assert_eq!(w.ahead_of("first"), Ahead { summary: Some(id.clone()), jobs: 0 });
        assert_eq!(w.ahead_of("second"), Ahead { summary: Some(id.clone()), jobs: 1 });
        // A queued job can be taken back before it starts.
        w.run_between("dropped", Box::new(|| panic!("never runs")));
        assert!(w.drop_job("dropped") && !w.drop_job("dropped"));
        *go.lock().unwrap() = true;
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(*log.lock().unwrap(), vec!["summary starts", "summary ends", "job"]);
        wait(&w);
        assert_eq!(w.current(), None);
    }

    #[test]
    fn the_meetings_own_type_wins_over_the_default() {
        let c = Config::from_settings(Some(&json!({ "summaryTemplate": "standup", "summaryInstructions": "Mention budgets." })));
        assert_eq!(c.options_for(None), Options { template: "standup".into(), instructions: "Mention budgets.".into() });
        assert_eq!(c.options_for(Some("interview")).template, "interview");
        assert_eq!(c.options_for(Some("from_the_future")).template, "standup");
        let d = Config::from_settings(Some(&json!({ "summaryTemplate": "nope" })));
        assert_eq!(d.options_for(None), Options { template: "general".into(), instructions: String::new() });
    }

    #[test]
    fn a_new_summary_replaces_the_old_ones_edits_and_is_written_as_the_chosen_type() {
        let (_d, storage, id, w, _) = setup(Box::new(|_| Ok(Box::new(Scripted { replies: vec![Ok(reply("Plan", "Second."))], prompts: vec![], context: 16_384 }))));
        {
            let s = storage.lock().unwrap();
            let mut first = crate::storage::store::tests::summary("First.");
            first.action_items[0].done = true;
            s.save_summary(&id, &first).unwrap();
            let mut edited = first.clone();
            edited.recap = "Edited.".into();
            s.update_summary(&id, &edited).unwrap();
            assert!(s.get_meeting(&id).unwrap().summary.unwrap().edited);
            s.update_meeting_file(&id, |f| {
                f.template = Some("interview".into());
                f.status = MeetingStatus::Summarizing;
            })
            .unwrap();
        }
        w.enqueue(&id);
        wait(&w);
        let s = storage.lock().unwrap().get_meeting(&id).unwrap().summary.unwrap();
        assert_eq!((s.recap.as_str(), s.edited, s.template.as_deref()), ("Second.", false, Some("interview")));
        assert!(s.action_items.iter().all(|a| !a.done));
    }

    #[test]
    fn summary_language_follows_the_setting() {
        let c = Config::from_settings(Some(&json!({ "summaryLanguage": "es" })));
        assert_eq!(c.language_for("en"), "es");
        let m = Config::from_settings(Some(&json!({})));
        assert_eq!((m.engine.as_str(), m.language_for("tl").as_str()), ("local", "tl"));
    }
}
