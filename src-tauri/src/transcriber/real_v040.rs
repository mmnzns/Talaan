//! Real-model checks for the engine v0.4.0 move (Cohere Transcribe, Gemma 4 E4B). All `#[ignore]`: they need the
//! models and the helpers. They use Talaan's own code (the download calls of `models_download`, `pipeline::run`, the
//! voice detector, `make_engine` and `summarize`), a scratch data folder and speech made by Windows' SAPI, never the
//! app's data folder or a real recording. The models are the shared folder's (`Craft Concepts Digital/Models`).
//!
//! ```text
//! TALAAN_REAL_SPEECH=<speech.wav, 16 kHz mono> cargo test --release real_v040 -- --ignored --nocapture --test-threads 1
//! ```
// Everything below is test code; the scanner in log_scan.rs reads what comes before `#[cfg(test)] mod` as app code.
#[cfg(test)]
mod checks {
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use crate::transcriber::engine::{ChunkResult, Engine};
use crate::transcriber::pipeline::run;
use crate::transcriber::speech::{EngineSpeech, EngineVad};
use crate::transcriber::vad::{Shape, RATE};
use crate::models::{Models, VAD};
use voice_engine_models::{catalog, Status};

fn real_models() -> Models {
    Models::open_shared_without_legacy().unwrap()
}

/// Talaan's own download steps (`models_download`): the voice detector first for a speech model, then the model.
/// A model that is already complete is not fetched again (the engine checks each file).
#[test]
#[ignore]
fn real_v040_download() {
    let models = real_models();
    for id in std::env::var("TALAAN_REAL_IDS").unwrap_or("cohere-transcribe,gemma-4-e4b".into()).split(',') {
        let model = catalog::find(id).unwrap();
        let before = short(&models.status(id));
        let cancel = AtomicBool::new(false);
        let started = Instant::now();
        if model.is_speech() && models.ready_dir(VAD).is_none() {
            models.store().download(VAD, &cancel, &mut |_| {}).unwrap();
        }
        let mut last = 0.0;
        models.store().download(id, &cancel, &mut |p| last = p.fraction).unwrap_or_else(|e| panic!("{id}: {e}"));
        let after = models.status(id);
        eprintln!("== {id}: before {before}, download call {:.1}s, last progress {last:.2}, after {}", started.elapsed().as_secs_f32(), short(&after));
        assert!(matches!(after, Status::Installed { .. }), "{id} is not installed");
    }
}

fn short(s: &Status) -> &'static str {
    match s {
        Status::Installed { .. } => "installed",
        Status::Downloading { .. } => "downloading",
        Status::Partial { .. } => "partial",
        Status::Missing => "missing",
    }
}

fn read(path: &Path) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).unwrap();
    assert_eq!((r.spec().sample_rate, r.spec().channels), (16_000, 1));
    r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
}

fn write(path: &Path, samples: &[f32]) {
    let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for s in samples {
        w.write_sample((s * 32767.0) as i16).unwrap();
    }
    w.finalize().unwrap();
}

/// Quiet room noise (about -50 dB), not digital silence.
fn quiet(secs: f32, seed: u32) -> Vec<f32> {
    let mut x = seed;
    (0..(secs * RATE as f32) as usize)
        .map(|_| {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            ((x >> 16) as f32 / 65536.0 - 0.5) * 0.006
        })
        .collect()
}

/// Wraps the real engine and notes what the pipeline gave it.
struct Spy {
    inner: EngineSpeech,
    chunk_secs: Vec<f32>,
    languages: Vec<Option<String>>,
}
impl Engine for Spy {
    fn transcribe(&mut self, samples: &[f32], language: Option<&str>) -> Result<ChunkResult, String> {
        self.chunk_secs.push(samples.len() as f32 / RATE as f32);
        self.languages.push(language.map(str::to_string));
        self.inner.transcribe(samples, language)
    }
    fn label(&self) -> &str {
        self.inner.label()
    }
    fn chunking(&self) -> Shape {
        self.inner.chunking()
    }
    fn backend(&self) -> Option<voice_engine_stt::SpeechBackend> {
        self.inner.backend()
    }
    fn detects_language(&self) -> bool {
        self.inner.detects_language()
    }
    fn languages(&self) -> Option<&'static [&'static str]> {
        self.inner.languages()
    }
}

