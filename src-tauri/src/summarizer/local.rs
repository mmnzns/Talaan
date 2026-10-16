//! The app's side of the local AI helper (`talaan-llm`): starts it when needed, sends one request at a time,
//! streams progress, and turns a crash into a plain error instead of taking Talaan down (saying "out of memory"
//! only when that is what happened).

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::collections::VecDeque;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use talaan_llm_protocol::{encode, Completion, Embedding, Event, Phase, Reply, Request};

pub const HELPER: &str = "talaan-llm";

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

struct Running {
    child: Child,
    stdin: ChildStdin,
    events: Receiver<Event>,
    /// What the helper printed to stderr that looks like an error (see `StderrTail`).
    stderr: Arc<Mutex<StderrTail>>,
    stderr_reader: Option<JoinHandle<()>>,
}

pub struct LocalAi {
    exe: PathBuf,
    running: Option<Running>,
    /// Why the helper last stopped unexpectedly (exit status and its error lines), for debugging. Never shown as
    /// the message, never transcript text.
    last_detail: Option<String>,
}

/// The last ~2 KB of the helper's stderr, keeping only lines that look like errors. llama.cpp's own logging is
/// turned off in the helper, so stderr only carries Rust panics, allocation failures and ggml asserts; filtering
/// to those lines (each capped) means prompt or transcript text never ends up here even if something printed it.
#[derive(Default)]
pub struct StderrTail {
    lines: VecDeque<String>,
    bytes: usize,
    /// After a panic header, the panic's message (which can quote any text) until Rust's own `note:` /
    /// `stack backtrace:` line or the next panic. Release builds abort on panic, so nothing useful is lost.
    in_panic_message: bool,
}

const TAIL_BYTES: usize = 2048;
const LINE_BYTES: usize = 200;

impl StderrTail {
    pub fn push(&mut self, line: &str) {
        let line = line.trim();
        let header = panic_header(line);
        if self.in_panic_message && header.is_none() {
            if line.starts_with("note: ") || line.starts_with("stack backtrace:") {
                self.in_panic_message = false;
            }
            return;
        }
        let line = match header {
            Some(h) => {
                self.in_panic_message = true;
                h
            }
            None if is_error_line(line) => line,
            None => return,
        };
        let mut cut = line.len().min(LINE_BYTES);
        while !line.is_char_boundary(cut) {
            cut -= 1;
        }
        let line = line[..cut].to_string();
        self.bytes += line.len() + 1;
        self.lines.push_back(line);
        while self.bytes > TAIL_BYTES {
            match self.lines.pop_front() {
                Some(l) => self.bytes -= l.len() + 1,
                None => break,
            }
        }
    }

    pub fn text(&self) -> String {
        self.lines.iter().cloned().collect::<Vec<_>>().join("\n")
    }
}

/// A Rust panic header, cut to `thread '<name>' panicked at <location>`: the message (on the next line since Rust
/// 1.73, quoted in the header before that) is never kept.
fn panic_header(line: &str) -> Option<&str> {
    if !line.starts_with("thread '") {
        return None;
    }
    let at = line.find("' panicked at")? + "' panicked at".len();
    let rest = &line[at..];
    let location = rest.trim_start();
    if location.starts_with('\'') || location.starts_with('"') {
        return Some(&line[..at]); // old format: the message comes first
    }
    let end = location.find(char::is_whitespace).unwrap_or(location.len());
    Some(&line[..at + (rest.len() - location.len()) + end])
}

/// Lines worth keeping (besides a panic header): a ggml / llama.cpp diagnostic, Rust's allocation failure, or the
/// C++ runtime's uncaught `std::bad_alloc`. Each starts with text of their own, never anything user-supplied first.
fn is_error_line(line: &str) -> bool {
    has_known_prefix(line) || line.contains("GGML_ASSERT") || line.contains("GGML_ABORT")
}

fn has_known_prefix(line: &str) -> bool {
    line.starts_with("ggml_")
        || line.starts_with("llama_")
        || line.starts_with("GGML_")
        || line.starts_with("memory allocation of")
        || line.starts_with(BAD_ALLOC)
}

const BAD_ALLOC: &str = "terminate called after throwing an instance of 'std::bad_alloc'";

/// Memory markers count only on a line with a known prefix, so text that merely mentions memory (a panic message
/// quoting the transcript, say) can neither be kept nor blamed on memory.
fn says_out_of_memory(line: &str) -> bool {
    let line = line.trim();
    if !has_known_prefix(line) {
        return false;
    }
    let lower = line.to_ascii_lowercase();
    MEMORY_MARKERS.iter().any(|m| lower.contains(m))
}

