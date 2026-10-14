//! Speech engines behind one small trait. Each call gets one chunk of speech (16 kHz mono) and returns
//! timed pieces relative to that chunk; the pipeline places them on the recording clock.

use super::models::{EngineKind, ModelInfo};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct ChunkResult {
    pub pieces: Vec<Piece>,
    /// Language the engine detected, when it does detection (Whisper).
    pub language: Option<String>,
}

pub trait Engine: Send {
    /// `language`: ISO 639-1 code, or None to let the engine decide.
    fn transcribe(&mut self, samples: &[f32], language: Option<&str>) -> Result<ChunkResult, String>;
    /// Short label stored with the transcript ("Parakeet", "Whisper Large Turbo").
    fn label(&self) -> &str;
    /// How speech is cut into chunks for this engine.
    fn chunking(&self) -> super::vad::Shape;
}

pub fn load(model: &ModelInfo, dir: &Path, use_gpu: bool) -> Result<Box<dyn Engine>, String> {
    match model.engine {
        EngineKind::Parakeet => Ok(Box::new(ParakeetEngine::load(dir, model.name)?)),
        EngineKind::Whisper => Ok(Box::new(WhisperEngine::load(&dir.join(model.files[0].name), model.name, use_gpu)?)),
    }
}

fn threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8)
}

/// Text that engines produce for non-speech ("[BLANK_AUDIO]", "(music)", "*laughs*") or nothing at all.
pub fn is_noise(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return true;
    }
    let wrapped = |o: char, c: char| t.starts_with(o) && t.ends_with(c);
    wrapped('[', ']') || wrapped('(', ')') || wrapped('*', '*') || t.chars().all(|c| !c.is_alphanumeric())
}

pub struct ParakeetEngine {
    model: parakeet_rs::ParakeetTDT,
    label: String,
}

impl ParakeetEngine {
    pub fn load(dir: &Path, label: &str) -> Result<Self, String> {
        let config = parakeet_rs::ExecutionConfig::new().with_intra_threads(threads());
        let model = parakeet_rs::ParakeetTDT::from_pretrained(dir, Some(config)).map_err(|e| format!("couldn't load the speech model: {e}"))?;
        Ok(ParakeetEngine { model, label: label.into() })
    }
}

impl ParakeetEngine {
    fn run(&mut self, samples: &[f32]) -> Result<Vec<Piece>, String> {
        use parakeet_rs::Transcriber;
        let result = self
            .model
            .transcribe_samples(samples.to_vec(), 16_000, 1, Some(parakeet_rs::TimestampMode::Sentences))
            .map_err(|e| format!("transcription failed: {e}"))?;
        Ok(result
            .tokens
            .into_iter()
            .filter(|t| !is_noise(&t.text))
            .map(|t| Piece { start_ms: (t.start.max(0.0) * 1000.0) as u64, end_ms: (t.end.max(t.start).max(0.0) * 1000.0) as u64, text: t.text.trim().to_string() })
            .collect())
    }
}

