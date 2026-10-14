//! Any device rate and channel count in, 16 kHz mono f32 out.

use super::clock::RATE;
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

const CHUNK: usize = 1024;

pub struct Resampler16k {
    in_rate: u32,
    channels: usize,
    inner: Option<Fft<f32>>,
    pending: Vec<f32>,
    out_buf: Vec<f32>,
    /// Output frames still to drop at the start (the resampler's delay).
    to_trim: usize,
    in_frames: u64,
    out_frames: u64,
}

impl Resampler16k {
    pub fn new(in_rate: u32, channels: usize) -> Self {
        let inner = (in_rate != RATE).then(|| Fft::<f32>::new(in_rate as usize, RATE as usize, CHUNK, 1, FixedSync::Input).expect("valid resampler"));
        let to_trim = inner.as_ref().map_or(0, |r| r.output_delay());
        let out_max = inner.as_ref().map_or(0, |r| r.output_frames_max());
        Resampler16k { in_rate, channels: channels.max(1), inner, pending: vec![], out_buf: vec![0.0; out_max], to_trim, in_frames: 0, out_frames: 0 }
    }

    pub fn in_rate(&self) -> u32 {
        self.in_rate
    }

    /// Output frames this input should produce in total (used to cut the tail exactly).
    fn expected_out(&self) -> u64 {
        self.in_frames * RATE as u64 / self.in_rate as u64
    }

    /// Interleaved input frames; returns the 16 kHz mono output produced so far.
    pub fn push(&mut self, interleaved: &[f32]) -> Vec<f32> {
        let frames = interleaved.len() / self.channels;
        self.in_frames += frames as u64;
        let mono = interleaved.chunks_exact(self.channels).map(|f| f.iter().sum::<f32>() / self.channels as f32);
        if self.inner.is_none() {
            let out: Vec<f32> = mono.collect();
            self.out_frames += out.len() as u64;
            return out;
        }
        self.pending.extend(mono);
        let mut out = vec![];
        while self.pending.len() >= CHUNK {
            self.process_chunk(&mut out, None);
        }
        out
    }

    /// Flushes what is still inside the resampler. Output length matches the input length exactly.
    pub fn finish(&mut self) -> Vec<f32> {
        let mut out = vec![];
        if self.inner.is_none() {
            return out;
        }
        let mut guard = 0;
        while self.out_frames < self.expected_out() && guard < 64 {
            let partial = self.pending.len().min(CHUNK);
            self.pending.resize(CHUNK, 0.0);
            self.process_chunk(&mut out, Some(partial));
            guard += 1;
        }
        let excess = self.out_frames.saturating_sub(self.expected_out()) as usize;
        out.truncate(out.len().saturating_sub(excess));
        self.out_frames -= excess as u64;
        out
    }

    fn process_chunk(&mut self, out: &mut Vec<f32>, partial: Option<usize>) {
        let r = self.inner.as_mut().unwrap();
        let input = InterleavedSlice::new(&self.pending[..CHUNK], 1, CHUNK).unwrap();
        let cap = self.out_buf.len();
        let mut output = InterleavedSlice::new_mut(&mut self.out_buf, 1, cap).unwrap();
        let indexing = partial.map(|p| rubato::Indexing::new().partial_len(p));
        let (_read, written) = r.process_into_buffer(&input, &mut output, indexing.as_ref()).unwrap();
        self.pending.drain(..CHUNK);
        let produced = &self.out_buf[..written];
        let skip = self.to_trim.min(produced.len());
        self.to_trim -= skip;
        out.extend_from_slice(&produced[skip..]);
        self.out_frames += (written - skip) as u64;
    }
}

pub fn to_i16(samples: &[f32]) -> Vec<i16> {
    samples.iter().map(|s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, secs: f32, hz: f32, channels: usize) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        (0..n).flat_map(|i| std::iter::repeat((2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin() * 0.5).take(channels)).collect()
    }

    fn run(rate: u32, channels: usize, input: &[f32], piece: usize) -> Vec<f32> {
        let mut r = Resampler16k::new(rate, channels);
        let mut out = vec![];
        for chunk in input.chunks(piece * channels) {
            out.extend(r.push(chunk));
        }
        out.extend(r.finish());
        out
    }

    #[test]
    fn length_matches_the_duration_exactly() {
        for (rate, ch) in [(48_000, 2), (44_100, 1), (16_000, 1), (24_000, 2)] {
            let out = run(rate, ch, &tone(rate, 2.0, 440.0, ch), 512);
            assert_eq!(out.len(), 32_000, "{rate} Hz x{ch}");
        }
    }

    #[test]
    fn a_tone_survives_resampling() {
        let out = run(48_000, 2, &tone(48_000, 1.0, 1_000.0, 2), 480);
        // energy in the middle (away from edges) is roughly that of a 0.5 amplitude sine
        let mid = &out[4_000..12_000];
        let rms = (mid.iter().map(|s| s * s).sum::<f32>() / mid.len() as f32).sqrt();
        assert!((rms - 0.353).abs() < 0.05, "rms {rms}");
        // and it crosses zero about 2000 times per second (1 kHz)
        let crossings = mid.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        assert!((480..=520).contains(&crossings), "{crossings}");
    }

    #[test]
    fn stereo_is_averaged_to_mono() {
        let mut r = Resampler16k::new(16_000, 2);
        assert_eq!(r.push(&[1.0, 0.0, 0.5, 0.5]), vec![0.5, 0.5]);
    }

    #[test]
    fn conversion_to_i16_clamps() {
        assert_eq!(to_i16(&[0.0, 2.0, -2.0]), vec![0, i16::MAX, -i16::MAX]);
    }
}
