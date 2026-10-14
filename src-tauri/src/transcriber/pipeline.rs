//! One meeting, start to finish: read both tracks, find speech, transcribe it chunk by chunk, remove echo,
//! merge into lines and settle the language. No Tauri or storage types beyond the transcript line.

use super::echo::remove_echo;
use super::engine::Engine;
use super::merge::{merge, Timed, Track};
use super::vad::{read_track, shape, Span, Vad};
use crate::storage::model::TranscriptLine;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Finds speech in a track (the real one is Silero; tests use a fake).
pub trait Detector {
    fn detect(&mut self, samples: &[f32]) -> Result<Vec<Span>, String>;
}

impl Detector for Vad {
    fn detect(&mut self, samples: &[f32]) -> Result<Vec<Span>, String> {
        Vad::detect(self, samples)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub lines: Vec<TranscriptLine>,
    /// ISO 639-1, or None when it couldn't be told.
    pub language: Option<String>,
    pub engine: String,
    pub echo_removed: usize,
    pub speech_ms: u64,
}

#[derive(Debug, PartialEq)]
pub enum PipelineError {
    Cancelled,
    Failed(String),
}

impl From<String> for PipelineError {
    fn from(e: String) -> Self {
        PipelineError::Failed(e)
    }
}

/// `language`: the meeting language setting (None = detect). `progress` gets 0..1.
pub fn run(
    dir: &Path,
    engine: &mut dyn Engine,
    detector: &mut dyn Detector,
    language: Option<&str>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Result<Output, PipelineError> {
    // Tracks that are missing (an imported file has only one) are skipped.
    let mut tracks = vec![];
    for (track, name) in [(Track::Me, "me.wav"), (Track::Them, "them.wav")] {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        let samples = read_track(&path).map_err(|e| format!("couldn't read {name}: {e}"))?;
        let raw = detector.detect(&samples)?;
        let spans = shape(&raw, &samples, engine.chunking());
        tracks.push((track, samples, spans, raw));
    }
    let total: usize = tracks.iter().flat_map(|(_, _, s, _)| s.iter().map(|s| s.len())).sum();

    // Whisper detects the language per call; settle it once on the longest chunk so a meeting never switches
    // language halfway, then use it for every chunk.
    let mut language = language.map(str::to_string);
    let longest = tracks.iter().enumerate().flat_map(|(t, (_, _, spans, _))| spans.iter().enumerate().map(move |(i, s)| (s.len(), t, i))).max();
    let mut first = None;
    if language.is_none() {
        if let Some((_, t, i)) = longest {
            let (_, samples, spans, _) = &tracks[t];
            let r = engine.transcribe(&samples[spans[i].start..spans[i].end], None)?;
            language = r.language.clone();
            first = Some(((t, i), r));
        }
    }

    let mut pieces = vec![];
    let mut done = 0usize;
    progress(0.0);
    for (t, (track, samples, spans, raw)) in tracks.iter().enumerate() {
        for (i, span) in spans.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(PipelineError::Cancelled);
            }
            let result = match &first {
                Some((at, r)) if *at == (t, i) => r.clone(),
                _ => engine.transcribe(&samples[span.start..span.end], language.as_deref())?,
            };
            let mut got = place(*track, span, result.pieces);
            // Engines sometimes skip a stretch of a chunk. Any detected speech of a second or more that got no
            // words is transcribed again on its own.
            for missed in uncovered(raw, span, &got) {
                let r = engine.transcribe(&samples[missed.start..missed.end], language.as_deref())?;
                got.extend(place(*track, &missed, r.pieces));
            }
            pieces.extend(got);
            done += span.len();
            progress(done as f32 / total.max(1) as f32);
        }
    }

    let (pieces, echo_removed) = remove_echo(pieces);
    let lines = merge(pieces);
    if language.is_none() {
        language = detect_language(&lines);
    }
    Ok(Output { lines, language, engine: engine.label().to_string(), echo_removed, speech_ms: (total as u64) * 1000 / 16_000 })
}

/// Engine pieces (relative to the chunk) on the recording clock.
fn place(track: Track, span: &Span, pieces: Vec<super::engine::Piece>) -> Vec<Timed> {
    let offset = span.start_ms();
    let end = offset + span.len() as u64 * 1000 / 16_000;
    pieces.into_iter().map(|p| Timed { track, start_ms: offset + p.start_ms, end_ms: (offset + p.end_ms).min(end), text: p.text }).collect()
}

