//! Finding speech in a track. Only speech goes to the engine: silence costs time and makes models invent text
//! ("Thank you." on a quiet line). Spans are sample ranges at 16 kHz on the recording clock.

use std::path::Path;

pub const RATE: usize = 16_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn len(&self) -> usize {
        self.end - self.start
    }
    pub fn start_ms(&self) -> u64 {
        (self.start * 1000 / RATE) as u64
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Shape {
    /// Added before and after each detected span (words start a little before the detector notices).
    pub pad: usize,
    /// Spans closer than this are joined (one sentence with a breath stays one chunk).
    pub join_gap: usize,
    /// Longer spans are split at their quietest point near this length (engines work on bounded chunks).
    pub max_len: usize,
    /// Shorter spans are dropped (clicks, coughs).
    pub min_len: usize,
}

impl Default for Shape {
    fn default() -> Self {
        Shape { pad: RATE / 5, join_gap: RATE / 2, max_len: 25 * RATE, min_len: RATE / 4 }
    }
}

/// Pads, joins, splits and filters raw detector spans. `samples` is the whole track (for quiet split points).
pub fn shape(raw: &[Span], samples: &[f32], s: Shape) -> Vec<Span> {
    let len = samples.len();
    let mut joined: Vec<Span> = vec![];
    for r in raw {
        let span = Span { start: r.start.saturating_sub(s.pad), end: (r.end + s.pad).min(len) };
        if span.end <= span.start {
            continue;
        }
        match joined.last_mut() {
            Some(last) if span.start <= last.end + s.join_gap => last.end = last.end.max(span.end),
            _ => joined.push(span),
        }
    }
    let mut out = vec![];
    for span in joined {
        split(span, samples, s.max_len, &mut out);
    }
    out.retain(|sp| sp.len() >= s.min_len);
    out
}

/// Splits at the quietest 100 ms window in the last fifth before `max_len`, repeatedly.
fn split(mut span: Span, samples: &[f32], max_len: usize, out: &mut Vec<Span>) {
    const WIN: usize = RATE / 10;
    while span.len() > max_len {
        let search_from = span.start + max_len * 4 / 5;
        let search_to = span.start + max_len - WIN;
        let mut best = (f32::MAX, span.start + max_len);
        let mut at = search_from;
        while at <= search_to {
            let e: f32 = samples[at..at + WIN].iter().map(|x| x * x).sum();
            if e < best.0 {
                best = (e, at + WIN / 2);
            }
            at += WIN / 2;
        }
        out.push(Span { start: span.start, end: best.1 });
        span.start = best.1;
    }
    out.push(span);
}

/// A 16 kHz mono 16-bit WAV (what the recorder writes) as f32 samples. Repairs are done by recovery, so a
/// header whose length is wrong is tolerated: everything after the header is read.
pub fn read_track(path: &Path) -> std::io::Result<Vec<f32>> {
    let bytes = std::fs::read(path)?;
    let data = data_offset(&bytes).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "not a WAV file"))?;
    Ok(bytes[data..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect())
}

fn data_offset(b: &[u8]) -> Option<usize> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let mut at = 12;
    while at + 8 <= b.len() {
        let size = u32::from_le_bytes([b[at + 4], b[at + 5], b[at + 6], b[at + 7]]) as usize;
        if &b[at..at + 4] == b"data" {
            return Some(at + 8);
        }
        at += 8 + size + (size & 1);
    }
    None
}

/// Silero speech detection through whisper.cpp's VAD.
pub struct Vad {
    ctx: whisper_rs::WhisperVadContext,
}

impl Vad {
    pub fn load(model: &Path) -> Result<Self, String> {
        let mut p = whisper_rs::WhisperVadContextParams::new();
        p.set_n_threads(4);
        p.set_use_gpu(false);
        let ctx = whisper_rs::WhisperVadContext::new(&model.to_string_lossy(), p).map_err(|e| format!("couldn't load voice detection: {e}"))?;
        Ok(Vad { ctx })
    }

