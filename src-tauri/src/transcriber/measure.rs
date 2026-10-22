//! Measurements for Parakeet (2026-10-22, docs/WINDOWS-PORT.md "Parakeet measurements"): how long a chunk to hand
//! the model, and whether the encoder is worth running on the GPU through DirectML. Nothing here ships: the scoring
//! helpers have ordinary unit tests, the live tests are `#[ignore]` and read their inputs from the environment.
//!
//! Chunk length (any processor):
//! `TALAAN_PK_MODEL=<model dir> TALAAN_PK_VAD=<ggml-silero*.bin> TALAAN_PK_WAV=<16 kHz mono wav> TALAAN_PK_TRUTH=<txt or truth.json>
//!  cargo test --release parakeet_chunk_live -- --ignored --nocapture`
//! (optional: `TALAAN_PK_LENGTHS=40,25,15` seconds, `TALAAN_PK_REPS=2`, `TALAAN_PK_NOISE_DB=15`, `TALAAN_PK_OUT=<dir>`).
//!
//! DirectML (adds `--features parakeet-directml`; one provider per process so memory numbers are clean):
//! `TALAAN_PK_EP=cpu|default|high|id0|id1|id2|bogus ... cargo test --release --features parakeet-directml parakeet_directml_live -- --ignored --nocapture`

use super::engine::{Engine, ParakeetEngine};
use super::vad::{read_track, shape, Shape, Span, Vad, RATE};
use std::path::PathBuf;
use std::time::Instant;

// ---------------------------------------------------------------------------------------------------------------
// Scoring: words, alignment, error kinds.

const ONES: [&str; 20] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen",
    "seventeen", "eighteen", "nineteen",
];
const TENS: [&str; 10] = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];

fn say_below_100(n: u64, out: &mut Vec<String>) {
    if n < 20 {
        out.push(ONES[n as usize].into());
    } else {
        out.push(TENS[(n / 10) as usize].into());
        if n % 10 != 0 {
            out.push(ONES[(n % 10) as usize].into());
        }
    }
}

fn say_below_1000(n: u64, out: &mut Vec<String>) {
    if n >= 100 {
        out.push(ONES[(n / 100) as usize].into());
        out.push("hundred".into());
    }
    if n % 100 != 0 || n == 0 {
        say_below_100(n % 100, out);
    }
}

/// A whole number in words ("forty seven", "four hundred thousand"); years from 1100 to 2099 as "twenty fifteen".
fn say(digits: &str) -> Vec<String> {
    let mut out = vec![];
    let Ok(n) = digits.parse::<u64>() else { return vec![digits.to_string()] };
    if digits.len() == 4 && (1100..2100).contains(&n) && n % 1000 >= 10 {
        say_below_100(n / 100, &mut out);
        say_below_100(n % 100, &mut out);
    } else if n < 1000 {
        say_below_1000(n, &mut out);
    } else if n < 1_000_000 {
        say_below_1000(n / 1000, &mut out);
        out.push("thousand".into());
        if n % 1000 != 0 {
            say_below_1000(n % 1000, &mut out);
        }
    } else {
        return vec![digits.to_string()];
    }
    out
}