/// Detected speech (raw spans, at least 1 s) inside `chunk` that no transcribed piece overlaps. Returned with
/// half a second of context on each side, kept inside the chunk.
fn uncovered(raw: &[Span], chunk: &Span, got: &[Timed]) -> Vec<Span> {
    const MIN: usize = 16_000;
    const CONTEXT: usize = 8_000;
    raw.iter()
        .filter(|r| r.start >= chunk.start && r.end <= chunk.end && r.len() >= MIN)
        .filter(|r| {
            let (a, b) = (r.start as u64 * 1000 / 16_000, r.end as u64 * 1000 / 16_000);
            !got.iter().any(|p| p.start_ms < b && a < p.end_ms.max(p.start_ms + 1))
        })
        .map(|r| Span { start: r.start.saturating_sub(CONTEXT).max(chunk.start), end: (r.end + CONTEXT).min(chunk.end) })
        .collect()
}

/// Language of the transcript text, when the detector is confident.
pub fn detect_language(lines: &[TranscriptLine]) -> Option<String> {
    let text: String = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join(" ");
    let info = whatlang::detect(&text)?;
    if !info.is_reliable() {
        return None;
    }
    iso639_1(info.lang().code()).map(str::to_string)
}

fn iso639_1(code3: &str) -> Option<&'static str> {
    Some(match code3 {
        "eng" => "en",
        "spa" => "es",
        "fra" => "fr",
        "deu" => "de",
        "ita" => "it",
        "por" => "pt",
        "nld" => "nl",
        "pol" => "pl",
        "rus" => "ru",
        "ukr" => "uk",
        "tgl" => "tl",
        "cmn" => "zh",
        "jpn" => "ja",
        "kor" => "ko",
        "ara" => "ar",
        "hin" => "hi",
        "tur" => "tr",
        "swe" => "sv",
        "dan" => "da",
        "nob" => "no",
        "fin" => "fi",
        "ces" => "cs",
        "slk" => "sk",
        "slv" => "sl",
        "hrv" => "hr",
        "srp" => "sr",
        "bul" => "bg",
        "ron" => "ro",
        "hun" => "hu",
        "ell" => "el",
        "lit" => "lt",
        "lav" => "lv",
        "est" => "et",
        "heb" => "he",
        "ind" => "id",
        "vie" => "vi",
        "tha" => "th",
        "cat" => "ca",
        "afr" => "af",
        "bel" => "be",
        "mkd" => "mk",
        "cym" => "cy",
        "pes" => "fa",
        "urd" => "ur",
        "ben" => "bn",
        "tam" => "ta",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcriber::engine::{ChunkResult, Piece};
    use crate::transcriber::vad::{Shape, RATE};

    /// Speech wherever the signal is non-zero.
    struct EnergyDetector;
    impl Detector for EnergyDetector {
        fn detect(&mut self, s: &[f32]) -> Result<Vec<Span>, String> {
            let mut out: Vec<Span> = vec![];
            for (i, w) in s.chunks(RATE / 10).enumerate() {
                if w.iter().any(|x| *x != 0.0) {
                    let (a, b) = (i * RATE / 10, i * RATE / 10 + w.len());
                    match out.last_mut() {
                        Some(l) if l.end == a => l.end = b,
                        _ => out.push(Span { start: a, end: b }),
                    }
                }
            }
            Ok(out)
        }
    }

    /// Says what it heard: the chunk's loudness tells which "sentence" it is.
    struct FakeEngine {
        calls: Vec<Option<String>>,
        detected: Option<&'static str>,
    }
    impl Engine for FakeEngine {
        fn transcribe(&mut self, samples: &[f32], language: Option<&str>) -> Result<ChunkResult, String> {
            self.calls.push(language.map(str::to_string));
            let level = samples.iter().fold(0f32, |a, b| a.max(b.abs()));
            let text = match (level * 10.0).round() as i32 {
                3 => "We should ship onboarding first.",
                5 => "Agreed, pricing can wait.",
                _ => "Something else.",
            };
            Ok(ChunkResult { pieces: vec![Piece { start_ms: 0, end_ms: samples.len() as u64 * 1000 / 16_000, text: text.into() }], language: self.detected.map(str::to_string) })
        }
        fn label(&self) -> &str {
            "Fake"
        }
        fn chunking(&self) -> Shape {
            Shape { pad: 0, ..Shape::default() }
        }
    }

    fn wav(path: &Path, parts: &[(f32, f32, f32)], secs: f32) {
        let spec = hound::WavSpec { channels: 1, sample_rate: 16000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..(secs * RATE as f32) as usize {
            let t = i as f32 / RATE as f32;
            let v = parts.iter().find(|(a, b, _)| t >= *a && t < *b).map(|p| p.2).unwrap_or(0.0);
            w.write_sample((v * 32767.0) as i16).unwrap();
        }
        w.finalize().unwrap();
    }

    fn meeting() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        // Them says sentence A at 2 s; the mic hears it as echo at the same time. I answer with B at 6 s.
        wav(&dir.path().join("them.wav"), &[(2.0, 4.0, 0.3)], 10.0);
        wav(&dir.path().join("me.wav"), &[(2.0, 4.0, 0.3), (6.0, 8.0, 0.5)], 10.0);
        dir
    }

    #[test]
    fn transcribes_both_tracks_removes_echo_and_merges() {
        let dir = meeting();
        let mut engine = FakeEngine { calls: vec![], detected: None };
        let mut seen = vec![];
        let out = run(dir.path(), &mut engine, &mut EnergyDetector, Some("en"), &AtomicBool::new(false), &mut |p| seen.push(p)).unwrap();
        let got: Vec<_> = out.lines.iter().map(|l| (l.speaker.as_str(), l.at, l.text.as_str())).collect();
        assert_eq!(got, vec![("them:0", 2000, "We should ship onboarding first."), ("me", 6000, "Agreed, pricing can wait.")]);
        assert_eq!(out.echo_removed, 1);
        assert_eq!(out.speech_ms, 6000);
        assert_eq!(out.language.as_deref(), Some("en"));
        assert_eq!(seen.first(), Some(&0.0));
        assert_eq!(seen.last(), Some(&1.0));
        assert!(seen.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn language_is_settled_once_then_used_for_every_chunk() {
        let dir = meeting();
        let mut engine = FakeEngine { calls: vec![], detected: Some("es") };
        let out = run(dir.path(), &mut engine, &mut EnergyDetector, None, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(out.language.as_deref(), Some("es"));
        // First call detects (None), the other two use the result; the detecting chunk isn't transcribed twice.
        assert_eq!(engine.calls, vec![None, Some("es".into()), Some("es".into())]);
    }

    #[test]
    fn speech_the_engine_skipped_is_transcribed_again() {
        let chunk = Span { start: 0, end: 30 * RATE };
        let raw = [Span { start: RATE, end: 4 * RATE }, Span { start: 10 * RATE, end: 14 * RATE }, Span { start: 20 * RATE, end: 20 * RATE + RATE / 2 }];
        let got = vec![Timed { track: Track::Me, start_ms: 1200, end_ms: 3800, text: "Covered.".into() }];
        // The 10-14 s speech has no words: retried with context. The short 0.5 s blip is ignored.
        assert_eq!(uncovered(&raw, &chunk, &got), vec![Span { start: 10 * RATE - RATE / 2, end: 14 * RATE + RATE / 2 }]);
    }

    #[test]
    fn a_missing_track_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        wav(&dir.path().join("them.wav"), &[(1.0, 2.0, 0.5)], 3.0);
        let out = run(dir.path(), &mut FakeEngine { calls: vec![], detected: None }, &mut EnergyDetector, Some("en"), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(out.lines.len(), 1);
    }

    #[test]
    fn cancel_stops_between_chunks() {
        let dir = meeting();
        let r = run(dir.path(), &mut FakeEngine { calls: vec![], detected: None }, &mut EnergyDetector, Some("en"), &AtomicBool::new(true), &mut |_| {});
        assert_eq!(r, Err(PipelineError::Cancelled));
    }

    #[test]
    fn detects_the_language_of_text() {
        let line = |t: &str| TranscriptLine { id: "l0".into(), at: 0, end: 1, speaker: "me".into(), text: t.into(), edited: false };
        assert_eq!(detect_language(&[line("We should ship the onboarding revamp first and move pricing to the next quarter.")]).as_deref(), Some("en"));
        assert_eq!(detect_language(&[line("Deberíamos lanzar primero la nueva bienvenida y dejar los precios para el próximo trimestre.")]).as_deref(), Some("es"));
        assert_eq!(detect_language(&[line("ok")]), None);
    }

    /// `TALAAN_SMOKE_MEETING=<dir> cargo test --release pipeline_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn pipeline_smoke() {
        use crate::transcriber::models::{ModelStore, CATALOG};
        let dir = std::path::PathBuf::from(std::env::var("TALAAN_SMOKE_MEETING").unwrap());
        let data = dirs::data_dir().unwrap().join("Talaan");
        let store = ModelStore::new(&data, false);
        let mut vad = Vad::load(&data.join("models/vad/ggml-silero-v6.2.0.bin")).unwrap();
        for m in CATALOG {
            let Some(d) = store.locate(m).dir().map(|d| d.to_path_buf()) else { continue };
            let mut engine = crate::transcriber::engine::load(m, &d, true).unwrap();
            let t = std::time::Instant::now();
            let out = run(&dir, engine.as_mut(), &mut vad, None, &AtomicBool::new(false), &mut |_| {}).unwrap();
            eprintln!("== {}: {:.1}s, {} lines, {} echo removed, language {:?}, {:.0}s speech", m.id, t.elapsed().as_secs_f32(), out.lines.len(), out.echo_removed, out.language, out.speech_ms as f32 / 1000.0);
            for l in &out.lines {
                eprintln!("   {:>6.1} {:<6} {}", l.at as f32 / 1000.0, l.speaker, l.text);
            }
        }
    }
}
