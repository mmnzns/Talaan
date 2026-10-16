//! Whisper on the GPU with the processor as a safety net (Windows). The meeting never fails because of the GPU:
//! when the GPU helper can't load the model or a chunk fails (crash, stall, error), that chunk and the rest of the
//! meeting go to the processor's Whisper. The failure is remembered (`gpu.json`) only once the processor has done
//! what the GPU couldn't, so an input the processor can't handle either isn't blamed on the GPU.

use super::engine::{self, ChunkResult, Engine};
use super::models::{EngineKind, ModelInfo};
use super::remote::{LoadError, RemoteWhisper, Timeouts};
use crate::gpu::{Gpu, Kind, Plan};
use std::path::Path;
use std::sync::Arc;

pub type CpuLoader = Box<dyn FnMut() -> Result<Box<dyn Engine>, String> + Send>;
pub type OnFailure = Arc<dyn Fn(&str) + Send + Sync>;

pub struct Fallback {
    gpu: Option<Box<dyn Engine>>,
    cpu: Option<Box<dyn Engine>>,
    load_cpu: CpuLoader,
    on_failure: OnFailure,
    /// A GPU failure waiting for the processor to prove it wasn't the input.
    pending: Option<String>,
    label: String,
    chunking: super::vad::Shape,
}

impl Fallback {
    /// Starts from the GPU helper's load result. A failed load goes straight to the processor.
    pub fn start(gpu: Result<Box<dyn Engine>, LoadError>, load_cpu: CpuLoader, on_failure: OnFailure, label: &str, chunking: super::vad::Shape) -> Result<Box<dyn Engine>, String> {
        let mut f = Fallback { gpu: None, cpu: None, load_cpu, on_failure, pending: None, label: label.into(), chunking };
        match gpu {
            Ok(e) => f.gpu = Some(e),
            Err(LoadError::NoGpu) => f.cpu = Some((f.load_cpu)()?),
            Err(LoadError::Failed(reason)) => {
                f.cpu = Some((f.load_cpu)()?);
                (f.on_failure)(&reason);
            }
        }
        Ok(Box::new(f))
    }

    fn cpu(&mut self) -> Result<&mut Box<dyn Engine>, String> {
        if self.cpu.is_none() {
            self.cpu = Some((self.load_cpu)()?);
        }
        Ok(self.cpu.as_mut().expect("loaded above"))
    }
}

