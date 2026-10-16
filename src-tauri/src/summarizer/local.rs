//! The app's side of the local AI helper (`talaan-llm`): starts it when needed, sends one request at a time,
//! streams progress, and turns a crash into a plain error instead of taking Talaan down (saying "out of memory"
//! only when that is what happened).
//!
//! On Windows a summary can first go to the GPU build (`talaan-llm-vulkan`, see `gpu.rs`). If that helper fails to
//! start, crashes, stalls, runs out of memory or returns an error, the same request is run again on the processor
//! helper; when the processor succeeds, the GPU failure is reported (`on_failure`, which writes gpu.json) and this
//! `LocalAi` stays on the processor.

use crate::helper::{Death, Process};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;
use std::time::{Duration, Instant};
use talaan_llm_protocol::{Backend, Completion, Embedding, Event, Phase, Reply, Request};

pub const HELPER: &str = "talaan-llm";
const WHO: &str = "local AI helper";

/// The helper's file name on this OS (`talaan-llm`, or `talaan-llm.exe` on Windows).
pub fn helper_file() -> String {
    format!("{HELPER}{}", std::env::consts::EXE_SUFFIX)
}

/// Where the helper lives: next to Talaan's own binary (in the app bundle, `Contents/MacOS/`; next to
/// `Talaan.exe` on Windows; in `target/<profile>/` during development).
pub fn helper_path() -> PathBuf {
    let name = helper_file();
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(&name))).unwrap_or_else(|| PathBuf::from(name))
}

/// A GPU build of the helper to try first.
pub struct GpuHelper {
    pub exe: PathBuf,
    pub env: Vec<(String, String)>,
    /// Called with what went wrong when the GPU failed and the processor then did the job.
    pub on_failure: Arc<dyn Fn(&str) + Send + Sync>,
    /// No word from the GPU helper for this long counts as a hang (a GPU reset can leave it stuck).
    pub stall: Duration,
}

/// No progress for this long from the GPU helper is a hang. Loading a 9B model from a slow disk takes well under it.
pub const GPU_STALL: Duration = Duration::from_secs(5 * 60);

struct Gpu {
    cfg: GpuHelper,
    proc: Option<Process<Event>>,
    /// The GPU failed or isn't there: the processor from now on.
    off: bool,
    backend: Option<Backend>,
}

pub struct LocalAi {
    exe: PathBuf,
    running: Option<Process<Event>>,
    gpu: Option<Gpu>,
    /// Why the helper last stopped unexpectedly (exit status and its error lines), for debugging. Never shown as
    /// the message, never transcript text.
    last_detail: Option<String>,
}

/// How a request ended, before it becomes words for the user.
enum RunError {
    Cancelled,
    /// The helper answered with an error.
    Said(String),
    /// Couldn't start it (spawn failed): the message names the exe.
    NoSpawn(String),
    /// It started but never said it was ready (`Death` when it died doing so).
    NoStart(Option<Death>),
    /// Over the total time limit.
    TooSlow,
    /// No event for longer than the stall limit (GPU only).
    Stalled,
    Died(Death),
    Io(String),
}

/// What happened when trying the GPU first.
enum GpuTry {
    Done(Reply),
    Cancelled,
    /// No GPU in that helper after all (it said `cpu`): nothing to remember.
    NoGpu,
    Failed(String),
}

impl LocalAi {
    pub fn new(exe: &Path) -> Self {
        LocalAi { exe: exe.to_path_buf(), running: None, gpu: None, last_detail: None }
    }

    /// Tries `gpu` first for completions, with `exe` (the processor helper) behind it.
    pub fn with_gpu(exe: &Path, gpu: GpuHelper) -> Self {
        LocalAi { exe: exe.to_path_buf(), running: None, gpu: Some(Gpu { cfg: gpu, proc: None, off: false, backend: None }), last_detail: None }
    }

    /// Why the helper last stopped unexpectedly (taken, so it's reported once).
    pub fn take_detail(&mut self) -> Option<String> {
        self.last_detail.take()
    }

    /// The GPU this LocalAi last ran on, if any.
    #[cfg(test)]
    fn gpu_backend(&self) -> Option<&Backend> {
        self.gpu.as_ref().filter(|g| !g.off).and_then(|g| g.backend.as_ref())
    }

