//! A helper process Talaan talks to over JSON lines (the local AI helper `talaan-llm`, and on Windows the GPU
//! helpers `talaan-llm-vulkan` and `talaan-stt-vulkan`): start it hidden, send requests, read events, keep the
//! error lines of its stderr, and say why it stopped (saying "out of memory" only when that is what happened).

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub struct Process<E> {
    child: Child,
    stdin: ChildStdin,
    events: Receiver<E>,
    /// What the helper printed to stderr that looks like an error (see `StderrTail`).
    stderr: Arc<Mutex<StderrTail>>,
    stderr_reader: Option<JoinHandle<()>>,
}

/// How a helper ended when it stopped answering.
pub struct Death {
    /// "local AI helper stopped: exit code 1 (0x00000001)" and its error lines.
    pub detail: String,
    pub out_of_memory: bool,
}

impl<E: DeserializeOwned + Send + 'static> Process<E> {
    /// Starts `exe` with these extra environment variables. Lines on stdout that aren't events are skipped.
    pub fn spawn(exe: &Path, env: &[(String, String)]) -> std::io::Result<Self> {
        let mut cmd = Command::new(exe);
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        for (k, v) in env {
            cmd.env(k, v);
        }
        hide_console(&mut cmd);
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().expect("piped");
        let stdout = child.stdout.take().expect("piped");
        let stderr_pipe = child.stderr.take().expect("piped");
        let (tx, events) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Ok(ev) = serde_json::from_str::<E>(&line) {
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
        Ok(Process { child, stdin, events, stderr, stderr_reader: Some(stderr_reader) })
    }

    pub fn send<R: Serialize>(&mut self, req: &R) -> std::io::Result<()> {
        writeln!(self.stdin, "{}", serde_json::to_string(req).expect("requests serialize")).and_then(|_| self.stdin.flush())
    }

    /// The next event; `Disconnected` once the helper closed its output (it died or exited).
    pub fn recv_timeout(&self, d: Duration) -> Result<E, RecvTimeoutError> {
        self.events.recv_timeout(d)
    }

    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// The helper stopped answering: wait briefly for it to exit (killing it if it doesn't), then describe it.
    /// `who` starts the detail ("local AI helper").
    pub fn died(mut self, who: &str) -> Death {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut status = None;
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(s)) => {
                    status = Some(s);
                    break;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(_) => break,
            }
        }
        if status.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        // The pipe closes once the process is gone, so the reader finishes.
        if let Some(h) = self.stderr_reader.take() {
            let _ = h.join();
        }
        let tail = self.stderr.lock().unwrap_or_else(|e| e.into_inner()).text();
        let out_of_memory = ran_out_of_memory(status, &tail);
        let mut detail = format!("{who} stopped: {}", describe_status(status));
        if !tail.is_empty() {
            detail.push('\n');
            detail.push_str(&tail);
        }
        Death { detail, out_of_memory }
    }

    /// Ends the helper now.
    pub fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The last ~2 KB of a helper's stderr, keeping only lines that look like errors. llama.cpp's and whisper.cpp's own
/// logging is turned off in the helpers, so stderr only carries Rust panics, allocation failures and ggml asserts;
/// filtering to those lines (each capped) means prompt or transcript text never ends up here even if something
/// printed it.
#[derive(Default)]
pub struct StderrTail {
    lines: VecDeque<String>,
    bytes: usize,
}

pub const TAIL_BYTES: usize = 2048;
pub const LINE_BYTES: usize = 200;