    /// Raw speech spans in samples.
    pub fn detect(&mut self, samples: &[f32]) -> Result<Vec<Span>, String> {
        if samples.is_empty() {
            return Ok(vec![]);
        }
        let mut params = whisper_rs::WhisperVadParams::new();
        params.set_threshold(0.5);
        params.set_min_speech_duration(250);
        params.set_min_silence_duration(300);
        params.set_speech_pad(0);
        let segs = self.ctx.segments_from_samples(params, samples).map_err(|e| format!("voice detection failed: {e}"))?;
        let mut out = vec![];
        for i in 0..segs.num_segments() {
            let (Some(t0), Some(t1)) = (segs.get_segment_start_timestamp(i), segs.get_segment_end_timestamp(i)) else { continue };
            // whisper.cpp reports centiseconds.
            let start = ((t0 as f64 / 100.0) * RATE as f64) as usize;
            let end = (((t1 as f64 / 100.0) * RATE as f64) as usize).min(samples.len());
            if end > start {
                out.push(Span { start, end });
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp(a: f32, b: f32) -> Span {
        Span { start: (a * RATE as f32) as usize, end: (b * RATE as f32) as usize }
    }

    #[test]
    fn pads_and_joins_close_spans() {
        let samples = vec![0.0; 20 * RATE];
        let out = shape(&[sp(1.0, 2.0), sp(2.6, 3.0), sp(8.0, 9.0)], &samples, Shape::default());
        assert_eq!(out, vec![sp(0.8, 3.2), sp(7.8, 9.2)]);
    }

    #[test]
    fn padding_stays_inside_the_track() {
        let samples = vec![0.0; 3 * RATE];
        let out = shape(&[sp(0.05, 2.95)], &samples, Shape::default());
        assert_eq!(out, vec![Span { start: 0, end: 3 * RATE }]);
    }

    #[test]
    fn drops_tiny_spans() {
        let samples = vec![0.0; 5 * RATE];
        let s = Shape { pad: 0, ..Shape::default() };
        assert!(shape(&[sp(1.0, 1.1)], &samples, s).is_empty());
    }

    #[test]
    fn long_speech_is_split_at_the_quietest_point() {
        // 60 s of "speech" with a quiet 100 ms at 22 s and another at 45 s.
        let mut samples = vec![0.5f32; 60 * RATE];
        for q in [22 * RATE, 45 * RATE] {
            samples[q..q + RATE / 10].iter_mut().for_each(|x| *x = 0.0);
        }
        let s = Shape { pad: 0, ..Shape::default() };
        let out = shape(&[Span { start: 0, end: 60 * RATE }], &samples, s);
        assert_eq!(out.len(), 3);
        assert!((out[0].end as i64 - (22 * RATE + RATE / 20) as i64).abs() <= (RATE / 20) as i64, "{:?}", out);
        assert!(out.iter().all(|sp| sp.len() <= s.max_len));
        assert_eq!(out.last().unwrap().end, 60 * RATE);
        assert!(out.windows(2).all(|w| w[0].end == w[1].start));
    }

    #[test]
    fn reads_a_recorder_wav() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: 16000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..1600 {
            w.write_sample(if i % 2 == 0 { 16384i16 } else { -16384 }).unwrap();
        }
        w.finalize().unwrap();
        let s = read_track(&path).unwrap();
        assert_eq!(s.len(), 1600);
        assert_eq!(s[0], 0.5);
        assert_eq!(s[1], -0.5);
    }

    /// `TALAAN_SMOKE_MEETING=<meeting dir> cargo test vad_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn vad_smoke() {
        let dir = std::path::PathBuf::from(std::env::var("TALAAN_SMOKE_MEETING").unwrap());
        let data = dirs::data_dir().unwrap().join("Talaan");
        let model = data.join("models/vad/ggml-silero-v6.2.0.bin");
        let mut vad = Vad::load(&model).unwrap();
        for track in ["me.wav", "them.wav"] {
            let samples = read_track(&dir.join(track)).unwrap();
            let t = std::time::Instant::now();
            let raw = vad.detect(&samples).unwrap();
            let spans = shape(&raw, &samples, Shape::default());
            let speech: usize = spans.iter().map(|s| s.len()).sum();
            eprintln!(
                "{track}: {:.0}s audio, {} raw, {} chunks, {:.0}s speech, vad took {:.2}s; first: {:?}",
                samples.len() as f32 / RATE as f32,
                raw.len(),
                spans.len(),
                speech as f32 / RATE as f32,
                t.elapsed().as_secs_f32(),
                spans.iter().take(6).map(|s| (s.start as f32 / RATE as f32, s.end as f32 / RATE as f32)).collect::<Vec<_>>()
            );
        }
    }
}