    /// Runs one completion. `progress` gets (phase, done, total). Cancelling stops the helper.
    pub fn complete(&mut self, c: &Completion, cancel: &AtomicBool, progress: &mut dyn FnMut(Phase, u32, u32)) -> Result<Reply, String> {
        self.last_detail = None;
        let mut pending = None;
        if self.gpu.as_ref().is_some_and(|g| !g.off) {
            match self.try_gpu(c, cancel, progress) {
                GpuTry::Done(r) => return Ok(r),
                GpuTry::Cancelled => return Err("cancelled".into()),
                GpuTry::NoGpu => {}
                GpuTry::Failed(reason) => pending = Some(reason),
            }
        }
        let cpu = Completion { gpu: false, ..c.clone() };
        let result = self.run_cpu(&Request::Complete(cpu), cancel, progress, |ev| match ev {
            Event::Done(r) => Some(r),
            _ => None,
        });
        let result = result.map_err(|e| self.message(e));
        if result.is_ok() {
            if let (Some(reason), Some(g)) = (pending, &self.gpu) {
                (g.cfg.on_failure)(&reason);
            }
        }
        result
    }

    fn try_gpu(&mut self, c: &Completion, cancel: &AtomicBool, progress: &mut dyn FnMut(Phase, u32, u32)) -> GpuTry {
        let g = self.gpu.as_mut().expect("checked");
        let stall = g.cfg.stall;
        let mut proc = g.proc.take();
        let backend = match start(&mut proc, &g.cfg.exe, &g.cfg.env) {
            Ok(b) => b,
            Err(e) => {
                g.off = true;
                return GpuTry::Failed(describe(e));
            }
        };
        if let Some(b) = backend {
            if !b.on_gpu() {
                g.off = true;
                if let Some(p) = proc.take() {
                    p.kill();
                }
                return GpuTry::NoGpu;
            }
            g.backend = Some(b);
        }
        let mut p = proc.expect("started");
        let req = Request::Complete(Completion { gpu: true, ..c.clone() });
        match run(&mut p, &req, cancel, progress, Some(stall), |ev| match ev {
            Event::Done(r) => Some(r),
            _ => None,
        }) {
            Ok(r) => {
                g.proc = Some(p);
                GpuTry::Done(r)
            }
            Err(RunError::Cancelled) => {
                p.kill();
                GpuTry::Cancelled
            }
            Err(e) => {
                g.off = true;
                GpuTry::Failed(describe(finish(e, p)))
            }
        }
    }

    /// Sends a request to the processor helper (starting it if needed) and waits for the event `want` accepts.
    fn run_cpu<T>(&mut self, req: &Request, cancel: &AtomicBool, progress: &mut dyn FnMut(Phase, u32, u32), want: impl Fn(Event) -> Option<T>) -> Result<T, RunError> {
        let mut proc = self.running.take();
        start(&mut proc, &self.exe, &[])?;
        let mut p = proc.expect("started");
        match run(&mut p, req, cancel, progress, None, want) {
            Ok(t) => {
                self.running = Some(p);
                Ok(t)
            }
            // The helper is still fine after reporting an error (a prompt too long, a bad schema).
            Err(RunError::Said(m)) => {
                self.running = Some(p);
                Err(RunError::Said(m))
            }
            Err(e) => Err(finish(e, p)),
        }
    }

    /// Words for the user (and `last_detail` for the log).
    fn message(&mut self, e: RunError) -> String {
        use crate::i18n::tr;
        match e {
            RunError::Cancelled => "cancelled".into(),
            RunError::Said(m) | RunError::NoSpawn(m) | RunError::Io(m) => m,
            RunError::NoStart(death) => {
                let oom = death.as_ref().is_some_and(|d| d.out_of_memory);
                self.last_detail = death.map(|d| d.detail);
                tr(if oom { "local_ai.out_of_memory" } else { "local_ai.no_start" })
            }
            RunError::TooSlow | RunError::Stalled => tr("local_ai.too_slow"),
            RunError::Died(d) => {
                let oom = d.out_of_memory;
                self.last_detail = Some(d.detail);
                tr(if oom { "local_ai.out_of_memory" } else { "local_ai.crashed" })
            }
        }
    }

    /// Embeds texts with an embedding model (one vector per text, L2-normalized). Always the processor helper.
    pub fn embed(&mut self, model_path: &Path, texts: Vec<String>, cancel: &AtomicBool) -> Result<Vec<Vec<f32>>, String> {
        let req = Request::Embed(Embedding { model_path: model_path.to_string_lossy().into_owned(), texts, gpu: false });
        self.run_cpu(&req, cancel, &mut |_, _, _| {}, |ev| match ev {
            Event::Embedded { vectors } => Some(vectors),
            _ => None,
        })
        .map_err(|e| match e {
            RunError::Died(d) => {
                self.last_detail = Some(d.detail);
                "the search model stopped unexpectedly".into()
            }
            e => self.message(e),
        })
    }

