//! Whisper on the Windows graphics card through the engine's speech helper (`talaan-stt-vulkan.exe`), with the
//! engine's processor helper (`talaan-stt.exe`) behind it: `SttClient` starts them, restarts them, falls back to the
//! processor when the card fails, and remembers the failure. Everything else speaks in this process (`speech.rs`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use voice_engine_client::{Limits, SttConfig};

/// The graphics card helper's file name (without the extension).
pub const GPU_HELPER: &str = "talaan-stt-vulkan";
/// The processor helper's file name (without the extension).
#[cfg_attr(not(windows), allow(dead_code))]
pub const CPU_HELPER: &str = "talaan-stt";
/// The file in the data folder where the engine remembers that the card failed.
pub const FAILURES_FILE: &str = "gpu-stt-failures";

#[cfg_attr(not(windows), allow(dead_code))]
fn helper(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

/// The engine's speech client settings for Talaan: the helpers sit next to the app, the card helper is used only
/// when installed (a build without the Vulkan SDK has none), a failed card is remembered in `data_dir` for this app
/// version, and implicit Vulkan layers (overlays that hook every Vulkan app) are off for the card helper.
#[cfg_attr(not(windows), allow(dead_code))] // the Mac has no speech helper; the function is pure, so it is tested there
pub fn config(exe_dir: &Path, data_dir: &Path, version: &str) -> SttConfig {
    let gpu = helper(exe_dir, GPU_HELPER);
    SttConfig {
        cpu_helper: helper(exe_dir, CPU_HELPER),
        gpu_helper: gpu.is_file().then_some(gpu),
        env: vec![],
        gpu_env: vec![("VK_LOADER_LAYERS_DISABLE".into(), "~implicit~".into())],
        gpu_failures: Some(data_dir.join(FAILURES_FILE)),
        gpu_key: version.into(),
        // No events arrive while Whisper decodes a chunk, and Talaan's Whisper chunks are up to
        // `Shape::whisper().max_len`, so the stall limit sits well above the longest decode.
        limits: Limits { stall: Duration::from_secs(10 * 60), ..Limits::default() },
    }
}

/// Whether the engine remembers a card failure for this app version (the file holds the key it failed with; the
/// engine compares it to `gpu_key`). Time estimates use this to say "processor" once the card has failed.
pub fn stt_card_failed(data_dir: &Path, version: &str) -> bool {
    std::fs::read_to_string(data_dir.join(FAILURES_FILE)).is_ok_and(|t| t.trim() == version)
}

#[cfg(windows)]
pub use windows_impl::HelperSpeech;

#[cfg(windows)]
mod windows_impl {
    use std::sync::Arc;

    use voice_engine_client::{EngineError, SttClient};
    use voice_engine_models::{ModelHandle, ModelInfo};
    use voice_engine_protocol::stt::{SttLoad, SttModelKind};
    use voice_engine_protocol::Backend;
    use voice_engine_stt::{ChunkOptions, SpeechBackend, Timing};

    use crate::transcriber::engine::{ChunkResult, Engine};
    use crate::transcriber::speech::to_chunk;
    use crate::transcriber::vad::Shape;

    pub struct HelperSpeech {
        client: Arc<SttClient>,
        label: String,
        last: Option<SpeechBackend>,
        _model: ModelHandle, // keeps the files from being erased while loaded
    }

    fn placed(b: &Backend, fell_back: bool) -> SpeechBackend {
        SpeechBackend { backend: b.backend.clone(), device: b.device.clone(), fell_back }
    }

    fn describe(e: &EngineError) -> String {
        match e {
            EngineError::Cancelled => "the speech helper was stopped".to_string(),
            other => format!("{other:?}"),
        }
    }

    impl HelperSpeech {
        /// Loads the Whisper model into the helper (the client falls back to the processor helper by itself).
        pub fn load(client: Arc<SttClient>, model: &'static ModelInfo, handle: ModelHandle) -> Result<Self, String> {
            let out = client
                .load(SttLoad {
                    model_id: model.id.into(),
                    model_dir: handle.dir().to_string_lossy().into_owned(),
                    kind: SttModelKind::Whisper,
                    gpu: true,
                    threads: None,
                })
                .map_err(|e| describe(&e))?;
            let last = Some(placed(&out.backend, out.fell_back));
            Ok(HelperSpeech { client, label: model.name.to_string(), last, _model: handle })
        }
    }

    impl Engine for HelperSpeech {
        fn transcribe(&mut self, samples: &[f32], language: Option<&str>) -> Result<ChunkResult, String> {
            let opts = ChunkOptions { language: language.map(str::to_string), timing: Timing::Segments, ..Default::default() };
            let out = self.client.transcribe(samples, &opts).map_err(|e| describe(&e))?;
            self.last = Some(placed(&out.backend, out.fell_back || self.last.as_ref().is_some_and(|b| b.fell_back)));
            Ok(to_chunk(out.value))
        }
        fn label(&self) -> &str {
            &self.label
        }
        fn chunking(&self) -> Shape {
            Shape::whisper()
        }
        fn backend(&self) -> Option<SpeechBackend> {
            self.last.clone()
        }
    }

    impl Drop for HelperSpeech {
        /// The worker drops its engine to free memory (idle, or another model): the helper lets go of the model too.
        /// A quick request; the helper keeps running for the next load.
        fn drop(&mut self) {
            if let Err(e) = self.client.unload() {
                log::warn!("the speech helper did not confirm unloading: {}", crate::logging::text(describe(&e)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_helper_is_used_only_when_installed() {
        let exe = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let c = config(exe.path(), data.path(), "1.2.3");
        assert_eq!(c.cpu_helper, exe.path().join(format!("talaan-stt{}", std::env::consts::EXE_SUFFIX)));
        assert_eq!(c.gpu_helper, None);
        std::fs::write(exe.path().join(format!("talaan-stt-vulkan{}", std::env::consts::EXE_SUFFIX)), b"").unwrap();
        let c = config(exe.path(), data.path(), "1.2.3");
        assert_eq!(c.gpu_helper, Some(exe.path().join(format!("talaan-stt-vulkan{}", std::env::consts::EXE_SUFFIX))));
    }

    #[test]
    fn config_carries_talaans_settings() {
        let c = config(Path::new("/app"), Path::new("/data"), "1.2.3");
        assert_eq!(c.gpu_failures, Some(Path::new("/data").join("gpu-stt-failures")));
        assert_eq!(c.gpu_key, "1.2.3");
        assert!(c.env.is_empty());
        assert_eq!(c.gpu_env, vec![("VK_LOADER_LAYERS_DISABLE".to_string(), "~implicit~".to_string())]);
        assert_eq!(c.limits.stall, Duration::from_secs(600));
        assert_eq!(c.limits.start, Limits::default().start);
        assert_eq!(c.limits.total, Limits::default().total);
    }

    #[test]
    fn a_remembered_card_failure_counts_only_for_its_version() {
        let data = tempfile::tempdir().unwrap();
        assert!(!stt_card_failed(data.path(), "1.2.3"));
        std::fs::write(data.path().join(FAILURES_FILE), "1.2.3").unwrap();
        assert!(stt_card_failed(data.path(), "1.2.3"));
        assert!(!stt_card_failed(data.path(), "1.2.4"));
    }
}
