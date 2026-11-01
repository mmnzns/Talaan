//! Speech engines behind one small trait. Each call gets one chunk of speech (16 kHz mono) and returns
//! timed pieces relative to that chunk; the pipeline places them on the recording clock.

use voice_engine_stt::SpeechBackend;

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
    /// Where the model runs (the graphics card or the processor), for engines that know. Others say nothing.
    fn backend(&self) -> Option<SpeechBackend> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::speech::{smoke_models, EngineSpeech, EngineVad};
    use super::super::vad::{read_track, shape, RATE};
    use super::*;
    use crate::models::VAD;
    use super::super::pipeline::Detector;

    /// Real-time factor of each downloaded model on a real meeting's Them track.
    /// `TALAAN_SMOKE_DATA=<scratch> TALAAN_SMOKE_MEETING=<meeting dir> cargo test --release engine_smoke -- --ignored --nocapture`
    /// where `<scratch>/shared-models/<id>/<file>` holds the models.
    #[test]
    #[ignore]
    fn engine_smoke() {
        let dir = std::path::PathBuf::from(std::env::var("TALAAN_SMOKE_MEETING").unwrap());
        let models = smoke_models();
        let mut vad = EngineVad::new(models.hold(VAD).unwrap());
        let them = read_track(&dir.join("them.wav")).unwrap();
        let raw = vad.detect(&them).unwrap();
        for m in voice_engine_models::catalog::speech() {
            let Ok(handle) = models.hold(m.id) else { continue };
            let t = std::time::Instant::now();
            let mut engine = EngineSpeech::load(m, handle, true).unwrap();
            let load_s = t.elapsed().as_secs_f32();
            let chunks = shape(&raw, &them, engine.chunking());
            let total: usize = chunks.iter().map(|c| c.len()).sum();
            let t = std::time::Instant::now();
            let mut pieces = 0;
            for c in &chunks {
                pieces += engine.transcribe(&them[c.start..c.end], None).unwrap().pieces.len();
            }
            let secs = t.elapsed().as_secs_f32();
            eprintln!(
                "== {}: load {load_s:.1}s ({:?}), {} chunks, {pieces} pieces, {:.0}s of speech in {secs:.1}s (x{:.0} real time)",
                m.id,
                engine.backend(),
                chunks.len(),
                total as f32 / RATE as f32,
                total as f32 / RATE as f32 / secs
            );
        }
    }

    /// Downloads the given models into a scratch folder (checksums checked), loads each and transcribes a
    /// recording. Never touches the app's own data folder.
    /// `TALAAN_SMOKE_DATA=<scratch> TALAAN_SMOKE_MEETING=<meeting dir> TALAAN_SMOKE_IDS=a,b cargo test --release new_models_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn new_models_smoke() {
        use std::sync::atomic::AtomicBool;
        let meeting = std::path::PathBuf::from(std::env::var("TALAAN_SMOKE_MEETING").unwrap());
        let models = smoke_models();
        for id in std::env::var("TALAAN_SMOKE_IDS").unwrap().split(',').chain(std::iter::once("vad")) {
            models.store().download(id, &AtomicBool::new(false), &mut |_| {}).unwrap_or_else(|e| panic!("{id}: {e}"));
        }
        let mut vad = EngineVad::new(models.hold(VAD).unwrap());
        let me = read_track(&meeting.join("me.wav")).unwrap();
        let raw = vad.detect(&me).unwrap();
        for id in std::env::var("TALAAN_SMOKE_IDS").unwrap().split(',') {
            let m = voice_engine_models::catalog::find(id).unwrap();
            let t = std::time::Instant::now();
            let mut engine = EngineSpeech::load(m, models.hold(id).unwrap(), true).unwrap_or_else(|e| panic!("{id}: {e}"));
            let load_s = t.elapsed().as_secs_f32();
            let chunks = shape(&raw, &me, engine.chunking());
            let total: usize = chunks.iter().map(|c| c.len()).sum();
            let t = std::time::Instant::now();
            let mut words = 0;
            for c in &chunks {
                words += engine.transcribe(&me[c.start..c.end], Some("en")).unwrap().pieces.iter().map(|p| p.text.split_whitespace().count()).sum::<usize>();
            }
            eprintln!("== {id}: load {load_s:.1}s, {:.0}s of speech in {:.1}s, {words} words", total as f32 / RATE as f32, t.elapsed().as_secs_f32());
            assert!(words > 0, "{id} heard nothing");
        }
    }
}
