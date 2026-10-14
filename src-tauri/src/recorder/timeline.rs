//! Writes samples at positions on the recording timeline: gaps become silence, overlaps are trimmed.

/// Where 16 kHz mono samples end up (a WAV file, or a Vec in tests).
pub trait SampleSink {
    fn write(&mut self, samples: &[i16]) -> std::io::Result<()>;
}

impl SampleSink for Vec<i16> {
    fn write(&mut self, samples: &[i16]) -> std::io::Result<()> {
        self.extend_from_slice(samples);
        Ok(())
    }
}

/// Gaps longer than this are treated as a clock glitch and capped, so a bad timestamp can't fill the disk.
pub const MAX_GAP_SAMPLES: u64 = 16_000 * 60 * 15;

pub struct TimelineWriter<S: SampleSink> {
    sink: S,
    written: u64,
}

impl<S: SampleSink> TimelineWriter<S> {
    pub fn new(sink: S) -> Self {
        TimelineWriter { sink, written: 0 }
    }

    pub fn written(&self) -> u64 {
        self.written
    }

    pub fn into_sink(self) -> S {
        self.sink
    }

    /// Places `samples` so the first one lands at `pos`.
    pub fn write_at(&mut self, pos: u64, samples: &[i16]) -> std::io::Result<()> {
        if pos > self.written {
            let gap = (pos - self.written).min(MAX_GAP_SAMPLES);
            self.pad(gap)?;
        }
        let skip = self.written.saturating_sub(pos) as usize;
        if skip < samples.len() {
            self.sink.write(&samples[skip..])?;
            self.written += (samples.len() - skip) as u64;
        }
        Ok(())
    }

    /// Fills with silence up to `pos` (used at stop so both tracks end together).
    pub fn pad_to(&mut self, pos: u64) -> std::io::Result<()> {
        if pos > self.written {
            self.pad((pos - self.written).min(MAX_GAP_SAMPLES))?;
        }
        Ok(())
    }

    fn pad(&mut self, n: u64) -> std::io::Result<()> {
        const ZEROS: [i16; 4096] = [0; 4096];
        let mut left = n;
        while left > 0 {
            let k = left.min(ZEROS.len() as u64) as usize;
            self.sink.write(&ZEROS[..k])?;
            left -= k as u64;
        }
        self.written += n;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contiguous_writes_append() {
        let mut w = TimelineWriter::new(vec![]);
        w.write_at(0, &[1, 2]).unwrap();
        w.write_at(2, &[3]).unwrap();
        assert_eq!(w.into_sink(), vec![1, 2, 3]);
    }

    #[test]
    fn a_gap_becomes_silence() {
        let mut w = TimelineWriter::new(vec![]);
        w.write_at(0, &[1]).unwrap();
        w.write_at(4, &[5]).unwrap();
        assert_eq!(w.into_sink(), vec![1, 0, 0, 0, 5]);
    }

    #[test]
    fn a_late_start_is_padded() {
        let mut w = TimelineWriter::new(vec![]);
        w.write_at(3, &[7, 8]).unwrap();
        assert_eq!(w.into_sink(), vec![0, 0, 0, 7, 8]);
    }

    #[test]
    fn overlap_is_trimmed() {
        let mut w = TimelineWriter::new(vec![]);
        w.write_at(0, &[1, 2, 3]).unwrap();
        w.write_at(1, &[9, 9, 4, 5]).unwrap();
        assert_eq!(w.into_sink(), vec![1, 2, 3, 4, 5]);
        let mut w = TimelineWriter::new(vec![]);
        w.write_at(0, &[1, 2, 3]).unwrap();
        w.write_at(0, &[9]).unwrap();
        assert_eq!(w.written(), 3);
    }

    #[test]
    fn huge_gaps_are_capped() {
        let mut w = TimelineWriter::new(vec![]);
        w.write_at(MAX_GAP_SAMPLES * 4, &[1]).unwrap();
        assert_eq!(w.written(), MAX_GAP_SAMPLES + 1);
    }

    #[test]
    fn pad_to_extends_with_silence() {
        let mut w = TimelineWriter::new(vec![]);
        w.write_at(0, &[1]).unwrap();
        w.pad_to(3).unwrap();
        w.pad_to(2).unwrap();
        assert_eq!(w.into_sink(), vec![1, 0, 0]);
    }
}
