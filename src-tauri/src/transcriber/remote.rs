//! Whisper in the Windows GPU helper (`talaan-stt-vulkan.exe`), behind the same `Engine` trait as the in-process
//! engines. Loading says whether the helper actually got a GPU; a crash, a stall or an error comes back as a
//! plain error, and `transcriber::fallback` then redoes the chunk on the processor.

use super::engine::{ChunkResult, Engine, Piece};
use crate::helper::Process;
use std::path::Path;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;
use talaan_llm_protocol::stt::{encode_samples, SttEvent, SttRequest};
use talaan_llm_protocol::Backend;

pub const HELPER: &str = "talaan-stt-vulkan";
const WHO: &str = "speech GPU helper";

/// Why the GPU helper isn't used.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    /// Nothing wrong, there is just no GPU to use (no discrete GPU): use the processor, remember nothing.
    NoGpu,
    /// It failed: didn't start, died, timed out, or couldn't load the model on the GPU.
    Failed(String),
}

/// How long the helper gets: to start and list devices, to load a model, and per chunk.
#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub start: Duration,
    pub load: Duration,
    /// Fixed part of a chunk's time limit; the rest is `per_audio` times the chunk's length.
    pub chunk: Duration,
    pub per_audio: u32,
}

impl Default for Timeouts {
    /// Generous: Whisper Large Turbo on a GPU does a 30 s chunk in a second or two, 3x real time on a CPU.
    fn default() -> Self {
        Timeouts { start: Duration::from_secs(60), load: Duration::from_secs(180), chunk: Duration::from_secs(60), per_audio: 2 }
    }
}

pub struct RemoteWhisper {
    proc: Option<Process<SttEvent>>,
    label: String,
    #[cfg_attr(not(test), allow(dead_code))]
    backend: Backend,
    timeouts: Timeouts,
}

impl RemoteWhisper {
    pub fn load(exe: &Path, env: &[(String, String)], model_file: &Path, label: &str, timeouts: Timeouts) -> Result<Self, LoadError> {
        let mut proc = Process::<SttEvent>::spawn(exe, env).map_err(|e| LoadError::Failed(format!("couldn't start {WHO} ({}): {e}", exe.display())))?;
        let ready = ask(&mut proc, &SttRequest::Ping, timeouts.start, |e| match e {
            SttEvent::Ready { backend } => Some(backend),
            _ => None,
        });
        let backend = match ready {
            Ok(b) if b.on_gpu() => b,
            Ok(_) => {
                proc.kill();
                return Err(LoadError::NoGpu);
            }
            Err(e) => return Err(LoadError::Failed(e.finish(proc))),
        };
        let req = SttRequest::Load { model_path: model_file.to_string_lossy().into_owned(), gpu: true };
        let loaded = ask(&mut proc, &req, timeouts.load, |e| match e {
            SttEvent::Loaded { backend } => Some(backend),
            _ => None,
        });
        match loaded {
            Ok(b) if b.on_gpu() => log::info!("speech GPU helper loaded the model on {} ({}, {} MB)", b.backend, b.device.as_deref().unwrap_or("?"), b.vram_mb),
            Ok(_) => {
                proc.kill();
                return Err(LoadError::NoGpu);
            }
            Err(e) => return Err(LoadError::Failed(e.finish(proc))),
        }
        Ok(RemoteWhisper { proc: Some(proc), label: label.into(), backend, timeouts })
    }

    /// The GPU the model is on.
    #[cfg(test)]
    pub fn backend(&self) -> &Backend {
        &self.backend
    }
}

/// A request that didn't get its answer.
enum AskError {
    /// The helper answered with an error.
    Said(String),
    Timeout,
    Died,
    Io(String),
}

impl AskError {
    /// The message, stopping the helper unless it only reported an error.
    fn finish(self, proc: Process<SttEvent>) -> String {
        match self {
            AskError::Said(m) => {
                proc.kill();
                m
            }
            AskError::Timeout => {
                proc.kill();
                format!("{WHO} stopped answering")
            }
            AskError::Died => proc.died(WHO).detail,
            AskError::Io(m) => {
                proc.kill();
                m
            }
        }
    }
}

fn ask<T>(proc: &mut Process<SttEvent>, req: &SttRequest, limit: Duration, want: impl Fn(SttEvent) -> Option<T>) -> Result<T, AskError> {
    proc.send(req).map_err(|e| AskError::Io(format!("couldn't talk to the {WHO}: {e}")))?;
    loop {
        match proc.recv_timeout(limit) {
            Ok(SttEvent::Error { message }) => return Err(AskError::Said(message)),
            Ok(ev) => {
                if let Some(t) = want(ev) {
                    return Ok(t);
                }
            }
            Err(RecvTimeoutError::Timeout) => return Err(AskError::Timeout),
            Err(RecvTimeoutError::Disconnected) => return Err(AskError::Died),
        }
    }
}