/// Voice detection, 35 s chunks and Cohere through `pipeline::run`, on SAPI speech: sensible text for speech, none
/// for silence or room noise, Auto means English, no chunk over 35 s.
#[test]
#[ignore]
fn real_v040_cohere_pipeline() {
    let speech = read(Path::new(&std::env::var("TALAAN_REAL_SPEECH").expect("TALAAN_REAL_SPEECH: a 16 kHz mono wav")));
    let models = real_models();
    let cohere = catalog::find("cohere-transcribe").unwrap();
    let mut vad = EngineVad::new(models.hold(VAD).unwrap());
    let t = Instant::now();
    let inner = EngineSpeech::load(cohere, models.hold(cohere.id).unwrap(), false).unwrap_or_else(|e| panic!("load: {e}"));
    eprintln!("== cohere load {:.1}s, backend {:?}", t.elapsed().as_secs_f32(), inner.backend());
    let mut spy = Spy { inner, chunk_secs: vec![], languages: vec![] };
    let cancel = AtomicBool::new(false);

    // 1. Short speech between silences on Them; room noise only on Me. Language: Auto.
    let dir = tempfile::tempdir().unwrap();
    let mut them = quiet(3.0, 1);
    them.extend(&speech);
    them.extend(quiet(8.0, 2));
    them.extend(&speech);
    them.extend(quiet(4.0, 3));
    write(&dir.path().join("them.wav"), &them);
    write(&dir.path().join("me.wav"), &quiet(25.0, 4));
    let t = Instant::now();
    let out = run(dir.path(), &mut spy, &mut vad, None, None, &cancel, &mut |_| {}).unwrap_or_else(|e| panic!("{e:?}"));
    let secs = t.elapsed().as_secs_f32();
    eprintln!(
        "== short: {:.1}s for {:.0}s of audio, {} lines, speech {} ms, language {:?}, chunks {:?}, languages given {:?}",
        secs,
        them.len() as f32 / 16000.0,
        out.lines.len(),
        out.speech_ms,
        out.language,
        spy.chunk_secs,
        spy.languages
    );
    for l in &out.lines {
        eprintln!("   [{}] {} ms: {}", l.speaker, l.at, l.text);
    }
    let all = out.lines.iter().map(|l| l.text.to_lowercase()).collect::<Vec<_>>().join(" ");
    assert!(all.contains("onboarding") && all.contains("pricing") && all.contains("budget"), "text: {all}");
    assert!(out.lines.iter().all(|l| l.speaker != "me"), "the quiet track produced text");
    assert_eq!(out.language.as_deref(), Some("en"));
    assert!(spy.languages.iter().all(|l| l.as_deref() == Some("en")), "Auto must be given as English: {:?}", spy.languages);
    assert!(spy.chunk_secs.iter().all(|s| *s <= 35.0));

    // 2. Silence only: the model is never called and there is no text.
    let silent = tempfile::tempdir().unwrap();
    write(&silent.path().join("me.wav"), &quiet(40.0, 5));
    write(&silent.path().join("them.wav"), &vec![0.0; 40 * RATE]);
    let calls_before = spy.chunk_secs.len();
    let out = run(silent.path(), &mut spy, &mut vad, None, None, &cancel, &mut |_| {}).unwrap();
    eprintln!("== silence: {} lines, {} engine calls", out.lines.len(), spy.chunk_secs.len() - calls_before);
    assert!(out.lines.is_empty());
    assert_eq!(spy.chunk_secs.len(), calls_before);

    // 3. Over a minute of near-continuous speech: split into chunks of at most 35 s.
    let long = tempfile::tempdir().unwrap();
    let mut them = vec![];
    for _ in 0..5 {
        them.extend(&speech);
        them.extend(quiet(0.4, 7));
    }
    write(&long.path().join("them.wav"), &them);
    spy.chunk_secs.clear();
    let t = Instant::now();
    let out = run(long.path(), &mut spy, &mut vad, Some("en"), None, &cancel, &mut |_| {}).unwrap();
    eprintln!("== long: {:.1}s for {:.0}s of audio, {} lines, chunks {:?}", t.elapsed().as_secs_f32(), them.len() as f32 / 16000.0, out.lines.len(), spy.chunk_secs);
    assert!(spy.chunk_secs.len() >= 2 && spy.chunk_secs.iter().all(|s| *s <= 35.0), "{:?}", spy.chunk_secs);
    assert!(out.lines.len() >= 5);
}