impl Engine for Fallback {
    fn transcribe(&mut self, samples: &[f32], language: Option<&str>) -> Result<ChunkResult, String> {
        if let Some(gpu) = self.gpu.as_mut() {
            match gpu.transcribe(samples, language) {
                Ok(r) => return Ok(r),
                Err(e) => {
                    // The helper is done for this meeting; free its GPU memory now.
                    self.gpu = None;
                    self.pending = Some(e);
                }
            }
        }
        let result = self.cpu()?.transcribe(samples, language);
        if result.is_ok() {
            if let Some(reason) = self.pending.take() {
                (self.on_failure)(&reason);
            }
        }
        result
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn chunking(&self) -> super::vad::Shape {
        self.chunking
    }
}

/// The speech engine for a model: on Windows, Whisper goes through the GPU helper when `gpu` says so, with the
/// processor behind it; everything else (Parakeet, Nemotron, the Mac's Metal Whisper) loads as before.
pub fn load(model: &ModelInfo, dir: &Path, use_gpu: bool, gpu: &Arc<Gpu>) -> Result<Box<dyn Engine>, String> {
    if cfg!(windows) && model.engine == EngineKind::Whisper {
        if let Plan::Gpu { exe, env } = gpu.plan(Kind::Stt, use_gpu) {
            let file = dir.join(model.files[0].name);
            let remote = RemoteWhisper::load(&exe, &env, &file, model.name, Timeouts::default()).map(|e| Box::new(e) as Box<dyn Engine>);
            let cpu_model: &'static ModelInfo = super::models::find(model.id).ok_or_else(|| format!("unknown speech model {}", model.id))?;
            let cpu_dir = dir.to_path_buf();
            let load_cpu: CpuLoader = Box::new(move || engine::load(cpu_model, &cpu_dir, false));
            return Fallback::start(remote, load_cpu, gpu.recorder(Kind::Stt), model.name, super::vad::Shape::whisper());
        }
    }
    engine::load(model, dir, use_gpu)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcriber::engine::Piece;
    use crate::transcriber::vad::Shape;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Says `text`, or fails from chunk number `fail_from` on.
    struct Fake {
        text: &'static str,
        fail_from: Option<usize>,
        calls: usize,
    }
    impl Engine for Fake {
        fn transcribe(&mut self, _s: &[f32], _l: Option<&str>) -> Result<ChunkResult, String> {
            self.calls += 1;
            if self.fail_from.is_some_and(|n| self.calls >= n) {
                return Err(format!("{} broke", self.text));
            }
            Ok(ChunkResult { pieces: vec![Piece { start_ms: 0, end_ms: 10, text: self.text.into() }], language: None })
        }
        fn label(&self) -> &str {
            self.text
        }
        fn chunking(&self) -> Shape {
            Shape::whisper()
        }
    }

    fn fake(text: &'static str, fail_from: Option<usize>) -> Box<dyn Engine> {
        Box::new(Fake { text, fail_from, calls: 0 })
    }

    struct Rig {
        recorded: Arc<Mutex<Vec<String>>>,
        cpu_loads: Arc<AtomicUsize>,
    }

    fn make_rig(cpu_fails: bool) -> (Rig, CpuLoader, OnFailure) {
        let recorded: Arc<Mutex<Vec<String>>> = Arc::default();
        let cpu_loads: Arc<AtomicUsize> = Arc::default();
        let (r, n) = (recorded.clone(), cpu_loads.clone());
        let load: CpuLoader = Box::new(move || {
            n.fetch_add(1, Ordering::SeqCst);
            Ok(fake("cpu", cpu_fails.then_some(1)))
        });
        let on: OnFailure = Arc::new(move |reason: &str| r.lock().unwrap().push(reason.to_string()));
        (Rig { recorded, cpu_loads }, load, on)
    }

    fn text(r: Result<ChunkResult, String>) -> String {
        r.map(|c| c.pieces[0].text.clone()).unwrap_or_else(|e| e)
    }

    #[test]
    fn a_working_gpu_never_touches_the_processor() {
        let (rig, load, on) = make_rig(false);
        let mut e = Fallback::start(Ok(fake("gpu", None)), load, on, "Whisper Large Turbo", Shape::whisper()).unwrap();
        for _ in 0..3 {
            assert_eq!(text(e.transcribe(&[0.0; 10], None)), "gpu");
        }
        assert_eq!(e.label(), "Whisper Large Turbo");
        assert_eq!(rig.cpu_loads.load(Ordering::SeqCst), 0);
        assert!(rig.recorded.lock().unwrap().is_empty());
    }

    #[test]
    fn a_gpu_crash_mid_meeting_redoes_the_chunk_on_the_processor_and_remembers() {
        let (rig, load, on) = make_rig(false);
        let mut e = Fallback::start(Ok(fake("gpu", Some(2))), load, on, "W", Shape::whisper()).unwrap();
        assert_eq!(text(e.transcribe(&[0.0; 10], None)), "gpu");
        assert_eq!(text(e.transcribe(&[0.0; 10], None)), "cpu", "the failed chunk is redone");
        assert_eq!(text(e.transcribe(&[0.0; 10], None)), "cpu", "and the rest stays on the processor");
        assert_eq!(*rig.recorded.lock().unwrap(), ["gpu broke"]);
        assert_eq!(rig.cpu_loads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_chunk_the_processor_cant_do_either_is_not_blamed_on_the_gpu() {
        let (rig, load, on) = make_rig(true);
        let mut e = Fallback::start(Ok(fake("gpu", Some(1))), load, on, "W", Shape::whisper()).unwrap();
        assert_eq!(text(e.transcribe(&[0.0; 10], None)), "cpu broke");
        assert!(rig.recorded.lock().unwrap().is_empty());
    }

    #[test]
    fn a_failed_gpu_load_uses_the_processor_and_remembers_but_no_gpu_does_not() {
        let (rig, load, on) = make_rig(false);
        let mut e = Fallback::start(Err(LoadError::Failed("speech GPU helper stopped: exit code 1".into())), load, on, "W", Shape::whisper()).unwrap();
        assert_eq!(text(e.transcribe(&[0.0; 10], None)), "cpu");
        assert_eq!(*rig.recorded.lock().unwrap(), ["speech GPU helper stopped: exit code 1"]);

        let (rig, load, on) = make_rig(false);
        let mut e = Fallback::start(Err(LoadError::NoGpu), load, on, "W", Shape::whisper()).unwrap();
        assert_eq!(text(e.transcribe(&[0.0; 10], None)), "cpu");
        assert!(rig.recorded.lock().unwrap().is_empty());
    }

    #[test]
    fn a_processor_that_cant_load_is_the_error() {
        let on: OnFailure = Arc::new(|_: &str| {});
        let load: CpuLoader = Box::new(|| Err("couldn't load the speech model: gone".into()));
        let e = Fallback::start(Err(LoadError::Failed("x".into())), load, on, "W", Shape::whisper()).err();
        assert_eq!(e.as_deref(), Some("couldn't load the speech model: gone"));
    }

    /// End to end with a fake GPU helper that crashes on the second chunk: the meeting's chunks all come back and
    /// gpu.json names the speech helper.
    #[test]
    fn a_crashing_gpu_helper_falls_back_through_gpu_json() {
        use crate::helper::fake::{crash, reply, script};
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let gpu = Arc::new(Gpu::fake(&dir.path().join("data"), &bin, true, "1.1.0", "drv"));
        let ready = r#"{"event":"ready","backend":"vulkan","device":"RTX","vram_mb":16000}"#;
        let loaded = r#"{"event":"loaded","backend":"vulkan","device":"RTX","vram_mb":16000}"#;
        let heard = r#"{"event":"transcribed","pieces":[{"start_ms":0,"end_ms":10,"text":"gpu"}]}"#;
        let exe = script(&bin, "stt", &[reply(&[ready]), reply(&[loaded]), reply(&[heard]), crash(&["ggml_vulkan: Device lost"], 5)]);
        let remote = RemoteWhisper::load(&exe, &[], Path::new("m.bin"), "W", Timeouts::default()).map(|e| Box::new(e) as Box<dyn Engine>);
        let (r, load, _) = make_rig(false);
        let mut e = Fallback::start(remote, load, gpu.recorder(Kind::Stt), "W", Shape::whisper()).unwrap();
        let got: Vec<String> = (0..3).map(|_| text(e.transcribe(&[0.0; 160], None))).collect();
        assert_eq!(got, ["gpu", "cpu", "cpu"]);
        assert_eq!(r.cpu_loads.load(Ordering::SeqCst), 1);
        let reason = gpu.failure(Kind::Stt).unwrap().reason;
        assert!(reason.contains("exit code 5") && reason.contains("Device lost"), "{reason}");
        assert!(gpu.failure(Kind::Llm).is_none());
    }
}
