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

/// The 14 languages Cohere Transcribe writes (ISO 639-1), from the engine's release notes. It cannot detect the
/// language, so the meeting's language has to be one of these (Auto means English). Also in `src/data/settings.ts`.
pub const COHERE_LANGUAGES: [&str; 14] = ["ar", "de", "el", "en", "es", "fr", "it", "ja", "ko", "nl", "pl", "pt", "vi", "zh"];

/// What a model can and can't do that the pipeline has to work around (read from the catalog, never from an id).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Traits {
    /// The model tells the language by itself. Cohere Transcribe doesn't: it must be given one.
    pub detects_language: bool,
    /// The languages it can write, when that is a short list (only for a model that can't detect).
    pub languages: Option<&'static [&'static str]>,
    /// Gives words with times. Without them a chunk comes back as one block of text.
    pub word_times: bool,
}

pub fn traits(model: &ModelInfo) -> Traits {
    match model.kind {
        Kind::Cohere => Traits { detects_language: false, languages: Some(&COHERE_LANGUAGES), word_times: model.word_times },
        _ => Traits { detects_language: true, languages: None, word_times: model.word_times },
    }
}

/// Chunk shape for a model: its own limits (Whisper's 30 s window, Cohere's 35 s), Parakeet's otherwise.
pub fn shape_for(kind: Kind) -> Shape {
    match kind {
        Kind::Whisper => Shape::whisper(),
        Kind::Cohere => Shape::cohere(),
        _ => Shape::parakeet(),
    }
}

pub struct EngineSpeech {
    inner: Box<dyn Transcriber>,
    label: String,
    kind: Kind,
    traits: Traits,
    _model: ModelHandle, // keeps the files from being erased while loaded
}

impl EngineSpeech {
    /// `use_gpu`: whether the setting asks for the graphics card for this model (the caller already limited it to
    /// models that have a card path on this platform, so a `fallback` that says the card failed is a real failure).
    pub fn load(model: &'static ModelInfo, handle: ModelHandle, use_gpu: bool) -> Result<Self, String> {
        let inner = voice_engine_stt::load(model, handle.dir(), LoadOptions { gpu: use_gpu, threads: None }).map_err(|e| speech_error_text(&e, model.name))?;
        Ok(EngineSpeech { inner, label: model.name.to_string(), kind: model.kind, traits: traits(model), _model: handle })
    }
}

impl Engine for EngineSpeech {
    fn transcribe(&mut self, samples: &[f32], language: Option<&str>) -> Result<ChunkResult, String> {
        let opts = ChunkOptions { language: language.map(str::to_string), timing: Timing::Segments, ..Default::default() };
        let transcript = self.inner.transcribe(samples, &opts).map_err(|e| speech_error_text(&e, &self.label))?;
        if transcript.stopped_early_at_ms.is_some() {
            log::info!("the speech model stopped early on a chunk (the engine decoded the rest again)");
        }
        Ok(to_chunk(transcript, !self.traits.word_times))
    }
    fn label(&self) -> &str {
        &self.label
    }
    fn chunking(&self) -> Shape {
        shape_for(self.kind)
    }
    fn detects_language(&self) -> bool {
        self.traits.detects_language
    }
    fn languages(&self) -> Option<&'static [&'static str]> {
        self.traits.languages
    }
    fn backend(&self) -> Option<SpeechBackend> {
        Some(self.inner.backend())
    }
}

/// `text_only`: the model gives no word times (`ModelInfo::word_times` is false), so a chunk is one block of text.
pub fn to_chunk(t: Transcript, text_only: bool) -> ChunkResult {
    let pieces = t
        .segments
        .into_iter()
        .map(|s| Piece { start_ms: s.start_ms, end_ms: s.end_ms, text: s.text.trim().to_string() })
        .filter(|p| !p.text.is_empty() && !voice_engine_stt::is_noise(&p.text))
        .flat_map(|p| if text_only { split_sentences(p) } else { vec![p] })
        .collect();
    ChunkResult { pieces, language: t.language }
}