/// A summary with Gemma 4 E4B through Talaan's own summarizer: `make_engine` from settings (the local AI helper, built
/// from engine v0.4.0, next to the test binary), then `summarize` with the real prompt, schema and parser on the
/// planning-call fixture. Prints what came back.
#[test]
#[ignore]
fn real_v040_gemma_summary() {
    use crate::commands::summarizer::make_engine;
    use crate::storage::model::TranscriptLine;
    use crate::summarizer::prompt::{tests::meeting, Options};
    use crate::summarizer::summarize::summarize;
    use crate::summarizer::worker::Config;

    let models = real_models();
    let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../fixtures/summary.planning-call.json")).unwrap();
    let lines: Vec<TranscriptLine> = serde_json::from_value(fixture["transcript"].clone()).unwrap();
    let mut m = meeting(lines.clone());
    m.speakers = serde_json::from_value(fixture["speakers"].clone()).unwrap();
    m.marks = serde_json::from_value(fixture["marks"].clone()).unwrap();
    m.item.duration_ms = fixture["durationMs"].as_u64().unwrap();

    let settings = serde_json::json!({ "summaryEngine": "local", "localModel": "gemma-4-e4b", "useGpu": std::env::var("TALAAN_REAL_GPU").map_or(true, |v| v != "0") });
    let cfg = Config { engine: "local".into(), language: "en".into(), template: String::new(), instructions: String::new(), settings };
    let data = tempfile::tempdir().unwrap();
    let mut engine = make_engine(&cfg, data.path(), &models).unwrap_or_else(|e| panic!("engine: {e:?}"));
    eprintln!("== engine: {} (context {} tokens), {} transcript lines", engine.label(), engine.context_tokens(), lines.len());
    let t = Instant::now();
    let out = summarize(&m, engine.as_mut(), "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap_or_else(|e| panic!("summarize: {e:?}"));
    eprintln!("== gemma summary in {:.1}s, {} part(s)", t.elapsed().as_secs_f32(), out.parts);
    eprintln!("   title: {:?}\n   recap: {}", out.title, out.summary.recap);
    for x in &out.summary.decisions {
        eprintln!("   D [{:?}] {}", x.at.map(|a| a / 1000), x.text);
    }
    for x in &out.summary.action_items {
        eprintln!("   A [{:?}] {} | owner {:?} | due {:?}", x.at.map(|a| a / 1000), x.text, x.owner, x.due);
    }
    for x in &out.summary.open_questions {
        eprintln!("   Q [{:?}] {}", x.at.map(|a| a / 1000), x.text);
    }
    assert!(!out.summary.recap.trim().is_empty());
    assert!(!out.summary.decisions.is_empty() || !out.summary.action_items.is_empty());
    let json = serde_json::to_string(&out.summary).unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&json).unwrap()["recap"].is_string());
    crate::summarizer::local::shutdown_all();
}
}
