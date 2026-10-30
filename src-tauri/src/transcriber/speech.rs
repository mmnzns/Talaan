//! Talaan's speech pipeline on voice-engine's runtimes: Whisper and the NVIDIA models in this process, voice
//! detection and speaker separation on the processor. The pipeline (chunking, echo, speakers, merging) stays Talaan's.

use std::path::PathBuf;

use voice_engine_models::{Kind, ModelHandle, ModelInfo};
use voice_engine_stt::{ChunkOptions, LoadOptions, SpeechBackend, SpeechError, Timing, Transcriber, Transcript};

use crate::i18n::{tr, tr_with};

use super::diarize::{Activity, SpeakerSplitter};
use super::engine::{ChunkResult, Engine, Piece};
use super::pipeline::Detector;
use super::vad::{Shape, Span, RATE};

/// Words for the user from a speech error (matched on its kind, never on text). The engine's own message, which
/// carries no path or speech but is English and technical, goes to the log only.
pub fn speech_error_text(e: &SpeechError, name: &str) -> String {
    match e {
        SpeechError::ModelMissing => tr_with("error.download_speech_model", &[("name", name)]),
        SpeechError::BadModel => tr_with("download.corrupt", &[("name", name)]),
        SpeechError::OutOfMemory => tr("transcribe.out_of_memory"),
        // Unsupported, TooLong (the pipeline cuts to the model's size), Internal: nothing the user can act on.
        other => {
            log::warn!("speech engine: {}", crate::logging::text(other));
            tr("transcribe.helper_failed")
        }
    }
}

pub struct EngineSpeech {
    inner: Box<dyn Transcriber>,
    label: String,
    kind: Kind,
    _model: ModelHandle, // keeps the files from being erased while loaded
}

impl EngineSpeech {
    /// `use_gpu`: whether the setting asks for the graphics card for this model (the caller already limited it to
    /// models that have a card path on this platform, so `fell_back` only means a real failure).
    pub fn load(model: &'static ModelInfo, handle: ModelHandle, use_gpu: bool) -> Result<Self, String> {
        let inner = voice_engine_stt::load(model, handle.dir(), LoadOptions { gpu: use_gpu, threads: None }).map_err(|e| speech_error_text(&e, model.name))?;
        Ok(EngineSpeech { inner, label: model.name.to_string(), kind: model.kind, _model: handle })
    }
}

impl Engine for EngineSpeech {
    fn transcribe(&mut self, samples: &[f32], language: Option<&str>) -> Result<ChunkResult, String> {
        let opts = ChunkOptions { language: language.map(str::to_string), timing: Timing::Segments, ..Default::default() };
        let transcript = self.inner.transcribe(samples, &opts).map_err(|e| speech_error_text(&e, &self.label))?;
        if transcript.stopped_early_at_ms.is_some() {
            log::info!("the speech model stopped early on a chunk (the engine decoded the rest again)");
        }
        Ok(to_chunk(transcript))
    }
    fn label(&self) -> &str {
        &self.label
    }
    fn chunking(&self) -> Shape {
        if self.kind == Kind::Whisper { Shape::whisper() } else { Shape::parakeet() }
    }
    fn backend(&self) -> Option<SpeechBackend> {
        Some(self.inner.backend())
    }
}

pub fn to_chunk(t: Transcript) -> ChunkResult {
    let pieces = t
        .segments
        .into_iter()
        .map(|s| Piece { start_ms: s.start_ms, end_ms: s.end_ms, text: s.text.trim().to_string() })
        .filter(|p| !p.text.is_empty() && !voice_engine_stt::is_noise(&p.text))
        .collect();
    ChunkResult { pieces, language: t.language }
}

/// The names of the two helper models in the messages ("... was damaged"), as the catalog calls them.
const VAD_NAME: &str = "Voice detection";
const SPEAKER_NAME: &str = "Speaker separation";

/// Voice detection: the engine's Silero settings are Talaan's (0.5, 250 ms, 300 ms, no padding).
pub struct EngineVad {
    dir: PathBuf,
    _model: ModelHandle,
}

impl EngineVad {
    pub fn new(handle: ModelHandle) -> Self {
        EngineVad { dir: handle.dir().to_path_buf(), _model: handle }
    }
}

