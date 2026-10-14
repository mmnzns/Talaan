//! The app's side of the local AI helper (`talaan-llm`): starts it when needed, sends one request at a time,
//! streams progress, and turns a crash (usually out of memory) into a plain error instead of taking Talaan down.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};
use talaan_llm_protocol::{encode, Completion, Event, Phase, Reply, Request};

pub const HELPER: &str = "talaan-llm";

/// Where the helper lives: next to Talaan's own binary (in the app bundle, `Contents/MacOS/`, and in
/// `target/<profile>/` during development).
pub fn helper_path() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(HELPER))).unwrap_or_else(|| PathBuf::from(HELPER))
}

struct Running {
    child: Child,
    stdin: ChildStdin,
    events: Receiver<Event>,
}

pub struct LocalAi {
    exe: PathBuf,
    running: Option<Running>,
}

impl LocalAi {
    pub fn new(exe: &Path) -> Self {
        LocalAi { exe: exe.to_path_buf(), running: None }
    }

    fn start(&mut self) -> Result<&mut Running, String> {
        let alive = self.running.as_mut().is_some_and(|r| matches!(r.child.try_wait(), Ok(None)));
        if !alive {
            self.running = None;
            let mut child = Command::new(&self.exe)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("couldn't start the local AI ({}): {e}", self.exe.display()))?;
            let stdin = child.stdin.take().expect("piped");
            let stdout = child.stdout.take().expect("piped");
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
            let mut r = Running { child, stdin, events };
            send(&mut r.stdin, &Request::Ping)?;
            match r.events.recv_timeout(Duration::from_secs(15)) {
                Ok(Event::Ready) => {}
                Ok(Event::Error { message }) => return Err(message),
                _ => return Err("the local AI didn't start".into()),
            }
            self.running = Some(r);
        }
        Ok(self.running.as_mut().expect("started above"))
    }

    /// Runs one completion. `progress` gets (phase, done, total). Cancelling stops the helper.
    pub fn complete(&mut self, c: &Completion, cancel: &AtomicBool, progress: &mut dyn FnMut(Phase, u32, u32)) -> Result<Reply, String> {
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
                Ok(Event::Ready) => {}
                Err(RecvTimeoutError::Timeout) => {
                    // A summary should never take this long; something is stuck.
                    if started.elapsed() > Duration::from_secs(30 * 60) {
                        self.stop();
                        return Err("the local AI took too long".into());
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.stop();
                    return Err("the local AI stopped unexpectedly, probably out of memory. Try the smaller model, or close other apps.".into());
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

    /// A helper that dies mid-request (out of memory, crash) becomes a readable error.
    #[test]
    fn a_crashing_helper_is_reported_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("fake-llm");
        // Answers the ping, then exits on the first real request.
        std::fs::write(&fake, "#!/bin/sh\nread line\necho '{\"event\":\"ready\"}'\nread line\nexit 1\n").unwrap();
        std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let mut ai = LocalAi::new(&fake);
        let c = Completion { model_path: "x".into(), n_ctx: 512, system: "".into(), user: "".into(), json_schema: None, max_tokens: 1, temperature: 0.0 };
        let e = ai.complete(&c, &AtomicBool::new(false), &mut |_, _, _| {}).unwrap_err();
        assert!(e.contains("stopped unexpectedly"), "{e}");
    }

    /// Real model through the real helper. Build it first: `cargo build --release -p talaan-llm`, then
    /// `TALAAN_LLM_MODEL=<gguf> cargo test --release local_ai_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn local_ai_smoke() {
        let model = std::env::var("TALAAN_LLM_MODEL").unwrap();
        let exe = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join(HELPER);
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
        let exe = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join(HELPER);
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
