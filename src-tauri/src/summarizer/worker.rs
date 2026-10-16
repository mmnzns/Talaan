//! The background summarizer: one thread, one meeting at a time, queue rebuilt from `summarizing` at launch.
//! A meeting enters `summarizing` and leaves `ready`, with a summary or with `summary_error` saying why not.

use super::summarize::{summarize, too_short, EngineError, SummarizeError, SummaryEngine};
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
    pub settings: Value,
}

impl Config {
    pub fn from_settings(v: Option<&Value>) -> Config {
        let v = v.cloned().unwrap_or(Value::Null);
        Config {
            engine: v["summaryEngine"].as_str().unwrap_or("local").to_string(),
            language: v["summaryLanguage"].as_str().filter(|l| !l.is_empty()).unwrap_or("meeting").to_string(),
            settings: v,
        }
    }

    pub fn language_for(&self, meeting_language: &str) -> String {
        if self.language == "meeting" { meeting_language.to_string() } else { self.language.clone() }
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
            .name("summarizer".into())
            .spawn(move || {
                crate::platform::background_priority();
                worker_loop(deps, queue, cancel)
            })
            .expect("summarizer thread");
        w
    }

    pub fn enqueue(&self, id: &str) {
        let (lock, cv) = &*self.queue;
        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
        if q.current.as_deref() != Some(id) && !q.ids.iter().any(|i| i == id) {
            q.ids.push_back(id.to_string());
            cv.notify_one();
        }
    }

    pub fn cancel_if_running(&self, id: &str) {
        let (lock, _) = &*self.queue;
        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
        q.ids.retain(|i| i != id);
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
        let id = {
            let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
            q.current = None;
            loop {
                if let Some(id) = q.ids.pop_front() {
                    q.current = Some(id.clone());
                    break id;
                }
                q = cv.wait(q).unwrap_or_else(|e| e.into_inner());
            }
        };
        cancel.store(false, Ordering::Relaxed);
        process(&deps, &id, &cancel);
        (deps.changed)();
    }
}

fn lock(storage: &Mutex<Storage>) -> std::sync::MutexGuard<'_, Storage> {
    storage.lock().unwrap_or_else(|e| e.into_inner())
}

/// Leaves the meeting transcript-only, saying why.
fn without_summary(deps: &Deps, id: &str, code: &str, message: String) {
    without_summary_detail(deps, id, code, message, None)
}

fn without_summary_detail(deps: &Deps, id: &str, code: &str, message: String, detail: Option<String>) {
    let mut title = String::new();
    let saved = lock(&deps.storage).update_meeting_file(id, |f| {
        f.status = MeetingStatus::Ready;
        f.progress = None;
        f.eta = None;
        f.summary_error = Some(ProcessingError { code: code.into(), message, detail });
        title = f.title.clone();
    });
    if saved.is_ok() {
        (deps.unsummarized)(id, &title);
    }
}

fn process(deps: &Deps, id: &str, cancel: &AtomicBool) {
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
        return without_summary(deps, id, TOO_SHORT, SummarizeError::TooShort.message());
    }
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
        Err(EngineError::Unavailable(m)) => return without_summary(deps, id, NO_ENGINE, m),
        Err(EngineError::Failed(m)) => return without_summary(deps, id, FAILED, m),
        Err(EngineError::Cancelled) => return,
    };

    let language = config.language_for(&meeting.language);
    let mut last_report = Instant::now() - Duration::from_secs(10);
    let mut first_report: Option<Instant> = None;
    let mut report = |p: f32| {
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

    match summarize(&meeting, engine.as_mut(), &language, cancel, &mut report) {
        Ok(out) => {
            let s = lock(&deps.storage);
            if s.save_summary(id, &out.summary).is_err() {
                drop(s);
                return without_summary(deps, id, FAILED, crate::i18n::tr("error.save_summary"));
            }
            // The start-up cost is part of the estimate, not of the pace.
            s.learn_speed(&profile.key, (job.elapsed().as_secs_f64() - profile.overhead).max(0.0), speech);
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
        Err(SummarizeError::Engine(EngineError::Cancelled)) => {}
        Err(e @ SummarizeError::TooShort) => without_summary(deps, id, TOO_SHORT, e.message()),
        Err(SummarizeError::Engine(EngineError::Unavailable(m))) => without_summary(deps, id, NO_ENGINE, m),
        Err(e) => without_summary_detail(deps, id, FAILED, e.message(), engine.failure_detail()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::model::TranscriptLine;
    use crate::summarizer::summarize::tests::{reply, Scripted};
    use serde_json::json;

    fn setup(make: Box<EngineFactory>) -> (tempfile::TempDir, Arc<Mutex<Storage>>, String, Worker, Arc<Mutex<Vec<String>>>) {
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
            profile: Box::new(|c| eta::summary_profile(&c.engine, "test", false)),
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
        w.enqueue(&id);
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
    fn summary_language_follows_the_setting() {
        let c = Config::from_settings(Some(&json!({ "summaryLanguage": "es" })));
        assert_eq!(c.language_for("en"), "es");
        let m = Config::from_settings(Some(&json!({})));
        assert_eq!((m.engine.as_str(), m.language_for("tl").as_str()), ("local", "tl"));
    }
}
