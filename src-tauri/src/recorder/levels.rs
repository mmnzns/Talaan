//! Loudness for the level bars: RMS over a short window, mapped to 0..1 on a -60..0 dB scale.

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

pub fn to_level(rms: f32) -> f32 {
    if rms <= 0.0 {
        return 0.0;
    }
    ((20.0 * rms.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
}

/// Keeps the loudest recent level so the UI can poll it at its own pace.
#[derive(Default)]
pub struct LevelMeter {
    peak: f32,
}

impl LevelMeter {
    pub fn feed(&mut self, samples: &[f32]) {
        self.peak = self.peak.max(to_level(rms(samples)));
    }

    /// Returns the level since the last call and resets it.
    pub fn take(&mut self) -> f32 {
        std::mem::take(&mut self.peak)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_is_zero_and_full_scale_is_one() {
        assert_eq!(to_level(rms(&[0.0; 100])), 0.0);
        assert!((to_level(rms(&[1.0, -1.0, 1.0, -1.0])) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn quiet_speech_lands_in_the_middle() {
        let l = to_level(0.03); // about -30 dB
        assert!((0.4..0.6).contains(&l), "{l}");
    }

    #[test]
    fn the_meter_keeps_the_peak_until_taken() {
        let mut m = LevelMeter::default();
        m.feed(&[0.5; 10]);
        m.feed(&[0.01; 10]);
        assert!(m.take() > 0.8);
        assert_eq!(m.take(), 0.0);
    }
}