impl Detector for EngineVad {
    fn detect(&mut self, samples: &[f32]) -> Result<Vec<Span>, String> {
        voice_engine_stt::vad::detect(&self.dir, samples).map(|s| to_spans(&s)).map_err(|e| speech_error_text(&e, VAD_NAME))
    }
}

pub fn to_spans(ms: &[(u64, u64)]) -> Vec<Span> {
    let at = |t: u64| (t as usize) * RATE / 1000;
    ms.iter().map(|&(a, b)| Span { start: at(a), end: at(b) }).collect()
}

pub struct EngineSplitter {
    inner: voice_engine_stt::diarize::Diarizer,
    _model: ModelHandle,
}

impl EngineSplitter {
    pub fn load(handle: ModelHandle) -> Result<Self, String> {
        let inner = voice_engine_stt::diarize::load(handle.dir(), None).map_err(|e| speech_error_text(&e, SPEAKER_NAME))?;
        Ok(EngineSplitter { inner, _model: handle })
    }
}

impl SpeakerSplitter for EngineSplitter {
    fn activity(&mut self, them: &[f32]) -> Result<Activity, String> {
        self.inner.run(them).map(|f| to_activity(&f)).map_err(|e| speech_error_text(&e, SPEAKER_NAME))
    }
}

pub fn to_activity(frames: &[voice_engine_stt::diarize::SpeakerFrame]) -> Activity {
    frames.iter().map(|f| f.speakers.to_vec()).collect()
}

/// A models store over `TALAAN_SMOKE_DATA/shared-models/<id>/<file>` for the ignored smoke tests; never the real
/// shared folder (`Models::open(Some(dir))` never moves anything in).
#[cfg(test)]
pub fn smoke_models() -> crate::models::Models {
    let data = PathBuf::from(std::env::var("TALAAN_SMOKE_DATA").expect("TALAAN_SMOKE_DATA: a scratch folder holding shared-models/<id>/<file>"));
    crate::models::Models::open(Some(&data)).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use voice_engine_stt::diarize::SpeakerFrame;
    use voice_engine_stt::{Segment, Transcript};

    #[test]
    fn engine_segments_become_pieces() {
        let t = Transcript {
            segments: vec![
                Segment { start_ms: 0, end_ms: 900, text: " Hello there.".into(), words: vec![] },
                Segment { start_ms: 900, end_ms: 1000, text: " [BLANK_AUDIO]".into(), words: vec![] },
            ],
            language: Some("en".into()),
            stopped_early_at_ms: None,
        };
        let r = to_chunk(t);
        assert_eq!(r.pieces, vec![Piece { start_ms: 0, end_ms: 900, text: "Hello there.".into() }]);
        assert_eq!(r.language.as_deref(), Some("en"));
    }

    #[test]
    fn speech_errors_become_words_not_debug_text() {
        let name = "Whisper Small";
        assert_eq!(speech_error_text(&SpeechError::ModelMissing, name), tr_with("error.download_speech_model", &[("name", name)]));
        assert_eq!(speech_error_text(&SpeechError::BadModel, name), tr_with("download.corrupt", &[("name", name)]));
        assert_eq!(speech_error_text(&SpeechError::OutOfMemory, name), tr("transcribe.out_of_memory"));
        for e in [SpeechError::Unsupported("x".into()), SpeechError::TooLong { max_secs: 40 }, SpeechError::Internal("whisper: -3".into())] {
            let m = speech_error_text(&e, name);
            assert_eq!(m, tr("transcribe.helper_failed"));
            assert!(!m.contains("whisper") && !m.contains("Internal"), "{m}");
        }
    }

    #[test]
    fn voice_spans_in_ms_become_sample_spans() {
        assert_eq!(to_spans(&[(0, 500), (1000, 1250)]), vec![Span { start: 0, end: 8_000 }, Span { start: 16_000, end: 20_000 }]);
    }

    #[test]
    fn speaker_frames_become_talaan_activity() {
        let mut f = SpeakerFrame { start_ms: 0, end_ms: 10, speakers: [0.0; 8] };
        f.speakers[1] = 0.9;
        let a = to_activity(&[f]);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].len(), 8);
        assert_eq!(a[0][1], 0.9);
    }

    #[test]
    fn parakeet_chunks_fit_the_models_40_second_limit() {
        assert!(Shape::parakeet().max_len <= 40 * RATE);
    }
}