const MEMORY_MARKERS: &[&str] = &["memory allocation of", "out of memory", "failed to allocate", "bad_alloc", "cannot allocate", "unable to allocate", "insufficient memory"];

/// Windows' STATUS_NO_MEMORY, the exit code of a process that died because an allocation failed.
#[cfg(windows)]
const STATUS_NO_MEMORY: u32 = 0xC000_0017;

/// Whether the helper died for lack of memory: it said so on stderr (Rust's "memory allocation of N bytes failed"
/// before aborting, ggml's "failed to allocate"), Windows ended it with STATUS_NO_MEMORY, or on macOS/Linux it was
/// killed with SIGKILL (what the system does to a process using too much memory; Talaan's own stop comes later).
pub fn ran_out_of_memory(status: Option<std::process::ExitStatus>, stderr: &str) -> bool {
    if stderr.lines().any(says_out_of_memory) {
        return true;
    }
    let Some(status) = status else { return false };
    #[cfg(windows)]
    {
        status.code().is_some_and(|c| c as u32 == STATUS_NO_MEMORY)
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal() == Some(9)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = status;
        false
    }
}

fn describe_status(status: Option<std::process::ExitStatus>) -> String {
    match status {
        None => "still running, stopped by Talaan".into(),
        Some(s) => match s.code() {
            #[cfg(windows)]
            Some(c) => format!("exit code {c} (0x{:08X})", c as u32),
            #[cfg(not(windows))]
            Some(c) => format!("exit code {c}"),
            None => format!("{s}"),
        },
    }
}

impl LocalAi {
    pub fn new(exe: &Path) -> Self {
        LocalAi { exe: exe.to_path_buf(), running: None, last_detail: None }
    }

    /// Why the helper last stopped unexpectedly (taken, so it's reported once).
    pub fn take_detail(&mut self) -> Option<String> {
        self.last_detail.take()
    }

