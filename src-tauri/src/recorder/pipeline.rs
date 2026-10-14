//! One track: device buffers in (with their recording-clock position), aligned 16 kHz samples out.
//!
//! Within a run of continuous audio, positions come from counting samples (smooth). Each buffer's own
//! clock position is only used to start a run and to detect a break: a pause, a device switch, dropped
//! audio or a different sample rate. A break ends the run; the next one starts at the clock position.

use super::clock::RATE;
use super::levels::LevelMeter;
use super::resample::{to_i16, Resampler16k};
use super::timeline::{SampleSink, TimelineWriter};
use std::io;

/// How far counted and clock positions may drift before a new run starts (0.25 s).
pub const RESYNC_SAMPLES: u64 = RATE as u64 / 4;

struct Run {
    base: u64,
    channels: usize,
    in_frames: u64,
    out_written: u64,
    resampler: Resampler16k,
}

impl Run {
    fn expected_pos(&self) -> u64 {
        self.base + self.in_frames * RATE as u64 / self.resampler.in_rate() as u64
    }
}

pub struct TrackPipeline<S: SampleSink> {
    timeline: TimelineWriter<S>,
    run: Option<Run>,
    meter: LevelMeter,
}

impl<S: SampleSink> TrackPipeline<S> {
    pub fn new(sink: S) -> Self {
        TrackPipeline { timeline: TimelineWriter::new(sink), run: None, meter: LevelMeter::default() }
    }

    /// `pos` is the recording-clock position (16 kHz samples) of the buffer's first frame, `None` while paused.
    pub fn push(&mut self, pos: Option<u64>, rate: u32, channels: usize, interleaved: &[f32]) -> io::Result<()> {
        let Some(pos) = pos else {
            return self.end_run();
        };
        if let Some(run) = &self.run {
            let drift = run.expected_pos().abs_diff(pos);
            if run.resampler.in_rate() != rate || run.channels != channels || drift > RESYNC_SAMPLES {
                self.end_run()?;
            }
        }
        let run = self.run.get_or_insert_with(|| Run { base: pos, channels, in_frames: 0, out_written: 0, resampler: Resampler16k::new(rate, channels) });
        run.in_frames += (interleaved.len() / channels.max(1)) as u64;
        let out = run.resampler.push(interleaved);
        let at = run.base + run.out_written;
        run.out_written += out.len() as u64;
        self.meter.feed(&out);
        self.timeline.write_at(at, &to_i16(&out))
    }

    /// Flushes the current run (on pause, device switch or stop).
    pub fn end_run(&mut self) -> io::Result<()> {
        if let Some(mut run) = self.run.take() {
            let tail = run.resampler.finish();
            let at = run.base + run.out_written;
            self.timeline.write_at(at, &to_i16(&tail))?;
        }
        Ok(())
    }

    /// Ends the track at `end_pos` so both tracks have the same length.
    pub fn finish(&mut self, end_pos: u64) -> io::Result<()> {
        self.end_run()?;
        self.timeline.pad_to(end_pos)
    }

    pub fn written(&self) -> u64 {
        self.timeline.written()
    }

    pub fn level(&mut self) -> f32 {
        self.meter.take()
    }

    pub fn into_sink(self) -> S {
        self.timeline.into_sink()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds `secs` of audio at `rate` in 10 ms buffers whose positions follow the clock exactly.
    fn feed(p: &mut TrackPipeline<Vec<i16>>, start_pos: u64, rate: u32, secs: f32, value: f32) {
        let per = (rate / 100) as usize;
        let n = (secs * 100.0) as u64;
        for i in 0..n {
            let pos = start_pos + i * RATE as u64 / 100;
            p.push(Some(pos), rate, 1, &vec![value; per]).unwrap();
        }
    }

    #[test]
    fn continuous_audio_has_the_right_length() {
        let mut p = TrackPipeline::new(vec![]);
        feed(&mut p, 0, 48_000, 3.0, 0.1);
        p.finish(0).unwrap();
        assert_eq!(p.into_sink().len(), 48_000);
    }

    #[test]
    fn a_dropout_becomes_silence_at_the_right_place() {
        let mut p = TrackPipeline::new(vec![]);
        feed(&mut p, 0, 16_000, 1.0, 0.5);
        feed(&mut p, 32_000, 16_000, 1.0, 0.5); // one second missing
        p.finish(0).unwrap();
        let out = p.into_sink();
        assert_eq!(out.len(), 48_000);
        assert!(out[16_000..32_000].iter().all(|s| *s == 0));
        assert!(out[32_000] != 0 && out[15_999] != 0);
    }

    #[test]
    fn a_pause_does_not_leave_a_hole() {
        // The clock removes paused time, so audio after the pause continues where it stopped.
        let mut p = TrackPipeline::new(vec![]);
        feed(&mut p, 0, 16_000, 1.0, 0.5);
        p.push(None, 16_000, 1, &[0.5; 160]).unwrap();
        feed(&mut p, 16_000, 16_000, 1.0, 0.5);
        p.finish(0).unwrap();
        let out = p.into_sink();
        assert_eq!(out.len(), 32_000);
        assert!(out.iter().all(|s| *s != 0));
    }

    #[test]
    fn a_device_switch_to_another_rate_keeps_the_timeline() {
        let mut p = TrackPipeline::new(vec![]);
        feed(&mut p, 0, 48_000, 1.0, 0.2); // MacBook mic
        feed(&mut p, 16_000, 24_000, 1.0, 0.2); // AirPods
        p.finish(0).unwrap();
        assert_eq!(p.into_sink().len(), 32_000);
    }

    #[test]
    fn two_tracks_with_different_latency_line_up() {
        // The same click reaches the mic track 120 ms later in device terms, but both buffers carry clock positions.
        let mut me = TrackPipeline::new(vec![]);
        let mut them = TrackPipeline::new(vec![]);
        let click_at: u64 = 16_000; // 1 s into the recording
        for i in 0..300u64 {
            let pos = i * 160;
            let mut buf = vec![0.0f32; 160];
            if pos <= click_at && click_at < pos + 160 {
                buf[(click_at - pos) as usize] = 0.9;
            }
            me.push(Some(pos), 16_000, 1, &buf).unwrap();
            if pos >= 1_920 {
                them.push(Some(pos), 16_000, 1, &buf).unwrap(); // starts 120 ms late
            }
        }
        me.finish(48_000).unwrap();
        them.finish(48_000).unwrap();
        let (a, b) = (me.into_sink(), them.into_sink());
        assert_eq!(a.len(), b.len());
        let peak = |v: &[i16]| v.iter().enumerate().max_by_key(|(_, s)| s.abs()).unwrap().0;
        assert_eq!(peak(&a), click_at as usize);
        assert_eq!(peak(&b), click_at as usize);
    }

    #[test]
    fn small_jitter_does_not_restart_the_run() {
        let mut p = TrackPipeline::new(vec![]);
        for i in 0..100u64 {
            let jitter = if i % 2 == 1 { 40 } else { 0 };
            p.push(Some(i * 160 + jitter), 16_000, 1, &[0.3; 160]).unwrap();
        }
        p.finish(0).unwrap();
        assert_eq!(p.into_sink().len(), 16_000);
    }

    #[test]
    fn levels_follow_the_audio() {
        let mut p = TrackPipeline::new(vec![]);
        feed(&mut p, 0, 16_000, 0.1, 0.5);
        assert!(p.level() > 0.8);
        assert_eq!(p.level(), 0.0);
    }
}