impl Engine for RemoteWhisper {
    fn transcribe(&mut self, samples: &[f32], language: Option<&str>) -> Result<ChunkResult, String> {
        let Some(mut proc) = self.proc.take() else { return Err(format!("the {WHO} stopped earlier")) };
        let audio = Duration::from_millis(samples.len() as u64 * 1000 / 16_000);
        let limit = self.timeouts.chunk + audio * self.timeouts.per_audio;
        let req = SttRequest::Transcribe { samples: encode_samples(samples), language: language.map(str::to_string) };
        let got = ask(&mut proc, &req, limit, |e| match e {
            SttEvent::Transcribed { pieces, language } => Some((pieces, language)),
            _ => None,
        });
        match got {
            Ok((pieces, language)) => {
                self.proc = Some(proc);
                Ok(ChunkResult { pieces: pieces.into_iter().map(|p| Piece { start_ms: p.start_ms, end_ms: p.end_ms, text: p.text }).collect(), language })
            }
            // An error from Whisper itself leaves the helper usable.
            Err(AskError::Said(m)) => {
                self.proc = Some(proc);
                Err(m)
            }
            Err(e) => Err(e.finish(proc)),
        }
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn chunking(&self) -> super::vad::Shape {
        super::vad::Shape::whisper()
    }
}

impl Drop for RemoteWhisper {
    fn drop(&mut self) {
        if let Some(p) = self.proc.take() {
            p.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helper::fake::{crash, reply, script};

    const READY_GPU: &str = r#"{"event":"ready","backend":"vulkan","device":"NVIDIA GeForce RTX 5070 Ti","vram_mb":16303}"#;
    const LOADED_GPU: &str = r#"{"event":"loaded","backend":"vulkan","device":"NVIDIA GeForce RTX 5070 Ti","vram_mb":16303}"#;
    const HEARD: &str = r#"{"event":"transcribed","pieces":[{"start_ms":0,"end_ms":900,"text":"Hello there."}],"language":"en"}"#;

    fn quick() -> Timeouts {
        Timeouts { start: Duration::from_secs(10), load: Duration::from_secs(10), chunk: Duration::from_secs(10), per_audio: 0 }
    }

    fn load(exe: &Path) -> Result<RemoteWhisper, LoadError> {
        RemoteWhisper::load(exe, &[], Path::new("model.bin"), "Whisper Large Turbo", quick())
    }

    #[test]
    fn a_gpu_helper_transcribes_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let exe = script(dir.path(), "stt", &[reply(&[READY_GPU]), reply(&[LOADED_GPU]), reply(&[HEARD]), reply(&[HEARD])]);
        let mut w = load(&exe).unwrap();
        assert_eq!(w.backend().device.as_deref(), Some("NVIDIA GeForce RTX 5070 Ti"));
        assert_eq!(w.label(), "Whisper Large Turbo");
        for _ in 0..2 {
            let r = w.transcribe(&[0.1; 16_000], Some("en")).unwrap();
            assert_eq!(r.pieces, vec![Piece { start_ms: 0, end_ms: 900, text: "Hello there.".into() }]);
            assert_eq!(r.language.as_deref(), Some("en"));
        }
    }

    #[test]
    fn no_discrete_gpu_is_not_a_failure() {
        let dir = tempfile::tempdir().unwrap();
        let exe = script(dir.path(), "stt", &[reply(&[r#"{"event":"ready","backend":"cpu","vram_mb":0}"#])]);
        assert_eq!(load(&exe).err(), Some(LoadError::NoGpu));
    }

    #[test]
    fn a_helper_that_cant_start_or_load_failed() {
        let dir = tempfile::tempdir().unwrap();
        let Err(LoadError::Failed(e)) = load(&dir.path().join("missing.exe")) else { panic!() };
        assert!(e.contains("couldn't start"), "{e}");
        let exe = script(dir.path(), "dies", &[crash(&["ggml_vulkan: vk::createInstance: ErrorIncompatibleDriver"], 1)]);
        let Err(LoadError::Failed(e)) = load(&exe) else { panic!() };
        assert!(e.contains("exit code 1") && e.contains("ErrorIncompatibleDriver"), "{e}");
        let exe = script(dir.path(), "noload", &[reply(&[READY_GPU]), reply(&[r#"{"event":"error","message":"couldn't load the speech model: bad file"}"#])]);
        assert_eq!(load(&exe).err(), Some(LoadError::Failed("couldn't load the speech model: bad file".into())));
    }

    #[test]
    fn a_crash_mid_meeting_is_an_error_and_the_helper_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let exe = script(dir.path(), "stt", &[reply(&[READY_GPU]), reply(&[LOADED_GPU]), reply(&[HEARD]), crash(&["ggml_vulkan: Device lost"], -1073740791)]);
        let mut w = load(&exe).unwrap();
        assert!(w.transcribe(&[0.0; 1600], None).is_ok());
        let e = w.transcribe(&[0.0; 1600], None).unwrap_err();
        assert!(e.contains("Device lost"), "{e}");
        assert!(w.transcribe(&[0.0; 1600], None).unwrap_err().contains("stopped earlier"));
    }

    #[test]
    fn a_stalled_helper_times_out() {
        let dir = tempfile::tempdir().unwrap();
        // Answers ping and load, then never answers the chunk.
        let exe = script(dir.path(), "stt", &[reply(&[READY_GPU]), reply(&[LOADED_GPU])]);
        let mut w = RemoteWhisper::load(&exe, &[], Path::new("m"), "W", Timeouts { chunk: Duration::from_millis(300), ..quick() }).unwrap();
        let t = std::time::Instant::now();
        let e = w.transcribe(&[0.0; 1600], None).unwrap_err();
        assert!(e.contains("stopped answering"), "{e}");
        assert!(t.elapsed() < Duration::from_secs(5));
    }
}