    /// Ends the helpers now (frees all of the model's memory).
    pub fn stop(&mut self) {
        if let Some(p) = self.running.take() {
            p.kill();
        }
        if let Some(p) = self.gpu.as_mut().and_then(|g| g.proc.take()) {
            p.kill();
        }
    }
}

impl Drop for LocalAi {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Starts the helper in `slot` unless it's still running. Returns what it said it runs on when it was (re)started.
fn start(slot: &mut Option<Process<Event>>, exe: &Path, env: &[(String, String)]) -> Result<Option<Backend>, RunError> {
    if slot.as_mut().is_some_and(|p| p.alive()) {
        return Ok(None);
    }
    if let Some(p) = slot.take() {
        p.kill();
    }
    let mut p = Process::<Event>::spawn(exe, env).map_err(|e| RunError::NoSpawn(format!("couldn't start the local AI ({}): {e}", exe.display())))?;
    if let Err(e) = p.send(&Request::Ping) {
        p.kill();
        return Err(RunError::Io(format!("couldn't talk to the local AI: {e}")));
    }
    // Generous: the first launch after an install or update waits on macOS checking the new binary.
    match p.recv_timeout(Duration::from_secs(60)) {
        Ok(Event::Ready { backend }) => {
            *slot = Some(p);
            Ok(Some(backend))
        }
        Ok(Event::Error { message }) => {
            p.kill();
            Err(RunError::Said(message))
        }
        Err(RecvTimeoutError::Disconnected) => Err(RunError::NoStart(Some(p.died(WHO)))),
        _ => {
            p.kill();
            Err(RunError::NoStart(None))
        }
    }
}

/// Sends `req` and waits for the event `want` accepts, passing progress on. `stall`: the longest silence allowed.
fn run<T>(p: &mut Process<Event>, req: &Request, cancel: &AtomicBool, progress: &mut dyn FnMut(Phase, u32, u32), stall: Option<Duration>, want: impl Fn(Event) -> Option<T>) -> Result<T, RunError> {
    p.send(req).map_err(|e| RunError::Io(format!("couldn't talk to the local AI: {e}")))?;
    let started = Instant::now();
    let mut heard = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(RunError::Cancelled);
        }
        match p.recv_timeout(Duration::from_millis(250)) {
            Ok(Event::Error { message }) => return Err(RunError::Said(message)),
            Ok(Event::Progress { phase, done, total }) => {
                heard = Instant::now();
                progress(phase, done, total);
            }
            Ok(ev) => {
                heard = Instant::now();
                if let Some(t) = want(ev) {
                    return Ok(t);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                // A summary should never take this long; something is stuck.
                if started.elapsed() > Duration::from_secs(30 * 60) {
                    return Err(RunError::TooSlow);
                }
                if stall.is_some_and(|s| heard.elapsed() > s) {
                    return Err(RunError::Stalled);
                }
            }
            Err(RecvTimeoutError::Disconnected) => return Err(RunError::Died(Death { detail: String::new(), out_of_memory: false })),
        }
    }
}

/// After a failed request: a helper that died is described, any other is stopped.
fn finish(e: RunError, p: Process<Event>) -> RunError {
    match e {
        RunError::Died(_) => RunError::Died(p.died(WHO)),
        e => {
            p.kill();
            e
        }
    }
}