    fn start(&mut self) -> Result<&mut Running, String> {
        let alive = self.running.as_mut().is_some_and(|r| matches!(r.child.try_wait(), Ok(None)));
        if !alive {
            self.running = None;
            let mut cmd = Command::new(&self.exe);
            cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
            hide_console(&mut cmd);
            let mut child = cmd
                .spawn()
                .map_err(|e| format!("couldn't start the local AI ({}): {e}", self.exe.display()))?;
            let stdin = child.stdin.take().expect("piped");
            let stdout = child.stdout.take().expect("piped");
            let stderr_pipe = child.stderr.take().expect("piped");
            let (tx, events) = mpsc::channel();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if let Ok(ev) = serde_json::from_str::<Event>(&line) {
                        if tx.send(ev).is_err() {
                            break;
                        }
                    }
                }
            });
            let stderr: Arc<Mutex<StderrTail>> = Arc::default();
            let tail = stderr.clone();
            // Always drained, so a chatty helper can never block on a full pipe.
            let stderr_reader = std::thread::spawn(move || {
                let mut reader = BufReader::new(stderr_pipe);
                let mut buf = Vec::new();
                while reader.read_until(b'\n', &mut buf).map(|n| n > 0).unwrap_or(false) {
                    tail.lock().unwrap_or_else(|e| e.into_inner()).push(&String::from_utf8_lossy(&buf));
                    buf.clear();
                }
            });
            let mut r = Running { child, stdin, events, stderr, stderr_reader: Some(stderr_reader) };
            send(&mut r.stdin, &Request::Ping)?;
            // Generous: the first launch after an install or update waits on macOS checking the new binary.
            match r.events.recv_timeout(Duration::from_secs(60)) {
                Ok(Event::Ready) => {}
                Ok(Event::Error { message }) => return Err(message),
                Err(RecvTimeoutError::Disconnected) => {
                    self.running = Some(r);
                    let oom = self.died();
                    return Err(crate::i18n::tr(if oom { "local_ai.out_of_memory" } else { "local_ai.no_start" }));
                }
                _ => return Err(crate::i18n::tr("local_ai.no_start")),
            }
            self.running = Some(r);
        }
        Ok(self.running.as_mut().expect("started above"))
    }

    /// The helper closed its output without answering: wait briefly for it to exit, keep its exit status and error
    /// lines in `last_detail`, and say whether it ran out of memory.
    fn died(&mut self) -> bool {
        let Some(mut r) = self.running.take() else { return false };
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut status = None;
        while Instant::now() < deadline {
            match r.child.try_wait() {
                Ok(Some(s)) => {
                    status = Some(s);
                    break;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(_) => break,
            }
        }
        if status.is_none() {
            let _ = r.child.kill();
            let _ = r.child.wait();
        }
        // The pipe closes once the process is gone, so the reader finishes.
        if let Some(h) = r.stderr_reader.take() {
            let _ = h.join();
        }
        let tail = r.stderr.lock().unwrap_or_else(|e| e.into_inner()).text();
        let oom = ran_out_of_memory(status, &tail);
        let mut detail = format!("local AI helper stopped: {}", describe_status(status));
        if !tail.is_empty() {
            detail.push('\n');
            detail.push_str(&tail);
        }
        self.last_detail = Some(detail);
        oom
    }

    /// Runs one completion. `progress` gets (phase, done, total). Cancelling stops the helper.
    pub fn complete(&mut self, c: &Completion, cancel: &AtomicBool, progress: &mut dyn FnMut(Phase, u32, u32)) -> Result<Reply, String> {
        self.last_detail = None;
        let r = self.start()?;
        send(&mut r.stdin, &Request::Complete(c.clone()))?;
        let started = Instant::now();
        loop {
            if cancel.load(Ordering::Relaxed) {
                self.stop();
                return Err("cancelled".into());
            }
            let r = self.running.as_mut().expect("running");
            match r.events.recv_timeout(Duration::from_millis(250)) {
                Ok(Event::Progress { phase, done, total }) => progress(phase, done, total),
                Ok(Event::Done(reply)) => return Ok(reply),
                Ok(Event::Error { message }) => return Err(message),
                Ok(Event::Ready | Event::Embedded { .. }) => {}
                Err(RecvTimeoutError::Timeout) => {
                    // A summary should never take this long; something is stuck.
                    if started.elapsed() > Duration::from_secs(30 * 60) {
                        self.stop();
                        return Err(crate::i18n::tr("local_ai.too_slow"));
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let oom = self.died();
                    return Err(crate::i18n::tr(if oom { "local_ai.out_of_memory" } else { "local_ai.crashed" }));
                }
            }
        }
    }

    /// Embeds texts with an embedding model (one vector per text, L2-normalized).
    pub fn embed(&mut self, model_path: &Path, texts: Vec<String>, cancel: &AtomicBool) -> Result<Vec<Vec<f32>>, String> {
        let r = self.start()?;
        send(&mut r.stdin, &Request::Embed(Embedding { model_path: model_path.to_string_lossy().into_owned(), texts }))?;
        loop {
            if cancel.load(Ordering::Relaxed) {
                self.stop();
                return Err("cancelled".into());
            }
            let r = self.running.as_mut().expect("running");
            match r.events.recv_timeout(Duration::from_millis(250)) {
                Ok(Event::Embedded { vectors }) => return Ok(vectors),
                Ok(Event::Error { message }) => return Err(message),
                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.died();
                    return Err("the search model stopped unexpectedly".into());
                }
            }
        }
    }

    /// Ends the helper now (frees all of the model's memory).
    pub fn stop(&mut self) {
        if let Some(mut r) = self.running.take() {
            let _ = r.child.kill();
            let _ = r.child.wait();
        }
    }
}

