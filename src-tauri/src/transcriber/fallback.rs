//! Whisper on the GPU with the processor as a safety net (Windows). The meeting never fails because of the GPU:
//! when the GPU helper can't load the model or a chunk fails (crash, stall, error), that chunk and the rest of the
//! meeting go to the processor's Whisper. The failure is remembered (`gpu.json`) only once the processor has done
//! what the GPU couldn't, so an input the processor can't handle either isn't blamed on the GPU.

use super::engine::{ChunkResult, Engine};
use super::speech::EngineSpeech;
use voice_engine_models::{Kind as ModelKind, ModelHandle, ModelInfo};
use super::remote::{LoadError, RemoteWhisper, Timeouts};
use crate::gpu::{Gpu, Kind, Plan};
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
                log::warn!("speech GPU helper didn't load, using the processor: {}", crate::logging::text(&reason));
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
                    log::warn!("speech on the GPU failed, the processor takes over: {}", crate::logging::text(&e));
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
/// processor behind it; everything else (Parakeet, Nemotron, the Mac's Metal Whisper) loads in this process through
/// the engine. `use_gpu` is whether the card is asked for (`worker::asks_card`). Until the helper client replaces this
/// (Task 5) the model's handle stays with the helper path (it goes to the processor engine if that is ever needed).
pub fn load(model: &'static ModelInfo, handle: ModelHandle, use_gpu: bool, gpu: &Arc<Gpu>) -> Result<Box<dyn Engine>, String> {
    if cfg!(windows) && model.kind == ModelKind::Whisper {
        if let Plan::Gpu { exe, env } = gpu.plan(Kind::Stt, use_gpu) {
            let file = handle.dir().join(model.files[0].name);
            let remote = RemoteWhisper::load(&exe, &env, &file, model.name, Timeouts::default()).map(|e| Box::new(e) as Box<dyn Engine>);
            // The processor engine takes the handle the first time it is needed; until then the closure holds it.
            let mut held = Some(handle);
            let load_cpu: CpuLoader = Box::new(move || {
                let handle = held.take().ok_or_else(|| "the speech model is already loaded".to_string())?;
                EngineSpeech::load(model, handle, false).map(|e| Box::new(e) as Box<dyn Engine>)
            });
            return Fallback::start(remote, load_cpu, gpu.recorder(Kind::Stt), model.name, super::vad::Shape::whisper());
        }
    }
    EngineSpeech::load(model, handle, use_gpu).map(|e| Box::new(e) as Box<dyn Engine>)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
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

    /// Live (Windows, a PC with a discrete GPU): the real GPU helper through `load`, then the processor, on the same
    /// call; then a GPU that crashes on its first chunk (`TALAAN_GPU=fail`) and a helper that can't start, each
    /// falling back to the processor and writing gpu.json; then a later meeting skipping the GPU until Try again.
    /// Uses a scratch data folder, never Talaan's own:
    /// `TALAAN_DATA_DIR=<scratch> cargo test --release gpu_whisper_live -- --ignored --nocapture --test-threads=1`
    /// It needs `<scratch>/shared-models/<model id>/` (default `whisper-base`, or `TALAAN_SMOKE_MODEL`) and
    /// `<scratch>/shared-models/vad/`. The helpers come from `src-tauri/binaries` (or `TALAAN_GPU_HELPERS`); the audio is
    /// `fixtures/three-speakers` (or `TALAAN_SMOKE_MEETING`).
    #[test]
    #[ignore]
    fn gpu_whisper_live() {
        use crate::gpu::Skip;
        use crate::transcriber::pipeline::run;
        use crate::transcriber::speech::EngineVad;
        use std::path::PathBuf;
        use std::sync::atomic::AtomicBool;
        use std::time::Instant;

        let data = PathBuf::from(std::env::var("TALAAN_DATA_DIR").expect("set TALAAN_DATA_DIR to a scratch folder"));
        let own = dirs::data_dir().unwrap().join("Talaan");
        assert!(std::fs::canonicalize(&data).ok() != std::fs::canonicalize(&own).ok(), "not Talaan's own data folder");
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let helpers = std::env::var("TALAAN_GPU_HELPERS").map(PathBuf::from).unwrap_or_else(|_| manifest.join("binaries"));
        let meeting = std::env::var("TALAAN_SMOKE_MEETING").map(PathBuf::from).unwrap_or_else(|_| manifest.join("../fixtures/three-speakers"));
        let id = std::env::var("TALAAN_SMOKE_MODEL").unwrap_or_else(|_| "whisper-base".into());
        let model = voice_engine_models::catalog::find(&id).expect("a catalog id");
        let models = Arc::new(crate::models::Models::open(Some(&data)).unwrap());
        let model_dir = models.ready_dir(model.id).expect("the model in <scratch>/shared-models");
        let mut vad = EngineVad::new(models.hold(crate::models::VAD).unwrap());
        // SAFETY: an ignored test run with --test-threads=1; nothing else reads the environment meanwhile.
        unsafe { std::env::remove_var(talaan_llm_protocol::FAIL_ENV) };

        let gpu = Arc::new(Gpu::detect_with_helpers(&data, &helpers));
        gpu.clear();
        let mut transcribe = |label: &str, engine: &mut dyn Engine| {
            let t = Instant::now();
            let out = run(&meeting, engine, &mut vad, None, None, &AtomicBool::new(false), &mut |_| {}).unwrap();
            let secs = t.elapsed().as_secs_f32();
            let text = out.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join(" ");
            eprintln!("== {label}: {secs:.2}s for {:.0}s of speech, {} lines, language {:?}\n   {text}", out.speech_ms as f32 / 1000.0, out.lines.len(), out.language);
            (secs, text)
        };
        let words = |s: &str| s.to_lowercase().split(|c: char| !c.is_alphanumeric() && c != '\'').filter(|w| !w.is_empty()).map(str::to_string).collect::<Vec<_>>();
        let truth: Vec<serde_json::Value> = std::fs::read(meeting.join("truth.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let truth = words(&truth.iter().filter_map(|l| l["text"].as_str()).collect::<Vec<_>>().join(" "));
        let recall = |text: &str| {
            let got: std::collections::HashSet<String> = words(text).into_iter().collect();
            truth.iter().filter(|w| got.contains(*w)).count() as f32 / truth.len().max(1) as f32
        };

        // Which GPU the helper picks, straight from it.
        let Plan::Gpu { exe, env } = gpu.plan(Kind::Stt, true) else { panic!("GPU not planned: {}", crate::gpu::describe(&gpu.plan(Kind::Stt, true))) };
        let probe = RemoteWhisper::load(&exe, &env, &model_dir.join(model.files[0].name), model.name, Timeouts::default()).unwrap_or_else(|e| panic!("{e:?}"));
        eprintln!("== GPU: {:?}; driver: {}", probe.backend(), crate::gpu::driver_fingerprint());
        drop(probe);

        // 1. GPU, the way the app loads it (twice; "load" is the helper starting and loading the model).
        for round in ["GPU, 1st meeting", "GPU, 2nd meeting"] {
            let t = Instant::now();
            let mut e = load(model, models.hold(model.id).unwrap(), true, &gpu).unwrap();
            let loaded = t.elapsed().as_secs_f32();
            let (_, text) = transcribe(round, e.as_mut());
            eprintln!("   load {loaded:.2}s, recall vs truth.json {:.0}%", recall(&text) * 100.0);
            assert!(gpu.marker().is_none(), "the GPU worked, nothing to remember: {:?}", gpu.marker());
        }
        // 2. The processor, same audio.
        let t = Instant::now();
        let mut cpu = EngineSpeech::load(model, models.hold(model.id).unwrap(), false).unwrap();
        let loaded = t.elapsed().as_secs_f32();
        let (_, cpu_text) = transcribe("CPU", &mut cpu);
        eprintln!("   load {loaded:.2}s, recall vs truth.json {:.0}%", recall(&cpu_text) * 100.0);
        drop(cpu);

        // 3. The GPU helper aborts on its first chunk: the meeting still transcribes, on the processor.
        unsafe { std::env::set_var(talaan_llm_protocol::FAIL_ENV, "fail") };
        let mut e = load(model, models.hold(model.id).unwrap(), true, &gpu).unwrap();
        let (_, text) = transcribe("GPU crash -> CPU", e.as_mut());
        unsafe { std::env::remove_var(talaan_llm_protocol::FAIL_ENV) };
        drop(e);
        assert!(recall(&text) > 0.5, "the fallback transcript is the processor's");
        let failure = gpu.failure(Kind::Stt).expect("gpu.json names the speech helper");
        eprintln!("== gpu.json after the crash:\n{}", std::fs::read_to_string(data.join("gpu.json")).unwrap());
        assert!(failure.reason.contains("speech GPU helper"), "{}", failure.reason);
        assert!(gpu.failure(Kind::Llm).is_none());

        // 4. A later meeting skips the GPU (no helper is started) until Try again.
        assert!(matches!(gpu.plan(Kind::Stt, true), Plan::Cpu(Skip::Failed(_))));
        eprintln!("== plan now: {}", crate::gpu::describe(&gpu.plan(Kind::Stt, true)));
        let mut e = load(model, models.hold(model.id).unwrap(), true, &gpu).unwrap();
        transcribe("after the failure (GPU skipped)", e.as_mut());
        drop(e);
        gpu.clear();
        assert!(matches!(gpu.plan(Kind::Stt, true), Plan::Gpu { .. }), "Try again uses the GPU again");

        // 5. A helper that can't start at all (not a program): processor, and remembered.
        let broken = tempfile::tempdir().unwrap();
        std::fs::write(broken.path().join(format!("{}{}", crate::gpu::STT_HELPER, std::env::consts::EXE_SUFFIX)), "not a program").unwrap();
        let bad = Arc::new(Gpu::detect_with_helpers(&data, broken.path()));
        let mut e = load(model, models.hold(model.id).unwrap(), true, &bad).unwrap();
        let (_, text) = transcribe("broken helper -> CPU", e.as_mut());
        drop(e);
        assert!(recall(&text) > 0.5);
        let reason = bad.failure(Kind::Stt).expect("remembered").reason;
        eprintln!("== broken helper reason: {reason}");
        assert!(reason.contains("couldn't start"), "{reason}");
        bad.clear();
    }
}