/// A GPU failure in words for gpu.json (English, technical).
fn describe(e: RunError) -> String {
    match e {
        RunError::Cancelled => "cancelled".into(),
        RunError::Said(m) | RunError::NoSpawn(m) | RunError::Io(m) => m,
        RunError::NoStart(Some(d)) | RunError::Died(d) => d.detail,
        RunError::NoStart(None) => "the GPU helper didn't start".into(),
        RunError::TooSlow => "the GPU helper took more than 30 minutes".into(),
        RunError::Stalled => "the GPU helper stopped answering".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helper::fake::{crash, reply, script};
    use std::sync::Mutex;

    #[test]
    fn a_missing_helper_is_a_clear_error() {
        let mut ai = LocalAi::new(Path::new("/nonexistent/talaan-llm"));
        let e = ai.complete(&completion(), &AtomicBool::new(false), &mut |_, _, _| {}).unwrap_err();
        assert!(e.starts_with("couldn't start the local AI"), "{e}");
    }

    const READY: &str = r#"{"event":"ready"}"#;
    const READY_GPU: &str = r#"{"event":"ready","backend":"vulkan","device":"NVIDIA GeForce RTX 5070 Ti","vram_mb":16303}"#;
    const DONE: &str = r#"{"event":"done","text":"FROM","prompt_tokens":1,"output_tokens":1,"seconds":0.1,"truncated":false}"#;

    fn done(from: &str) -> String {
        DONE.replace("FROM", from)
    }

    /// A fake helper: answers the ping, then on the first real request prints `stderr` lines and exits with `code`.
    fn fake_helper(dir: &Path, stderr: &[&str], code: i32) -> PathBuf {
        script(dir, "fake-llm", &[reply(&[READY]), crash(stderr, code)])
    }

    fn completion() -> Completion {
        Completion { model_path: "x".into(), n_ctx: 512, system: "".into(), user: "".into(), json_schema: None, max_tokens: 1, temperature: 0.0, gpu: true }
    }

    /// A helper that dies mid-request becomes a readable error, and it doesn't blame memory without a reason
    /// (Miguel's first Windows interview: the helper's console window was closed, 31 GB of RAM free).
    #[test]
    fn a_crashing_helper_is_reported_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let mut ai = LocalAi::new(&fake_helper(dir.path(), &["thread 'main' panicked at llm/src/main.rs:10:5:", "Agreed, pricing waits until Q1"], 1));
        let e = ai.complete(&completion(), &AtomicBool::new(false), &mut |_, _, _| {}).unwrap_err();
        assert!(e.contains("stopped unexpectedly"), "{e}");
        assert!(!e.contains("memory"), "{e}");
        let detail = ai.take_detail().unwrap();
        assert!(detail.contains("exit code 1"), "{detail}");
        assert!(detail.contains("panicked at llm/src/main.rs:10:5"), "{detail}");
        // Lines that aren't error markers (which could be prompt text) are never kept.
        assert!(!detail.contains("pricing"), "{detail}");
        assert_eq!(ai.take_detail(), None, "reported once");
    }

    #[test]
    fn a_helper_that_ran_out_of_memory_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let mut ai = LocalAi::new(&fake_helper(dir.path(), &["memory allocation of 8589934592 bytes failed"], 3));
        let e = ai.complete(&completion(), &AtomicBool::new(false), &mut |_, _, _| {}).unwrap_err();
        assert!(e.contains("ran out of memory"), "{e}");
        assert!(ai.take_detail().unwrap().contains("memory allocation of 8589934592 bytes failed"));
    }

    /// GPU then processor, with what was recorded.
    struct Rig {
        _dir: tempfile::TempDir,
        ai: LocalAi,
        recorded: Arc<Mutex<Vec<String>>>,
    }

    fn rig(gpu_steps: &[crate::helper::fake::Step], cpu_steps: &[crate::helper::fake::Step], stall: Duration) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let gpu_exe = script(dir.path(), "llm-vulkan", gpu_steps);
        let cpu_exe = script(dir.path(), "llm", cpu_steps);
        let recorded: Arc<Mutex<Vec<String>>> = Arc::default();
        let r = recorded.clone();
        let on_failure = Arc::new(move |reason: &str| r.lock().unwrap().push(reason.to_string()));
        let ai = LocalAi::with_gpu(&cpu_exe, GpuHelper { exe: gpu_exe, env: vec![("VK_LOADER_LAYERS_DISABLE".into(), "~implicit~".into())], on_failure, stall });
        Rig { _dir: dir, ai, recorded }
    }

    fn summarize(ai: &mut LocalAi) -> Result<String, String> {
        ai.complete(&completion(), &AtomicBool::new(false), &mut |_, _, _| {}).map(|r| r.text)
    }

    #[test]
    fn a_working_gpu_does_the_summary() {
        let gpu_done = done("gpu");
        let mut r = rig(&[reply(&[READY_GPU]), reply(&[&gpu_done]), reply(&[&gpu_done])], &[], GPU_STALL);
        assert_eq!(summarize(&mut r.ai).unwrap(), "gpu");
        assert_eq!(r.ai.gpu_backend().and_then(|b| b.device.as_deref()), Some("NVIDIA GeForce RTX 5070 Ti"));
        assert_eq!(summarize(&mut r.ai).unwrap(), "gpu", "the same helper process again");
        assert!(r.recorded.lock().unwrap().is_empty());
    }

    #[test]
    fn a_gpu_crash_is_redone_on_the_processor_and_remembered() {
        let cpu_done = done("cpu");
        let mut r = rig(
            &[reply(&[READY_GPU]), crash(&["ggml_vulkan: vk::Queue::submit: ErrorDeviceLost"], -1073740791)],
            &[reply(&[READY]), reply(&[&cpu_done]), reply(&[&cpu_done])],
            GPU_STALL,
        );
        assert_eq!(summarize(&mut r.ai).unwrap(), "cpu");
        let recorded = r.recorded.lock().unwrap().clone();
        assert_eq!(recorded.len(), 1);
        assert!(recorded[0].contains("ErrorDeviceLost") && recorded[0].contains("0xC0000409"), "{recorded:?}");
        assert_eq!(r.ai.take_detail(), None, "the summary itself didn't fail");
        assert_eq!(summarize(&mut r.ai).unwrap(), "cpu", "and it stays on the processor");
        assert_eq!(r.recorded.lock().unwrap().len(), 1);
        assert!(r.ai.gpu_backend().is_none());
    }

    #[test]
    fn a_gpu_that_wont_start_or_errors_falls_back_too() {
        let cpu_done = done("cpu");
        let mut r = rig(&[crash(&["ggml_vulkan: vk::createInstance: ErrorIncompatibleDriver"], 1)], &[reply(&[READY]), reply(&[&cpu_done])], GPU_STALL);
        assert_eq!(summarize(&mut r.ai).unwrap(), "cpu");
        assert!(r.recorded.lock().unwrap()[0].contains("ErrorIncompatibleDriver"));

        let oom = r#"{"event":"error","message":"not enough memory for the AI model: ErrorOutOfDeviceMemory"}"#;
        let mut r = rig(&[reply(&[READY_GPU]), reply(&[oom])], &[reply(&[READY]), reply(&[&cpu_done])], GPU_STALL);
        assert_eq!(summarize(&mut r.ai).unwrap(), "cpu");
        assert_eq!(*r.recorded.lock().unwrap(), ["not enough memory for the AI model: ErrorOutOfDeviceMemory"]);
    }

    #[test]
    fn a_stalled_gpu_is_given_up_on() {
        let cpu_done = done("cpu");
        // Ready, then silence on the request.
        let mut r = rig(&[reply(&[READY_GPU])], &[reply(&[READY]), reply(&[&cpu_done])], Duration::from_millis(400));
        let t = Instant::now();
        assert_eq!(summarize(&mut r.ai).unwrap(), "cpu");
        assert!(t.elapsed() < Duration::from_secs(10));
        assert_eq!(*r.recorded.lock().unwrap(), ["the GPU helper stopped answering"]);
    }

    #[test]
    fn an_error_the_processor_repeats_is_not_the_gpus_fault() {
        let too_long = r#"{"event":"error","message":"the transcript part is too long for the model (9000 tokens, context 512)"}"#;
        let mut r = rig(&[reply(&[READY_GPU]), reply(&[too_long])], &[reply(&[READY]), reply(&[too_long])], GPU_STALL);
        let e = summarize(&mut r.ai).unwrap_err();
        assert!(e.contains("too long"), "{e}");
        assert!(r.recorded.lock().unwrap().is_empty());
    }

    #[test]
    fn a_gpu_helper_without_a_gpu_is_quietly_skipped() {
        let cpu_done = done("cpu");
        let mut r = rig(&[reply(&[r#"{"event":"ready","backend":"cpu","vram_mb":0}"#])], &[reply(&[READY]), reply(&[&cpu_done])], GPU_STALL);
        assert_eq!(summarize(&mut r.ai).unwrap(), "cpu");
        assert!(r.recorded.lock().unwrap().is_empty());
    }

    #[test]
    fn cancelling_on_the_gpu_does_not_fall_back() {
        let mut r = rig(&[reply(&[READY_GPU])], &[], GPU_STALL);
        let e = r.ai.complete(&completion(), &AtomicBool::new(true), &mut |_, _, _| {}).unwrap_err();
        assert_eq!(e, "cancelled");
        assert!(r.recorded.lock().unwrap().is_empty());
    }

    /// Real model through the real helper. Build it first: `cargo build --release -p talaan-llm`, then
    /// `TALAAN_LLM_MODEL=<gguf> cargo test --release local_ai_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn local_ai_smoke() {
        let model = std::env::var("TALAAN_LLM_MODEL").unwrap();
        let exe = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join(helper_file());
        eprintln!("helper: {} exists={}", exe.display(), exe.exists());
        let mut ai = LocalAi::new(&exe);
        let c = Completion {
            model_path: model,
            n_ctx: 4096,
            system: "Reply with JSON only.".into(),
            user: "Give a title and a one-sentence recap for: 'We agreed to ship onboarding first and move pricing to Q1.'".into(),
            json_schema: Some(r#"{"type":"object","properties":{"title":{"type":"string"},"recap":{"type":"string"}},"required":["title","recap"]}"#.into()),
            max_tokens: 200,
            temperature: 0.0,
            gpu: true,
        };
        let r = ai.complete(&c, &AtomicBool::new(false), &mut |p, d, t| eprintln!("{p:?} {d}/{t}")).unwrap();
        eprintln!("{} ({} prompt, {} out, {:.1}s)", r.text, r.prompt_tokens, r.output_tokens, r.seconds);
        let v: serde_json::Value = serde_json::from_str(&r.text).unwrap();
        assert!(v["recap"].as_str().is_some());
    }
}

/// Bake-off: the real prompt, schema and parser on the planning-call fixture, for every GGUF in a folder.
/// `TALAAN_LLM_DIR=<folder> cargo test --release bakeoff -- --ignored --nocapture`
#[cfg(test)]
mod bakeoff {
    use super::*;
    use crate::storage::model::TranscriptLine;
    use crate::summarizer::parse::{parse_draft, to_summary};
    use crate::summarizer::prompt::{summary_prompt, tests::meeting, SCHEMA};

    #[test]
    #[ignore]
    fn bakeoff() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../fixtures/summary.planning-call.json")).unwrap();
        let lines: Vec<TranscriptLine> = serde_json::from_value(fixture["transcript"].clone()).unwrap();
        let mut m = meeting(lines.clone());
        m.speakers = serde_json::from_value(fixture["speakers"].clone()).unwrap();
        m.marks = serde_json::from_value(fixture["marks"].clone()).unwrap();
        m.item.duration_ms = fixture["durationMs"].as_u64().unwrap();
        let prompt = summary_prompt(&m, &lines, "en", None);
        let exe = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join(helper_file());
        let dir = std::env::var("TALAAN_LLM_DIR").unwrap();
        let mut models: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "gguf")).collect();
        models.sort();
        for path in models {
            let mut ai = LocalAi::new(&exe);
            let c = Completion { model_path: path.to_string_lossy().into(), n_ctx: 8192, system: prompt.system.clone(), user: prompt.user.clone(), json_schema: Some(SCHEMA.into()), max_tokens: 1500, temperature: 0.2, gpu: true };
            let mut read_s = 0.0;
            let t = Instant::now();
            let r = ai.complete(&c, &AtomicBool::new(false), &mut |p, d, total| {
                if p == Phase::Reading && d == total {
                    read_s = t.elapsed().as_secs_f32();
                }
            });
            let name = path.file_name().unwrap().to_string_lossy();
            match r {
                Err(e) => eprintln!("\n== {name}: ERROR {e}"),
                Ok(r) => {
                    eprintln!("\n== {name}: {:.1}s total (prompt read by {read_s:.1}s), {} prompt tok, {} out tok ({:.0} tok/s writing), truncated {}", r.seconds, r.prompt_tokens, r.output_tokens, r.output_tokens as f32 / (r.seconds - read_s).max(0.1), r.truncated);
                    match parse_draft(&r.text) {
                        Err(e) => eprintln!("   PARSE ERROR: {e}\n   {}", &r.text[..r.text.len().min(400)]),
                        Ok(d) => {
                            let s = to_summary(&d, &lines, "x");
                            eprintln!("   title: {}\n   recap: {}", d.title, s.recap);
                            for x in &s.decisions { eprintln!("   D [{:?}] {}", x.at.map(|a| a / 1000), x.text); }
                            for x in &s.action_items { eprintln!("   A [{:?}] {} | owner {:?} | due {:?}", x.at.map(|a| a / 1000), x.text, x.owner, x.due); }
                            for x in &s.open_questions { eprintln!("   Q [{:?}] {}", x.at.map(|a| a / 1000), x.text); }
                        }
                    }
                }
            }
        }
    }
}