impl Drop for LocalAi {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Windows: the helper is a console program, so starting it from Talaan (a windowed app with no console) would
/// open a black console window for every summary, and closing that window kills the summary. `CREATE_NO_WINDOW`
/// runs it without one; the piped stdin/stdout/stderr are unaffected. Nothing to do elsewhere.
#[cfg(windows)]
pub fn hide_console(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub fn hide_console(_cmd: &mut Command) {}

fn send(stdin: &mut ChildStdin, req: &Request) -> Result<(), String> {
    writeln!(stdin, "{}", encode(req)).and_then(|_| stdin.flush()).map_err(|e| format!("couldn't talk to the local AI: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_helper_is_a_clear_error() {
        let mut ai = LocalAi::new(Path::new("/nonexistent/talaan-llm"));
        let c = Completion { model_path: "x".into(), n_ctx: 512, system: "".into(), user: "".into(), json_schema: None, max_tokens: 1, temperature: 0.0 };
        let e = ai.complete(&c, &AtomicBool::new(false), &mut |_, _, _| {}).unwrap_err();
        assert!(e.starts_with("couldn't start the local AI"), "{e}");
    }

    /// A fake helper: answers the ping, then on the first real request prints `stderr` lines and exits with `code`.
    fn fake_helper(dir: &Path, stderr: &[&str], code: i32) -> PathBuf {
        #[cfg(unix)]
        {
            let fake = dir.join("fake-llm");
            let lines: String = stderr.iter().map(|l| format!("echo '{l}' 1>&2
")).collect();
            std::fs::write(&fake, format!("#!/bin/sh
read line
echo '{{\"event\":\"ready\"}}'
read line
{lines}exit {code}
")).unwrap();
            std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
            fake
        }
        #[cfg(windows)]
        {
            let fake = dir.join("fake-llm.cmd");
            let lines: String = stderr.iter().map(|l| format!("echo {l} 1>&2
")).collect();
            std::fs::write(&fake, format!("@echo off
set /p line=
echo {{\"event\":\"ready\"}}
set /p line=
{lines}exit /b {code}
")).unwrap();
            fake
        }
    }

    fn completion() -> Completion {
        Completion { model_path: "x".into(), n_ctx: 512, system: "".into(), user: "".into(), json_schema: None, max_tokens: 1, temperature: 0.0 }
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

    #[test]
    fn memory_is_blamed_only_on_evidence() {
        assert!(!ran_out_of_memory(None, ""));
        assert!(!ran_out_of_memory(None, "thread 'main' panicked at src/main.rs:1:1:"));
        assert!(ran_out_of_memory(None, "ggml_backend_cpu_buffer_type_alloc_buffer: failed to allocate buffer of size 4294967296"));
        assert!(ran_out_of_memory(None, "terminate called after throwing an instance of 'std::bad_alloc'"));
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            assert!(ran_out_of_memory(Some(std::process::ExitStatus::from_raw(0xC000_0017)), ""));
            assert!(!ran_out_of_memory(Some(std::process::ExitStatus::from_raw(1)), ""));
            // A closed console window (CTRL_CLOSE_EVENT) is not memory.
            assert!(!ran_out_of_memory(Some(std::process::ExitStatus::from_raw(0xC000_013A)), ""));
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert!(ran_out_of_memory(Some(std::process::ExitStatus::from_raw(9)), "")); // SIGKILL
            assert!(!ran_out_of_memory(Some(std::process::ExitStatus::from_raw(6)), "")); // SIGABRT
        }
    }

    #[test]
    fn the_stderr_tail_is_bounded_and_keeps_only_error_lines() {
        let mut t = StderrTail::default();
        t.push("Me: here is the roadmap we discussed
");
        assert_eq!(t.text(), "");
        for i in 0..500 {
            t.push(&format!("ggml_assert line {i}: {}", "x".repeat(400)));
        }
        let text = t.text();
        assert!(text.len() <= TAIL_BYTES, "{}", text.len());
        assert!(text.contains("line 499"), "keeps the latest lines");
        assert!(text.lines().all(|l| l.len() <= LINE_BYTES));
    }

    /// A panic's message can quote anything (the transcript, the prompt): the header's location is kept, the
    /// message never is, even when it looks like a ggml line or mentions memory.
    #[test]
    fn a_panic_message_is_never_kept_and_never_blamed_on_memory() {
        let mut t = StderrTail::default();
        t.push("thread 'main' panicked at llm/src/main.rs:10:5:");
        t.push("Sam: we ran out of memory on the old laptops, failed to allocate budget");
        t.push("ggml_ fake line quoting the transcript: out of memory");
        t.push("note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace");
        t.push("ggml_backend_cpu_buffer_type_alloc_buffer: allocated buffer");
        let text = t.text();
        assert_eq!(text, "thread 'main' panicked at llm/src/main.rs:10:5:\nggml_backend_cpu_buffer_type_alloc_buffer: allocated buffer");
        assert!(!ran_out_of_memory(None, &text));

        // Before Rust 1.73 the message was quoted in the header itself.
        let mut t = StderrTail::default();
        t.push("thread 'main' panicked at 'Agreed, pricing waits until Q1', llm/src/main.rs:10:5");
        assert_eq!(t.text(), "thread 'main' panicked at");
    }

    #[test]
    fn memory_markers_count_only_on_known_lines() {
        let mut t = StderrTail::default();
        t.push("Me: the server said out of memory again");
        t.push("We cannot allocate more people to this");
        assert_eq!(t.text(), "");
        assert!(!ran_out_of_memory(None, "Me: the server said out of memory again"));
        assert!(!ran_out_of_memory(None, "C:/src/ggml.c:12: GGML_ASSERT(ok) failed: out of memory in the meeting"));
        assert!(ran_out_of_memory(None, "llama_kv_cache_init: failed to allocate buffer for kv cache"));
        assert!(ran_out_of_memory(None, "thread 'main' panicked at x.rs:1:1:\nmemory allocation of 1024 bytes failed"));
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
            let c = Completion { model_path: path.to_string_lossy().into(), n_ctx: 8192, system: prompt.system.clone(), user: prompt.user.clone(), json_schema: Some(SCHEMA.into()), max_tokens: 1500, temperature: 0.2 };
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