/// A text-only model returns a whole chunk (up to 35 s, maybe several speakers) as one piece. Talaan's speaker
/// naming and echo removal work per sentence, so the block is cut at sentence ends and the chunk's time is shared
/// out by length (a sentence twice as long gets twice the time). The times are an estimate inside the chunk: good
/// enough to put a sentence with the right speaker and the right minute, not to the second.
pub fn split_sentences(p: Piece) -> Vec<Piece> {
    let mut parts: Vec<String> = vec![];
    let mut cur = String::new();
    let mut chars = p.text.chars().peekable();
    while let Some(c) = chars.next() {
        cur.push(c);
        let cjk_end = matches!(c, '。' | '！' | '？');
        let end = matches!(c, '.' | '!' | '?' | '…' | '؟') && chars.peek().is_none_or(|n| n.is_whitespace());
        if cjk_end || end {
            // A run of closers ("?!", "...") stays with its sentence.
            while chars.peek().is_some_and(|n| matches!(n, '.' | '!' | '?' | '…' | '。' | '！' | '？' | '"' | '”' | '’' | ')')) {
                cur.push(chars.next().unwrap());
            }
            parts.push(std::mem::take(&mut cur));
        }
    }
    parts.push(cur);
    let parts: Vec<String> = parts.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    if parts.len() <= 1 {
        return vec![p];
    }
    let total: usize = parts.iter().map(|s| s.chars().count()).sum::<usize>().max(1);
    let span = p.end_ms.saturating_sub(p.start_ms);
    let (mut done, last) = (0usize, parts.len() - 1);
    parts
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let start_ms = p.start_ms + span * done as u64 / total as u64;
            done += text.chars().count();
            let end_ms = if i == last { p.end_ms } else { p.start_ms + span * done as u64 / total as u64 };
            Piece { start_ms, end_ms, text }
        })
        .collect()
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
                Segment { start_ms: 0, end_ms: 900, text: " Hello there.".into(), words: vec![], starts_word: true },
                Segment { start_ms: 900, end_ms: 1000, text: " [BLANK_AUDIO]".into(), words: vec![], starts_word: true },
            ],
            language: Some("en".into()),
            stopped_early_at_ms: None,
        };
        let r = to_chunk(t, false);
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

    #[test]
    fn every_model_gets_a_chunk_shape_that_fits_its_limit() {
        for m in voice_engine_models::catalog::speech() {
            let shape = shape_for(m.kind);
            if let Some(secs) = m.chunk_secs {
                assert!(shape.max_len <= secs as usize * RATE, "{}: {} s chunks", m.id, shape.max_len / RATE);
            }
        }
        assert_eq!(shape_for(Kind::Cohere).max_len, 35 * RATE);
        assert_eq!(shape_for(Kind::Whisper).max_len, Shape::whisper().max_len);
    }

    /// Word times gate: Talaan reads what a model can and can't do from `ModelInfo::word_times` and its kind.
    #[test]
    fn a_model_without_word_times_is_text_only_and_the_others_are_not() {
        let cohere = traits(voice_engine_models::catalog::find("cohere-transcribe").unwrap());
        assert!(!cohere.word_times && !cohere.detects_language);
        assert_eq!(cohere.languages.map(|l| l.len()), Some(14));
        for m in voice_engine_models::catalog::speech().filter(|m| m.word_times) {
            let t = traits(m);
            assert!(t.word_times && t.detects_language && t.languages.is_none(), "{}", m.id);
        }
        // The same rule decides whether a chunk's block of text is cut into sentences.
        let block = || Transcript {
            segments: vec![Segment { start_ms: 0, end_ms: 10_000, text: "First one. Second one is longer. Third?".into(), words: vec![], starts_word: true }],
            language: None,
            stopped_early_at_ms: None,
        };
        assert_eq!(to_chunk(block(), false).pieces.len(), 1);
        assert_eq!(to_chunk(block(), !cohere.word_times).pieces.len(), 3);
    }

    #[test]
    fn a_block_of_text_is_cut_at_sentence_ends_and_the_time_shared_by_length() {
        let p = Piece { start_ms: 1_000, end_ms: 11_000, text: "Aaaa. Bbbbbbbbb! Cc?".into() };
        let out = split_sentences(p);
        assert_eq!(out.iter().map(|p| p.text.as_str()).collect::<Vec<_>>(), ["Aaaa.", "Bbbbbbbbb!", "Cc?"]);
        assert_eq!(out[0].start_ms, 1_000);
        assert_eq!(out[2].end_ms, 11_000);
        assert!(out.windows(2).all(|w| w[0].end_ms == w[1].start_ms));
        assert!(out[1].end_ms - out[1].start_ms > out[0].end_ms - out[0].start_ms);
    }

    #[test]
    fn splitting_leaves_decimals_and_single_sentences_alone() {
        let one = Piece { start_ms: 0, end_ms: 5_000, text: "It costs 3.5 million dollars".into() };
        assert_eq!(split_sentences(one.clone()), vec![one]);
        let cjk = split_sentences(Piece { start_ms: 0, end_ms: 4_000, text: "你好。再见！".into() });
        assert_eq!(cjk.iter().map(|p| p.text.as_str()).collect::<Vec<_>>(), ["你好。", "再见！"]);
    }
}
