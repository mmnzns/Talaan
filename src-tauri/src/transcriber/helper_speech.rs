//! Whisper on the Windows graphics card through the engine's speech helper (`talaan-stt-vulkan.exe`), with the
//! engine's processor helper (`talaan-stt.exe`) behind it: `SttClient` starts them, restarts them, falls back to the
//! processor when the card fails, and remembers the failure. Everything else speaks in this process (`speech.rs`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use voice_engine_client::{Fallback, Limits, SttConfig};

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

/// The reason to keep for a loaded engine after a request. The card failed when the model loaded (`before`); the
/// requests that follow run on the processor helper and say `RememberedFailure` (the card was not tried), which would
/// lose that, so the failure stays until a request says something else: the card ran again after "Try again"
/// (`None`), or failed anew.
#[cfg_attr(not(windows), allow(dead_code))] // only the Windows speech helper has a load and requests to follow; the function is pure, so it is tested everywhere
pub fn keep_card_failure(before: Option<Fallback>, now: Option<Fallback>) -> Option<Fallback> {
    if now == Some(Fallback::RememberedFailure) && crate::gpu::is_card_failure(before) {
        before
    } else {
        now
    }
}

/// Whether the engine remembers a card failure for this app version (the file holds the key it failed with; the
/// engine compares it to `gpu_key`). Time estimates use this to say "processor" once the card has failed.
pub fn stt_card_failed(data_dir: &Path, version: &str) -> bool {
    std::fs::read_to_string(data_dir.join(FAILURES_FILE)).is_ok_and(|t| t.trim() == version)
}

/// Words for the user from what the speech helper said (matched on the kind of error). The error's own text goes
/// to the log, through `logging::text`. `Cancelled` is a stop that Talaan asked for (quitting, a deleted meeting;
/// the worker leaves those meetings alone): when it still reaches the screen, the helper was stopped some other
/// way (Try again on the card), so it reads as an unexpected stop.
#[cfg_attr(not(windows), allow(dead_code))] // the Mac has no speech helper; the function is pure, so it is tested there
pub fn helper_error_text(e: &voice_engine_client::EngineError, name: &str) -> String {
    use crate::i18n::{tr, tr_with};
    use voice_engine_client::EngineError as E;
    match e {
        E::OutOfMemory => tr("transcribe.out_of_memory"),
        E::ModelMissing { .. } => tr_with("error.download_speech_model", &[("name", name)]),
        E::BadModel => tr_with("download.corrupt", &[("name", name)]),
        // CouldNotStart, Crashed, TimedOut, GpuFailed, Unsupported, Internal, VersionMismatch ...: nothing the user can act on.
        other => {
            let detail = format!("{other:?}");
            log::warn!("speech helper: {}", crate::logging::text(detail));
            tr("transcribe.helper_failed")
        }
    }
}

#[cfg(windows)]
pub use windows_impl::HelperSpeech;

#[cfg(windows)]
mod windows_impl {
    use std::sync::Arc;

    use super::keep_card_failure;
    use voice_engine_client::SttClient;
    use voice_engine_models::{ModelHandle, ModelInfo};
    use voice_engine_protocol::stt::{SttLoad, SttModelKind};
    use voice_engine_protocol::Backend;
    use voice_engine_stt::{ChunkOptions, Fallback, SpeechBackend, Timing};

    use crate::transcriber::engine::{ChunkResult, Engine};
    use crate::transcriber::speech::to_chunk;
    use super::helper_error_text;
    use crate::transcriber::vad::Shape;

    pub struct HelperSpeech {
        client: Arc<SttClient>,
        label: String,
        last: Option<SpeechBackend>,
        _model: ModelHandle, // keeps the files from being erased while loaded
    }

    fn placed(b: &Backend, fallback: Option<Fallback>) -> SpeechBackend {
        SpeechBackend { backend: b.backend.clone(), device: b.device.clone(), fallback }
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
                .map_err(|e| helper_error_text(&e, model.name))?;
            let last = Some(placed(&out.backend, out.fallback));
            Ok(HelperSpeech { client, label: model.name.to_string(), last, _model: handle })
        }
    }

    impl Engine for HelperSpeech {
        fn transcribe(&mut self, samples: &[f32], language: Option<&str>) -> Result<ChunkResult, String> {
            let opts = ChunkOptions { language: language.map(str::to_string), timing: Timing::Segments, ..Default::default() };
            let out = self.client.transcribe(samples, &opts).map_err(|e| helper_error_text(&e, &self.label))?;
            self.last = Some(placed(&out.backend, keep_card_failure(self.last.as_ref().and_then(|b| b.fallback), out.fallback)));
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
                let detail = format!("{e:?}");
                log::warn!("the speech helper did not confirm unloading: {}", crate::logging::text(detail));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_errors_become_words_not_debug_text() {
        use crate::i18n::{tr, tr_with};
        use voice_engine_client::EngineError as E;
        let name = "Whisper Large Turbo";
        assert_eq!(helper_error_text(&E::OutOfMemory, name), tr("transcribe.out_of_memory"));
        assert_eq!(helper_error_text(&E::ModelMissing { path: "/Users/someone/m.bin".into() }, name), tr_with("error.download_speech_model", &[("name", name)]));
        assert_eq!(helper_error_text(&E::BadModel, name), tr_with("download.corrupt", &[("name", name)]));
        let generic = tr("transcribe.helper_failed");
        for e in [
            E::CouldNotStart { message: "C:\\Users\\someone\\talaan-stt.exe".into() },
            E::Crashed { exit_code: None, signal: Some(6), detail: "abort()".into() },
            E::TimedOut,
            E::Cancelled,
            E::GpuFailed { fell_back: false },
            E::Internal { message: "x".into() },
        ] {
            let m = helper_error_text(&e, name);
            assert_eq!(m, generic);
            assert!(!m.contains("Users") && !m.contains("Crashed"), "{m}");
        }
    }

    #[test]
    fn a_card_failure_at_load_survives_the_remembered_failure_that_follows() {
        use voice_engine_stt::Fallback::*;
        assert_eq!(keep_card_failure(Some(CardFailed), Some(RememberedFailure)), Some(CardFailed));
        assert_eq!(keep_card_failure(Some(CardOutOfMemory), Some(RememberedFailure)), Some(CardOutOfMemory));
        // Anything else the request says wins: the card ran again (Try again), failed anew, or never had a path.
        assert_eq!(keep_card_failure(Some(CardFailed), None), None);
        assert_eq!(keep_card_failure(Some(CardFailed), Some(CardOutOfMemory)), Some(CardOutOfMemory));
        assert_eq!(keep_card_failure(Some(NoCardPath), Some(RememberedFailure)), Some(RememberedFailure));
        assert_eq!(keep_card_failure(None, Some(NoCardHelper)), Some(NoCardHelper));
    }

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