/// Words for scoring: lower case, punctuation gone, hyphens split, digits spoken, "%" as "percent".
pub fn words(text: &str) -> Vec<String> {
    // "400,000" is one number.
    let mut flat = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == ',' && i > 0 && chars[i - 1].is_ascii_digit() && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
            continue;
        }
        flat.push(match c {
            '\u{2019}' | '\u{2018}' => '\'',
            '%' => ' ',
            c => c,
        });
        if c == '%' {
            flat.push_str(" percent ");
        }
    }
    let mut out = vec![];
    for raw in flat.to_lowercase().split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '.')) {
        // A full stop inside a number ("4.2") stays; at the edges it is punctuation.
        let tok = raw.trim_matches(|c| c == '\'' || c == '.');
        if tok.is_empty() {
            continue;
        }
        for piece in tok.split('.').filter(|t| !t.is_empty()) {
            if piece.chars().all(|c| c.is_ascii_digit()) {
                out.extend(say(piece));
            } else {
                out.push(piece.to_string());
            }
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Hit,
    Sub,
    Del,
    Ins,
}

/// Word-level edit alignment of `hyp` against `reference` (ties prefer a hit, then a substitution).
pub fn align(reference: &[String], hyp: &[String]) -> Vec<(Op, usize, usize)> {
    let (n, m) = (reference.len(), hyp.len());
    let mut d = vec![vec![0u32; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i as u32;
    }
    for j in 0..=m {
        d[0][j] = j as u32;
    }
    for i in 1..=n {
        for j in 1..=m {
            let sub = d[i - 1][j - 1] + u32::from(reference[i - 1] != hyp[j - 1]);
            d[i][j] = sub.min(d[i - 1][j] + 1).min(d[i][j - 1] + 1);
        }
    }
    let (mut i, mut j) = (n, m);
    let mut ops = vec![];
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && d[i][j] == d[i - 1][j - 1] + u32::from(reference[i - 1] != hyp[j - 1]) {
            ops.push((if reference[i - 1] == hyp[j - 1] { Op::Hit } else { Op::Sub }, i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if i > 0 && d[i][j] == d[i - 1][j] + 1 {
            ops.push((Op::Del, i - 1, j));
            i -= 1;
        } else {
            ops.push((Op::Ins, i, j - 1));
            j -= 1;
        }
    }
    ops.reverse();
    ops
}

#[derive(Debug, Default, Clone)]
pub struct Score {
    pub reference: usize,
    pub sub: usize,
    pub del: usize,
    pub ins: usize,
    /// Runs of 4 or more consecutive missing words (a dropped phrase or sentence): count and words.
    pub dropped_runs: usize,
    pub dropped_words: usize,
    /// Inserted runs of 2 or more words that repeat the words right before or after them (a repeated phrase).
    pub repeat_runs: usize,
    pub repeat_words: usize,
    /// Where (as a fraction of the reference) the dropped runs start, with their first words.
    pub dropped_at: Vec<(f32, String)>,
    pub repeated_at: Vec<(f32, String)>,
}

impl Score {
    pub fn wer(&self) -> f64 {
        (self.sub + self.del + self.ins) as f64 / self.reference.max(1) as f64
    }
}

pub fn score(reference: &[String], hyp: &[String]) -> Score {
    let ops = align(reference, hyp);
    let mut s = Score { reference: reference.len(), ..Default::default() };
    let mut k = 0;
    while k < ops.len() {
        let (op, r, h) = (&ops[k].0, ops[k].1, ops[k].2);
        match op {
            Op::Sub => s.sub += 1,
            Op::Del | Op::Ins => {
                let mut e = k;
                while e < ops.len() && ops[e].0 == *op {
                    e += 1;
                }
                let run = e - k;
                if *op == Op::Del {
                    s.del += run;
                    if run >= 4 {
                        s.dropped_runs += 1;
                        s.dropped_words += run;
                        s.dropped_at.push((r as f32 / reference.len().max(1) as f32, reference[r..(r + run).min(reference.len())].iter().take(8).cloned().collect::<Vec<_>>().join(" ")));
                    }
                } else {
                    s.ins += run;
                    let inserted = &hyp[h..h + run];
                    let before = h >= run && &hyp[h - run..h] == inserted;
                    let after = h + 2 * run <= hyp.len() && &hyp[h + run..h + 2 * run] == inserted;
                    if run >= 2 && (before || after) {
                        s.repeat_runs += 1;
                        s.repeat_words += run;
                        s.repeated_at.push((r as f32 / reference.len().max(1) as f32, inserted.iter().take(8).cloned().collect::<Vec<_>>().join(" ")));
                    }
                }
                k = e;
                continue;
            }
            Op::Hit => {}
        }
        k += 1;
    }
    s
}

fn describe(label: &str, s: &Score) -> String {
    format!(
        "{label}: WER {:.2}% (ref {} words: {} wrong, {} missing, {} extra) | dropped phrases {} ({} words) | repeated phrases {} ({} words)",
        s.wer() * 100.0,
        s.reference,
        s.sub,
        s.del,
        s.ins,
        s.dropped_runs,
        s.dropped_words,
        s.repeat_runs,
        s.repeat_words
    )
}

// ---------------------------------------------------------------------------------------------------------------
// Inputs, chunks, memory.

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.trim().is_empty())
}

fn need(name: &str) -> String {
    env(name).unwrap_or_else(|| panic!("set {name}"))
}

/// The script as words: a plain text file, or a fixture's truth.json (every line's text in order).
fn truth_words(path: &str) -> Vec<String> {
    let raw = std::fs::read_to_string(path).unwrap();
    if path.ends_with(".json") {
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let text: Vec<&str> = v.as_array().unwrap().iter().filter_map(|l| l["text"].as_str()).collect();
        words(&text.join(" "))
    } else {
        words(&raw)
    }
}

/// White noise through a one-pole low-pass (it sits where speech does), mixed in at `snr_db` against the track's RMS.
fn add_noise(samples: &mut [f32], snr_db: f32, seed: u64) {
    let rms = (samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32).sqrt();
    let mut state = seed | 1;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f32 / (1u64 << 53) as f32 - 0.5
    };
    let mut lp = 0.0f32;
    let noise: Vec<f32> = (0..samples.len())
        .map(|_| {
            let w = next() + next() + next() + next();
            lp = 0.7 * lp + 0.3 * w;
            lp
        })
        .collect();
    let n_rms = (noise.iter().map(|x| x * x).sum::<f32>() / noise.len().max(1) as f32).sqrt();
    let gain = rms / 10f32.powf(snr_db / 20.0) / n_rms.max(1e-9);
    for (s, n) in samples.iter_mut().zip(noise) {
        *s = (*s + n * gain).clamp(-1.0, 1.0);
    }
}

#[cfg(windows)]
fn memory_mb() -> (f64, f64, f64) {
    #[repr(C)]
    struct Counters {
        cb: u32,
        page_faults: u32,
        peak_working_set: usize,
        working_set: usize,
        _quota: [usize; 4],
        pagefile: usize,
        peak_pagefile: usize,
        private: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        safe fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut c = Counters { cb: std::mem::size_of::<Counters>() as u32, page_faults: 0, peak_working_set: 0, working_set: 0, _quota: [0; 4], pagefile: 0, peak_pagefile: 0, private: 0 };
    // SAFETY: `c` is a valid, correctly sized PROCESS_MEMORY_COUNTERS_EX for this process.
    unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    let mb = |b: usize| b as f64 / 1_048_576.0;
    (mb(c.working_set), mb(c.peak_working_set), mb(c.private))
}

#[cfg(not(windows))]
fn memory_mb() -> (f64, f64, f64) {
    (0.0, 0.0, 0.0)
}

struct Audio {
    samples: Vec<f32>,
    spans: Vec<Span>,
    vad_s: f64,
}

fn load_audio() -> Audio {
    let mut samples = read_track(std::path::Path::new(&need("TALAAN_PK_WAV"))).unwrap();
    if let Some(db) = env("TALAAN_PK_NOISE_DB") {
        add_noise(&mut samples, db.parse().unwrap(), 12345);
    }
    let mut vad = Vad::load(std::path::Path::new(&need("TALAAN_PK_VAD"))).unwrap();
    let t = Instant::now();
    let spans = vad.detect(&samples).unwrap();
    Audio { samples, spans, vad_s: t.elapsed().as_secs_f64() }
}

struct Run {
    text: Vec<String>,
    secs: f64,
    chunks: Vec<(f32, f32, usize, f64)>,
}

/// What the transcriber does for one track, minus the merge: shape the VAD spans for a chunk length, then each chunk.
fn run_chunks(engine: &mut dyn Engine, audio: &Audio, max_len_s: f32) -> Run {
    let shaped = shape(&audio.spans, &audio.samples, Shape { max_len: (max_len_s * RATE as f32) as usize, ..Shape::parakeet() });
    let mut text = vec![];
    let mut chunks = vec![];
    let t = Instant::now();
    for c in &shaped {
        let t0 = Instant::now();
        let r = engine.transcribe(&audio.samples[c.start..c.end], Some("en")).unwrap();
        let mut n = 0;
        for p in r.pieces {
            n += 1;
            text.push(p.text);
        }
        chunks.push((c.start as f32 / RATE as f32, c.len() as f32 / RATE as f32, n, t0.elapsed().as_secs_f64()));
    }
    Run { text: words(&text.join(" ")), secs: t.elapsed().as_secs_f64(), chunks }
}

fn model_dir() -> PathBuf {
    PathBuf::from(need("TALAAN_PK_MODEL"))
}

fn write_out(name: &str, words: &[String]) {
    if let Some(dir) = env("TALAAN_PK_OUT") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(PathBuf::from(dir).join(format!("{name}.txt")), words.join(" ")).unwrap();
    }
}

// ---------------------------------------------------------------------------------------------------------------
// A. Chunk length.

#[test]
#[ignore]
fn parakeet_chunk_live() {
    let truth = truth_words(&need("TALAAN_PK_TRUTH"));
    let audio = load_audio();
    let secs = audio.samples.len() as f64 / RATE as f64;
    let lengths: Vec<f32> = env("TALAAN_PK_LENGTHS").unwrap_or_else(|| "40,25,15".into()).split(',').map(|s| s.trim().parse().unwrap()).collect();
    let reps: usize = env("TALAAN_PK_REPS").map(|s| s.parse().unwrap()).unwrap_or(2);
    let t = Instant::now();
    let mut engine = ParakeetEngine::load(&model_dir(), "Parakeet", super::models::EngineKind::Parakeet).unwrap();
    eprintln!("RESULT | audio {secs:.0} s, {} VAD spans ({:.2} s), model load {:.1} s, truth {} words, noise {:?}", audio.spans.len(), audio.vad_s, t.elapsed().as_secs_f64(), truth.len(), env("TALAAN_PK_NOISE_DB"));
    // The first call pays for ONNX Runtime's lazy setup; keep it out of the timings.
    let warm = shape(&audio.spans, &audio.samples, Shape::parakeet());
    let _ = engine.transcribe(&audio.samples[warm[0].start..warm[0].end.min(warm[0].start + 10 * RATE)], Some("en")).unwrap();
    for len in &lengths {
        let mut times = vec![];
        let mut last = None;
        for _ in 0..reps {
            let r = run_chunks(&mut engine, &audio, *len);
            times.push(r.secs);
            last = Some(r);
        }
        let r = last.unwrap();
        let s = score(&truth, &r.text);
        let lens: Vec<f32> = r.chunks.iter().map(|c| c.1).collect();
        eprintln!(
            "RESULT | max {len:>4.0} s | {} chunks (avg {:.1} s, longest {:.1} s) | time {} s | {:.0}x real time",
            lens.len(),
            lens.iter().sum::<f32>() / lens.len().max(1) as f32,
            lens.iter().cloned().fold(0.0, f32::max),
            times.iter().map(|t| format!("{t:.2}")).collect::<Vec<_>>().join(" / "),
            secs / times.iter().cloned().fold(f64::MAX, f64::min)
        );
        eprintln!("RESULT |   {}", describe(&format!("{len:.0} s"), &s));
        for (at, w) in &s.dropped_at {
            eprintln!("RESULT |   dropped at {:.0}%: {w}", at * 100.0);
        }
        for (at, w) in &s.repeated_at {
            eprintln!("RESULT |   repeated at {:.0}%: {w}", at * 100.0);
        }
        let stem = std::path::Path::new(&need("TALAAN_PK_WAV")).file_stem().unwrap().to_string_lossy().into_owned();
        write_out(&format!("chunk-{stem}-{len:.0}s{}", env("TALAAN_PK_NOISE_DB").map(|d| format!("-snr{d}")).unwrap_or_default()), &r.text);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// B. DirectML.

#[cfg(feature = "parakeet-directml")]
fn config_for(kind: &str) -> parakeet_rs::ExecutionConfig {
    use ort::ep::{self, directml::PerformancePreference};
    use parakeet_rs::{ExecutionConfig, ExecutionProvider};
    let threads = talaan_stt::threads();
    let base = ExecutionConfig::new().with_intra_threads(threads);
    // DirectML needs the memory pattern and parallel execution off (ONNX Runtime's DirectML page).
    let dml = |ep: ep::ExecutionProviderDispatch| {
        base.clone().with_custom_configure(move |b| Ok(b.with_memory_pattern(false)?.with_parallel_execution(false)?.with_execution_providers([ep.clone()])?))
    };
    match kind {
        "cpu" => base,
        // Through parakeet-rs's own switch: DirectML::default() (any GPU), and a failed registration is silent.
        "default" => base.with_execution_provider(ExecutionProvider::DirectML).with_custom_configure(|b| Ok(b.with_memory_pattern(false)?.with_parallel_execution(false)?)),
        "default-strict" => dml(ep::DirectML::default().build().error_on_failure()),
        "high" => dml(ep::DirectML::default().with_performance_preference(PerformancePreference::HighPerformance).build().error_on_failure()),
        "minpower" => dml(ep::DirectML::default().with_performance_preference(PerformancePreference::MinimumPower).build().error_on_failure()),
        k if k.starts_with("id") => dml(ep::DirectML::default().with_device_id(k[2..].parse().unwrap()).build().error_on_failure()),
        "bogus" => dml(ep::DirectML::default().with_device_id(99).build().error_on_failure()),
        other => panic!("unknown provider {other}"),
    }
}

/// One provider per process (`TALAAN_PK_EP`), so the memory numbers belong to it alone. The decoder/joint stays on
/// the processor (it is a loop of tiny calls); only the encoder moves.
#[cfg(feature = "parakeet-directml")]
#[test]
#[ignore]
fn parakeet_directml_live() {
    let kind = need("TALAAN_PK_EP");
    let truth = truth_words(&need("TALAAN_PK_TRUTH"));
    let audio = load_audio();
    let secs = audio.samples.len() as f64 / RATE as f64;
    let len: f32 = env("TALAAN_PK_LEN").map(|s| s.parse().unwrap()).unwrap_or(40.0);
    // "<provider>+joint": the decoder/joint session goes to the same provider (default: it stays on the processor).
    let joint = if kind.ends_with("+joint") { config_for(kind.trim_end_matches("+joint")) } else { parakeet_rs::ExecutionConfig::new().with_intra_threads(talaan_stt::threads()) };
    let kind_enc = kind.trim_end_matches("+joint").to_string();
    eprintln!("RESULT | pid {} provider {kind}", std::process::id());
    let before = memory_mb();
    let t = Instant::now();
    let loaded = ParakeetEngine::load_tdt_with(&model_dir(), "Parakeet", config_for(&kind_enc), joint);
    let load_s = t.elapsed().as_secs_f64();
    let mut engine = match loaded {
        Ok(e) => e,
        Err(e) => {
            eprintln!("RESULT | {kind}: load FAILED after {load_s:.2} s: {e}");
            let cpu = ParakeetEngine::load_tdt_with(&model_dir(), "Parakeet", config_for("cpu"), parakeet_rs::ExecutionConfig::new().with_intra_threads(talaan_stt::threads())).unwrap();
            eprintln!("RESULT | {kind}: the processor loads fine as the fallback ({:?})", cpu.label());
            return;
        }
    };
    let after_load = memory_mb();
    eprintln!("RESULT | {kind}: load {load_s:.2} s; process memory {:.0} MB before, {:.0} MB after load (private {:.0} MB)", before.0, after_load.0, after_load.2);
    // First call (sessions finish their setup here: on DirectML this builds the GPU graph for the shape).
    let warm = shape(&audio.spans, &audio.samples, Shape { max_len: (len * RATE as f32) as usize, ..Shape::parakeet() });
    let t = Instant::now();
    let warm_text = engine.transcribe(&audio.samples[warm[0].start..warm[0].end], Some("en")).unwrap();
    eprintln!("RESULT | {kind}: first chunk ({:.1} s of audio) {:.2} s, {} pieces", warm[0].len() as f64 / RATE as f64, t.elapsed().as_secs_f64(), warm_text.pieces.len());
    let reps: usize = env("TALAAN_PK_REPS").map(|s| s.parse().unwrap()).unwrap_or(2);
    let mut times = vec![];
    let mut last = None;
    for _ in 0..reps {
        let r = run_chunks(&mut engine, &audio, len);
        times.push(r.secs);
        last = Some(r);
    }
    let r = last.unwrap();
    let after_run = memory_mb();
    let best = times.iter().cloned().fold(f64::MAX, f64::min);
    eprintln!(
        "RESULT | {kind}: {} chunks of up to {len:.0} s over {secs:.0} s of audio | time {} s | {:.1}x real time | process memory {:.0} MB after, peak {:.0} MB, private {:.0} MB",
        r.chunks.len(),
        times.iter().map(|t| format!("{t:.2}")).collect::<Vec<_>>().join(" / "),
        secs / best,
        after_run.0,
        after_run.1,
        after_run.2
    );
    if env("TALAAN_PK_VERBOSE").is_some() {
        for (i, c) in r.chunks.iter().enumerate() {
            eprintln!("RESULT |   chunk {i}: at {:.1} s, {:.1} s long, {} pieces, {:.2} s", c.0, c.1, c.2, c.3);
        }
    }
    let s = score(&truth, &r.text);
    eprintln!("RESULT | {}", describe(&format!("{kind} vs script"), &s));
    if let Some(dir) = env("TALAAN_PK_OUT") {
        write_out(&format!("ep-{kind}"), &r.text);
        if let Ok(cpu) = std::fs::read_to_string(PathBuf::from(&dir).join("ep-cpu.txt")) {
            let cpu: Vec<String> = cpu.split_whitespace().map(String::from).collect();
            let d = score(&cpu, &r.text);
            eprintln!("RESULT | {}", describe(&format!("{kind} vs the processor's transcript"), &d));
            eprintln!("RESULT | {kind}: identical transcript to the processor: {}", cpu == r.text);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Tests of the scoring itself.

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<String> {
        words(s)
    }

    #[test]
    fn words_are_lower_case_without_punctuation() {
        assert_eq!(w("Okay, everyone: let's start. It's \u{201C}fine\u{201D}!"), ["okay", "everyone", "let's", "start", "it's", "fine"]);
        assert_eq!(w("a check-in on 4.2 percent"), ["a", "check", "in", "on", "four", "two", "percent"].map(String::from));
    }

    #[test]
    fn digits_become_words_like_the_script() {
        assert_eq!(w("47 steps"), w("forty seven steps"));
        assert_eq!(w("in 2015"), w("in twenty fifteen"));
        assert_eq!(w("$400,000"), w("four hundred thousand"));
        assert_eq!(w("7% more"), w("seven percent more"));
        assert_eq!(w("900 invoices"), w("nine hundred invoices"));
        assert_eq!(w("20"), w("twenty"));
        assert_eq!(w("100"), w("one hundred"));
    }

    #[test]
    fn a_perfect_transcript_scores_zero() {
        let r = w("the landing page is done");
        let s = score(&r, &r);
        assert_eq!((s.sub, s.del, s.ins, s.dropped_runs, s.repeat_runs), (0, 0, 0, 0, 0));
        assert_eq!(s.wer(), 0.0);
    }

    #[test]
    fn counts_wrong_missing_and_extra_words() {
        let s = score(&w("we ship on friday afternoon"), &w("we ship on monday"));
        assert_eq!((s.sub, s.del, s.ins), (1, 1, 0));
        let s = score(&w("we ship friday"), &w("we really ship friday"));
        assert_eq!((s.sub, s.del, s.ins), (0, 0, 1));
    }

    #[test]
    fn a_dropped_sentence_is_one_dropped_phrase() {
        let r = w("first we cleaned the data then we checked every total twice and finally we shipped it");
        let h = w("first we cleaned the data and finally we shipped it");
        let s = score(&r, &h);
        assert_eq!((s.dropped_runs, s.dropped_words), (1, 6));
        assert!(s.dropped_at[0].1.starts_with("then we checked"));
    }

    #[test]
    fn a_repeated_phrase_is_found_before_and_after() {
        let r = w("we agreed to move the launch to the twentieth of the month");
        let before = w("we agreed to move the launch move the launch to the twentieth of the month");
        let s = score(&r, &before);
        assert_eq!((s.repeat_runs, s.repeat_words), (1, 3));
        let unrelated = w("we agreed to move the launch quite late to the twentieth of the month");
        assert_eq!(score(&r, &unrelated).repeat_runs, 0);
    }

    #[test]
    fn noise_hits_the_requested_level() {
        let mut x: Vec<f32> = (0..16_000).map(|i| ((i as f32) * 0.05).sin() * 0.3).collect();
        let clean = x.clone();
        add_noise(&mut x, 20.0, 7);
        let p = |v: &[f32]| (v.iter().map(|a| a * a).sum::<f32>() / v.len() as f32).sqrt();
        let noise: Vec<f32> = x.iter().zip(&clean).map(|(a, b)| a - b).collect();
        let snr = 20.0 * (p(&clean) / p(&noise)).log10();
        assert!((snr - 20.0).abs() < 0.5, "snr {snr}");
    }
}