impl Engine for ParakeetEngine {
    /// Parakeet sometimes stops decoding partway through a chunk (seen when a chunk starts mid-sentence).
    /// The chunk is all speech by construction, so when output ends well before the audio does, the rest is
    /// transcribed again from where it stopped.
    fn transcribe(&mut self, samples: &[f32], _language: Option<&str>) -> Result<ChunkResult, String> {
        let total_ms = samples.len() as u64 * 1000 / 16_000;
        let mut pieces: Vec<Piece> = vec![];
        let mut from_ms = 0u64;
        for _ in 0..8 {
            let from = (from_ms * 16) as usize;
            let mut got = self.run(&samples[from..])?;
            got.iter_mut().for_each(|p| {
                p.start_ms += from_ms;
                p.end_ms += from_ms;
            });
            let reached = got.last().map(|p| p.end_ms).unwrap_or(from_ms);
            let progressed = reached > from_ms + 500;
            pieces.extend(got);
            if !progressed || total_ms.saturating_sub(reached) < RESUME_GAP_MS {
                break;
            }
            from_ms = reached;
        }
        Ok(ChunkResult { pieces, language: None })
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn chunking(&self) -> super::vad::Shape {
        super::vad::Shape::parakeet()
    }
}

/// Audio left after the last word that counts as "decoding stopped early".
const RESUME_GAP_MS: u64 = 2_000;

pub struct WhisperEngine {
    // The state holds ~400 MB of buffers; one per engine, reused for every chunk.
    state: whisper_rs::WhisperState,
    _ctx: whisper_rs::WhisperContext,
    label: String,
}

impl WhisperEngine {
    pub fn load(file: &Path, label: &str, use_gpu: bool) -> Result<Self, String> {
        let mut p = whisper_rs::WhisperContextParameters::default();
        p.use_gpu(use_gpu);
        p.flash_attn(use_gpu);
        let ctx = whisper_rs::WhisperContext::new_with_params(file, p).map_err(|e| format!("couldn't load the speech model: {e}"))?;
        let state = ctx.create_state().map_err(|e| format!("couldn't load the speech model: {e}"))?;
        Ok(WhisperEngine { state, _ctx: ctx, label: label.into() })
    }
}

impl Engine for WhisperEngine {
    fn transcribe(&mut self, samples: &[f32], language: Option<&str>) -> Result<ChunkResult, String> {
        let state = &mut self.state;
        let mut params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(threads() as i32);
        params.set_language(Some(language.unwrap_or("auto")));
        // Each chunk stands alone: carrying text between chunks is how Whisper starts repeating itself.
        params.set_no_context(true);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        // Whisper needs at least 1 s; pad short chunks with silence.
        let mut audio = samples.to_vec();
        if audio.len() < 16_000 + 1_600 {
            audio.resize(16_000 + 1_600, 0.0);
        }
        state.full(params, &audio).map_err(|e| format!("transcription failed: {e}"))?;
        let language = whisper_rs::get_lang_str(state.full_lang_id_from_state()).map(str::to_string);
        let mut pieces = vec![];
        for i in 0..state.full_n_segments() {
            let Some(seg) = state.get_segment(i) else { continue };
            let text = seg.to_str_lossy().map(|t| t.trim().to_string()).unwrap_or_default();
            if is_noise(&text) || seg.no_speech_probability() > 0.8 {
                continue;
            }
            // Centiseconds.
            let start_ms = (seg.start_timestamp().max(0) * 10) as u64;
            let end_ms = ((seg.end_timestamp().max(0) * 10) as u64).min(samples.len() as u64 * 1000 / 16_000).max(start_ms);
            pieces.push(Piece { start_ms, end_ms, text });
        }
        Ok(ChunkResult { pieces, language })
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn chunking(&self) -> super::vad::Shape {
        super::vad::Shape::whisper()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_markers_are_not_speech() {
        for t in ["", "  ", "[BLANK_AUDIO]", "(music)", "*laughs*", "...", "♪"] {
            assert!(is_noise(t), "{t:?}");
        }
        for t in ["Okay.", "Sounds good", "(inaudible) let's go"] {
            assert!(!is_noise(t), "{t:?}");
        }
    }

    /// Real-time factor of each downloaded model on 60 s of a real meeting.
    /// `TALAAN_SMOKE_MEETING=<meeting dir> cargo test --release engine_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn engine_smoke() {
        use super::super::models::{ModelStore, CATALOG};
        use super::super::vad::{read_track, shape, Vad, RATE};
        let dir = std::path::PathBuf::from(std::env::var("TALAAN_SMOKE_MEETING").unwrap());
        let data = dirs::data_dir().unwrap().join("Talaan");
        let store = ModelStore::new(&data, false);
        let mut vad = Vad::load(&data.join("models/vad/ggml-silero-v6.2.0.bin")).unwrap();
        let them = read_track(&dir.join("them.wav")).unwrap();
        let raw = vad.detect(&them).unwrap();
        for m in CATALOG {
            let Some(d) = store.locate(m).dir().map(|d| d.to_path_buf()) else { continue };
            let t = std::time::Instant::now();
            let mut engine = load(m, &d, true).unwrap();
            let load_s = t.elapsed().as_secs_f32();
            let chunks = shape(&raw, &them, engine.chunking());
            let total: usize = chunks.iter().map(|c| c.len()).sum();
            let t = std::time::Instant::now();
            let mut out = vec![];
            for c in &chunks {
                let r = engine.transcribe(&them[c.start..c.end], None).unwrap();
                for p in r.pieces {
                    out.push(format!("[{:.1}s] {}", (c.start_ms() + p.start_ms) as f32 / 1000.0, p.text));
                }
            }
            let secs = t.elapsed().as_secs_f32();
            eprintln!("== {}: load {load_s:.1}s, {} chunks, {:.0}s of speech in {secs:.1}s (x{:.0} real time)", m.id, chunks.len(), total as f32 / RATE as f32, total as f32 / RATE as f32 / secs);
            for l in out.iter().take(40) {
                eprintln!("   {l}");
            }
        }
    }
}

#[cfg(test)]
mod probe {
    #[test]
    #[ignore]
    fn parakeet_probe() {
        use super::super::vad::read_track;
        let dir = std::path::PathBuf::from(std::env::var("TALAAN_SMOKE_MEETING").unwrap());
        let them = read_track(&dir.join("them.wav")).unwrap();
        let data = dirs::data_dir().unwrap().join("Talaan/models/parakeet-v3");
        let mut m = super::ParakeetEngine::load(&data, "Parakeet").unwrap();
        for (a, b) in [(42.97f32, 55.59f32), (45.5, 55.59), (47.0, 55.59), (40.0, 58.0), (42.97, 47.0)] {
            let chunk = &them[(a * 16000.0) as usize..(b * 16000.0) as usize];
            let r = super::Engine::transcribe(&mut m, chunk, None).unwrap();
            eprintln!("{a}-{b}: {:?}", r.pieces.iter().map(|p| format!("[{}] {}", p.start_ms, p.text)).collect::<Vec<_>>());
        }
    }
}
