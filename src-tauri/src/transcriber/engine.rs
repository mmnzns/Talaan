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
        EngineKind::Parakeet | EngineKind::ParakeetUnified | EngineKind::Nemotron => Ok(Box::new(ParakeetEngine::load(dir, model.name, model.engine)?)),
        EngineKind::Whisper => Ok(Box::new(WhisperEngine::load(&dir.join(model.files[0].name), model.name, use_gpu)?)),
        EngineKind::Llm | EngineKind::Diarization | EngineKind::Embedding => Err("not a speech model".into()),
    }
}

fn threads() -> usize {
    thread_count(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4), cfg!(windows))
}

/// Mac: all cores, 2..=8 (the original formula). Windows: three quarters of the logical cores, 2..=12. Measured on a
/// 16-thread Ryzen: 12 threads is fastest and 16 is 2-3x slower (same finding as Parlato).
fn thread_count(logical: usize, windows: bool) -> usize {
    if windows {
        (logical * 3 / 4).clamp(2, 12)
    } else {
        logical.clamp(2, 8)
    }
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

/// The NVIDIA models: Parakeet TDT (v2, v3, Ultra), Parakeet Unified and Nemotron. Same chunking, same
/// timestamps; they differ only in how they load and decode.
enum NvidiaModel {
    Tdt(parakeet_rs::ParakeetTDT),
    Unified(parakeet_rs::ParakeetUnified),
    /// With the language prompts the model accepts ("en-US", "fr-FR"...).
    Nemotron(Box<parakeet_rs::Nemotron>, Vec<&'static str>),
}

pub struct ParakeetEngine {
    model: NvidiaModel,
    label: String,
}

impl ParakeetEngine {
    pub fn load(dir: &Path, label: &str, kind: EngineKind) -> Result<Self, String> {
        let config = parakeet_rs::ExecutionConfig::new().with_intra_threads(threads());
        let failed = |e: parakeet_rs::Error| format!("couldn't load the speech model: {e}");
        let model = match kind {
            EngineKind::ParakeetUnified => NvidiaModel::Unified(parakeet_rs::ParakeetUnified::from_pretrained(dir, Some(config)).map_err(failed)?),
            EngineKind::Nemotron => {
                let handle = parakeet_rs::NemotronHandle::from_pretrained(dir, Some(config)).map_err(failed)?;
                NvidiaModel::Nemotron(Box::new(parakeet_rs::Nemotron::from_shared(&handle)), handle.available_languages())
            }
            _ => NvidiaModel::Tdt(parakeet_rs::ParakeetTDT::from_pretrained(dir, Some(config)).map_err(failed)?),
        };
        Ok(ParakeetEngine { model, label: label.into() })
    }

    /// Nemotron takes a language hint ("fr" → its "fr-FR" prompt); unknown or none means it detects.
    fn set_language(&mut self, language: Option<&str>) {
        if let NvidiaModel::Nemotron(m, langs) = &mut self.model {
            if langs.is_empty() {
                return; // the English-only variant takes no prompt
            }
            let locale = language.and_then(|code| langs.iter().copied().find(|l| l.split('-').next() == Some(code)));
            let _ = m.set_target_lang(locale.unwrap_or("auto"));
        }
    }
}

impl ParakeetEngine {
    /// Half a second of silence on both sides: Parakeet often returns nothing for audio that starts or
    /// ends right on speech (seen on real calls).
    fn run(&mut self, samples: &[f32]) -> Result<Vec<Piece>, String> {
        use parakeet_rs::Transcriber;
        const PAD: usize = 8_000;
        let mut audio = vec![0.0f32; PAD];
        audio.extend_from_slice(samples);
        audio.extend(std::iter::repeat_n(0.0, PAD));
        let mode = Some(parakeet_rs::TimestampMode::Sentences);
        let result = match &mut self.model {
            NvidiaModel::Tdt(m) => m.transcribe_samples(audio, 16_000, 1, mode),
            NvidiaModel::Unified(m) => m.transcribe_samples(audio, 16_000, 1, mode),
            NvidiaModel::Nemotron(m, _) => m.transcribe_audio_with_timestamps(&audio, mode),
        }
        .map_err(|e| format!("transcription failed: {e}"))?;
        let len_ms = samples.len() as u64 * 1000 / 16_000;
        let shift = |t: f32| ((t.max(0.0) * 1000.0) as u64).saturating_sub(500).min(len_ms);
        Ok(result
            .tokens
            .into_iter()
            .filter(|t| !is_noise(&t.text))
            .map(|t| Piece { start_ms: shift(t.start), end_ms: shift(t.end.max(t.start)), text: t.text.trim().to_string() })
            .collect())
    }
}

impl Engine for ParakeetEngine {
    /// Parakeet sometimes stops decoding partway through a chunk (seen when a chunk starts mid-sentence).
    /// The chunk is all speech by construction, so when output ends well before the audio does, the rest is
    /// transcribed again from where it stopped.
    fn transcribe(&mut self, samples: &[f32], language: Option<&str>) -> Result<ChunkResult, String> {
        self.set_language(language);
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
        // Flash attention only pays off on Metal; on CPU it costs 10-15%.
        p.flash_attn(cfg!(target_os = "macos") && use_gpu);
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
    fn windows_thread_count_is_three_quarters_clamped() {
        for (logical, want) in [(16, 12), (8, 6), (4, 3), (2, 2), (1, 2), (32, 12)] {
            assert_eq!(thread_count(logical, true), want, "{logical} cores");
        }
    }

    #[test]
    fn mac_thread_count_is_all_cores_clamped() {
        for (logical, want) in [(16, 8), (10, 8), (8, 8), (4, 4), (2, 2), (1, 2)] {
            assert_eq!(thread_count(logical, false), want, "{logical} cores");
        }
    }

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
mod download_smoke {
    /// Downloads the given models into a scratch folder (checksums checked), loads each and transcribes a
    /// recording. Never touches the app's own data folder.
    /// `TALAAN_SMOKE_DATA=<scratch> TALAAN_SMOKE_MEETING=<meeting dir> TALAAN_SMOKE_IDS=a,b cargo test --release new_models_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn new_models_smoke() {
        use super::super::download::download_model;
        use super::super::models::{find, VAD};
        use super::super::vad::{read_track, shape, Vad, RATE};
        use std::sync::atomic::AtomicBool;
        let data = std::path::PathBuf::from(std::env::var("TALAAN_SMOKE_DATA").unwrap());
        let meeting = std::path::PathBuf::from(std::env::var("TALAAN_SMOKE_MEETING").unwrap());
        let get = |id: &str| {
            let m = find(id).unwrap();
            let dir = data.join("models").join(id);
            download_model(m, &dir, &AtomicBool::new(false), &mut |_| {}).unwrap_or_else(|e| panic!("{id}: {}", e.user_message()));
            (m, dir)
        };
        let (_, vad_dir) = get(VAD.id);
        let mut vad = Vad::load(&vad_dir.join(VAD.files[0].name)).unwrap();
        let me = read_track(&meeting.join("me.wav")).unwrap();
        let raw = vad.detect(&me).unwrap();
        for id in std::env::var("TALAAN_SMOKE_IDS").unwrap().split(',') {
            let (m, dir) = get(id);
            let t = std::time::Instant::now();
            let mut engine = super::load(m, &dir, true).unwrap_or_else(|e| panic!("{id}: {e}"));
            let load_s = t.elapsed().as_secs_f32();
            let chunks = shape(&raw, &me, engine.chunking());
            let total: usize = chunks.iter().map(|c| c.len()).sum();
            let t = std::time::Instant::now();
            let mut text = vec![];
            for c in &chunks {
                text.extend(engine.transcribe(&me[c.start..c.end], Some("en")).unwrap().pieces.into_iter().map(|p| p.text));
            }
            eprintln!("== {id}: load {load_s:.1}s, {:.0}s of speech in {:.1}s\n   {}", total as f32 / RATE as f32, t.elapsed().as_secs_f32(), text.join(" "));
            assert!(!text.is_empty(), "{id} heard nothing");
        }
    }
}

#[cfg(test)]
mod probe {
    #[test]
    #[ignore]
    fn parakeet_probe() {
        use super::super::vad::{read_track, shape, Shape, Vad};
        let dir = std::path::PathBuf::from(std::env::var("TALAAN_SMOKE_MEETING").unwrap());
        let me = read_track(&dir.join("me.wav")).unwrap();
        let data = dirs::data_dir().unwrap().join("Talaan");
        let mut vad = Vad::load(&data.join("models/vad/ggml-silero-v6.2.0.bin")).unwrap();
        let spans = shape(&vad.detect(&me).unwrap(), &me, Shape::parakeet());
        let mut m = super::ParakeetEngine::load(&data.join("models/parakeet-v3"), "Parakeet", crate::transcriber::models::EngineKind::Parakeet).unwrap();
        let _ = spans;
        let f = |a: f32| (a * 16000.0) as usize;
        let mut padded = vec![0.0f32; 8000];
        padded.extend_from_slice(&me[f(75.3)..f(98.8)]);
        let cases: Vec<(String, Vec<f32>)> = vec![
            ("75.3-98.8".into(), me[f(75.3)..f(98.8)].to_vec()),
            ("pad+75.3-98.8".into(), padded),
            ("75.3-87".into(), me[f(75.3)..f(87.0)].to_vec()),
            ("87-98.8".into(), me[f(87.0)..f(98.8)].to_vec()),
            ("74-100".into(), me[f(74.0)..f(100.0)].to_vec()),
        ];
        for (name, chunk) in cases {
            let peak = chunk.iter().fold(0f32, |a, b| a.max(b.abs()));
            let r = super::Engine::transcribe(&mut m, &chunk, None).unwrap();
            eprintln!("chunk {name} peak {peak:.2}: {:?}", r.pieces.iter().map(|p| format!("[{}] {}", p.start_ms, p.text)).collect::<Vec<_>>());
        }
    }
}