impl StderrTail {
    pub fn push(&mut self, line: &str) {
        let line = line.trim();
        if !is_error_line(line) {
            return;
        }
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

/// Lines worth keeping: a Rust panic header (it names only the source location), an allocation failure, or a
/// ggml / llama.cpp / whisper.cpp diagnostic.
fn is_error_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    (line.starts_with("thread '") && line.contains("panicked at"))
        || line.starts_with("ggml_")
        || line.starts_with("llama_")
        || line.starts_with("whisper_")
        || line.starts_with("GGML_")
        || line.contains("GGML_ASSERT")
        || line.contains("GGML_ABORT")
        || line.contains("vk::")
        || MEMORY_MARKERS.iter().any(|m| lower.contains(m))
}

const MEMORY_MARKERS: &[&str] = &["memory allocation of", "out of memory", "failed to allocate", "bad_alloc", "cannot allocate", "unable to allocate", "insufficient memory", "erroroutofdevicememory"];

/// Windows' STATUS_NO_MEMORY, the exit code of a process that died because an allocation failed.
#[cfg(windows)]
const STATUS_NO_MEMORY: u32 = 0xC000_0017;

/// Whether a helper died for lack of memory: it said so on stderr (Rust's "memory allocation of N bytes failed"
/// before aborting, ggml's "failed to allocate"), Windows ended it with STATUS_NO_MEMORY, or on macOS/Linux it was
/// killed with SIGKILL (what the system does to a process using too much memory; Talaan's own stop comes later).
pub fn ran_out_of_memory(status: Option<ExitStatus>, stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    if MEMORY_MARKERS.iter().any(|m| lower.contains(m)) {
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

fn describe_status(status: Option<ExitStatus>) -> String {
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

/// Windows: the helpers are console programs, so starting one from Talaan (a windowed app with no console) would
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

/// Test helpers: a fake helper script that plays a fixed conversation.
#[cfg(test)]
pub mod fake {
    use std::path::{Path, PathBuf};

    /// One step of a fake helper: read a request line, then print these stdout lines and these stderr lines.
    /// `exit` ends the script with that code after the step.
    pub struct Step<'a> {
        pub stdout: &'a [&'a str],
        pub stderr: &'a [&'a str],
        pub exit: Option<i32>,
    }

    pub fn reply<'a>(stdout: &'a [&'a str]) -> Step<'a> {
        Step { stdout, stderr: &[], exit: None }
    }

    pub fn crash<'a>(stderr: &'a [&'a str], code: i32) -> Step<'a> {
        Step { stdout: &[], stderr, exit: Some(code) }
    }

    /// Writes a script (`.cmd` on Windows, `sh` elsewhere) named `name` that answers each request in turn, then
    /// (if no step exited) waits for stdin to close. Lines must not contain `%`, `^`, `<`, `>`, `&` or `|` (cmd).
    pub fn script(dir: &Path, name: &str, steps: &[Step]) -> PathBuf {
        #[cfg(unix)]
        {
            let mut body = String::from("#!/bin/sh\n");
            for s in steps {
                body.push_str("read line\n");
                // Single-quoted for sh, with any ' written as '\'' (a panic header has quotes).
                for l in s.stdout {
                    body.push_str(&format!("echo '{}'\n", l.replace('\'', r"'\''")));
                }
                for l in s.stderr {
                    body.push_str(&format!("echo '{}' 1>&2\n", l.replace('\'', r"'\''")));
                }
                if let Some(c) = s.exit {
                    body.push_str(&format!("exit {c}\n"));
                }
            }
            body.push_str("while read line; do :; done\n");
            let path = dir.join(name);
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
            path
        }
        #[cfg(windows)]
        {
            let mut body = String::from("@echo off\r\n");
            for s in steps {
                body.push_str("set /p line=\r\n");
                for l in s.stdout {
                    body.push_str(&format!("echo {l}\r\n"));
                }
                for l in s.stderr {
                    body.push_str(&format!("echo {l} 1>&2\r\n"));
                }
                if let Some(c) = s.exit {
                    body.push_str(&format!("exit /b {c}\r\n"));
                }
            }
            // Wait for stdin to close (set /p returns at once on EOF, with an error level).
            body.push_str(":wait\r\nset line=\r\nset /p line=\r\nif errorlevel 1 exit /b 0\r\ngoto wait\r\n");
            let path = dir.join(format!("{name}.cmd"));
            std::fs::write(&path, body).unwrap();
            path
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_is_blamed_only_on_evidence() {
        assert!(!ran_out_of_memory(None, ""));
        assert!(!ran_out_of_memory(None, "thread 'main' panicked at src/main.rs:1:1:"));
        assert!(ran_out_of_memory(None, "ggml_backend_cpu_buffer_type_alloc_buffer: failed to allocate buffer of size 4294967296"));
        assert!(ran_out_of_memory(None, "terminate called after throwing an instance of 'std::bad_alloc'"));
        assert!(ran_out_of_memory(None, "ggml_vulkan: Device memory allocation of size 2147483648 failed. vk::Device::allocateMemory: ErrorOutOfDeviceMemory"));
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            assert!(ran_out_of_memory(Some(ExitStatus::from_raw(0xC000_0017)), ""));
            assert!(!ran_out_of_memory(Some(ExitStatus::from_raw(1)), ""));
            // A closed console window (CTRL_CLOSE_EVENT) is not memory.
            assert!(!ran_out_of_memory(Some(ExitStatus::from_raw(0xC000_013A)), ""));
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert!(ran_out_of_memory(Some(ExitStatus::from_raw(9)), "")); // SIGKILL
            assert!(!ran_out_of_memory(Some(ExitStatus::from_raw(6)), "")); // SIGABRT
        }
    }

    #[test]
    fn the_stderr_tail_is_bounded_and_keeps_only_error_lines() {
        let mut t = StderrTail::default();
        t.push("Me: here is the roadmap we discussed\n");
        assert_eq!(t.text(), "");
        for i in 0..500 {
            t.push(&format!("ggml_assert line {i}: {}", "x".repeat(400)));
        }
        let text = t.text();
        assert!(text.len() <= TAIL_BYTES, "{}", text.len());
        assert!(text.contains("line 499"), "keeps the latest lines");
        assert!(text.lines().all(|l| l.len() <= LINE_BYTES));
        let mut w = StderrTail::default();
        w.push("whisper_init_with_params_no_state: failed to load model");
        w.push(" Bonjour tout le monde");
        assert_eq!(w.text(), "whisper_init_with_params_no_state: failed to load model");
    }

    #[test]
    fn a_fake_helper_answers_in_turn_and_dies_on_cue() {
        let dir = tempfile::tempdir().unwrap();
        let exe = fake::script(dir.path(), "h", &[fake::reply(&[r#"{"event":"ready"}"#]), fake::crash(&["ggml_vulkan: device lost"], 3)]);
        let mut p = Process::<serde_json::Value>::spawn(&exe, &[("TALAAN_X".into(), "1".into())]).unwrap();
        p.send(&serde_json::json!({"op": "ping"})).unwrap();
        assert_eq!(p.recv_timeout(Duration::from_secs(10)).unwrap()["event"], "ready");
        p.send(&serde_json::json!({"op": "go"})).unwrap();
        assert!(matches!(p.recv_timeout(Duration::from_secs(10)), Err(RecvTimeoutError::Disconnected)));
        let d = p.died("test helper");
        assert!(d.detail.contains("exit code 3"), "{}", d.detail);
        assert!(d.detail.contains("device lost"), "{}", d.detail);
        assert!(!d.out_of_memory);
    }
}
