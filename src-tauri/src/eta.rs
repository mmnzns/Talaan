//! Time left for transcription and summaries. Pure: no Tauri or storage types.
//!
//! An estimate is "fixed start-up cost + seconds of work per second of audio × audio length". The rate starts
//! from a default per engine, model and device (processor or graphics chip) and is replaced by what this computer
//! measured after each job (`learn`, stored per machine in `speed.json`). While a job runs, the time it has taken
//! so far for the share it has done takes over from the estimate (`remaining`).

use voice_engine_models::{Kind, ModelInfo};

/// Where the work runs. `Remote`: a cloud service or a local AI server, whose speed Talaan can't know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    Gpu,
    Cpu,
    Remote,
}

impl Device {
    fn tag(self) -> &'static str {
        match self {
            Device::Gpu => "gpu",
            Device::Cpu => "cpu",
            Device::Remote => "remote",
        }
    }
}

/// How one kind of job is timed on this computer.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    /// The `speed.json` key: engine, model and device.
    pub key: String,
    pub device: Device,
    /// Default seconds of work per second of audio, until this computer has measured one.
    pub rate: f64,
    /// Seconds of start-up (loading the model, starting the helper) on top of the audio.
    pub overhead: f64,
}

impl Profile {
    /// The whole job, in seconds, with the measured rate when there is one.
    pub fn total(&self, measured: Option<f64>, audio_secs: f64) -> f64 {
        self.overhead + measured.unwrap_or(self.rate) * audio_secs.max(0.0)
    }
}

/// Speech to text. Rates are per second of recording (both tracks are read; only speech is transcribed).
/// Measured: Whisper Large Turbo ran at about 3.3x real time on a Windows processor (0.3), Parakeet at about
/// 160 s per hour of audio (0.045); Metal and Vulkan Whisper are well under a minute per hour.
pub fn stt_profile(model: &ModelInfo, on_gpu: bool) -> Profile {
    let device = if on_gpu && model.kind == Kind::Whisper { Device::Gpu } else { Device::Cpu };
    // The engine's newer ids run like their family: "-full" is the uncompressed file of a model Talaan already
    // timed, "-en" the English-only version of a multilingual one. Both keep their own `speed.json` key.
    let family = model.id.trim_end_matches("-full");
    let family = if model.kind == Kind::Whisper { family.trim_end_matches("-en") } else { family };
    let cpu = match family {
        "whisper-tiny" => 0.04,
        "whisper-base" => 0.07,
        "whisper-small" => 0.18,
        "whisper-medium" => 0.55,
        "whisper-large-v2" | "whisper-large-v3" => 1.0,
        _ => match model.kind {
            Kind::Whisper => 0.3,
            Kind::Nemotron => 0.08,
            _ if model.id == "parakeet-ultra" => 0.09,
            _ => 0.045,
        },
    };
    let rate = if device == Device::Gpu { (cpu / 6.0_f64).max(0.01) } else { cpu };
    Profile { key: format!("stt:{}:{}", model.id, device.tag()), device, rate, overhead: 10.0 }
}

/// Summaries. Rates are per second of speech in the transcript. Measured on Windows: Qwen3.5 4B took 10.5 min on
/// the processor and 28 s on the graphics chip for a one-hour call.
/// `engine`: "local" | "ollama" | "byok"; `model`: the local model id or the provider.
pub fn summary_profile(engine: &str, model: &str, on_gpu: bool) -> Profile {
    let (device, rate, overhead) = match engine {
        "local" => {
            let larger = model.contains("9b");
            match (on_gpu, larger) {
                (true, false) => (Device::Gpu, 0.01, 15.0),
                (true, true) => (Device::Gpu, 0.02, 20.0),
                (false, false) => (Device::Cpu, 0.18, 20.0),
                (false, true) => (Device::Cpu, 0.4, 30.0),
            }
        }
        "ollama" => (Device::Remote, 0.05, 10.0),
        _ => (Device::Remote, 0.008, 8.0),
    };
    Profile { key: format!("summary:{engine}:{model}:{}", device.tag()), device, rate, overhead }
}

/// Jobs on less audio than this don't teach the rate anything (start-up dominates).
pub const MIN_LEARN_SECS: f64 = 60.0;

/// The new rate after a job: the first measurement replaces the default, later ones move it halfway, so one odd
/// run (the computer was busy) doesn't throw the next estimate far off. None when the job was too short to say.
pub fn learn(old: Option<f64>, work_secs: f64, audio_secs: f64) -> Option<f64> {
    if audio_secs < MIN_LEARN_SECS || !work_secs.is_finite() || work_secs <= 0.0 {
        return None;
    }
    let measured = (work_secs / audio_secs).clamp(0.001, 20.0);
    Some(match old {
        Some(o) if o.is_finite() && o > 0.0 => (o + measured) / 2.0,
        _ => measured,
    })
}

/// Seconds left. `total`: the estimate for the whole job; `elapsed`: since it started; `progress`: 0..1;
/// `running`: seconds since progress began (after loading). Early on the estimate counts down; once a share of the
/// work is done, the pace so far takes over.
pub fn remaining(total: f64, elapsed: f64, progress: f64, running: f64) -> f64 {
    let by_estimate = (total - elapsed).max(0.0);
    let p = progress.clamp(0.0, 1.0);
    if p >= 1.0 {
        return 0.0;
    }
    if p < 0.05 || running < 5.0 {
        return by_estimate;
    }
    let by_pace = running * (1.0 - p) / p;
    let trust = ((p - 0.05) / 0.25).clamp(0.0, 1.0);
    trust * by_pace + (1.0 - trust) * by_estimate
}

#[cfg(test)]
mod tests {
    use super::*;
    use voice_engine_models::catalog::find;

    #[test]
    fn whisper_on_the_processor_is_much_slower_than_on_the_graphics_chip() {
        let turbo = find("whisper-turbo").unwrap();
        let cpu = stt_profile(turbo, false);
        let gpu = stt_profile(turbo, true);
        assert_eq!((cpu.device, gpu.device), (Device::Cpu, Device::Gpu));
        assert_eq!(cpu.key, "stt:whisper-turbo:cpu");
        // One hour on the processor: about 18 min; on the graphics chip: a few minutes at most.
        let hour = 3600.0;
        assert!((15.0 * 60.0..25.0 * 60.0).contains(&cpu.total(None, hour)), "{}", cpu.total(None, hour));
        assert!(gpu.total(None, hour) < 5.0 * 60.0);
    }

    #[test]
    fn parakeet_always_runs_on_the_processor() {
        let p = stt_profile(find("parakeet-v3").unwrap(), true);
        assert_eq!(p.device, Device::Cpu);
        assert!(p.total(None, 3600.0) < 4.0 * 60.0);
    }

    #[test]
    fn a_local_summary_on_the_processor_takes_minutes_and_a_cloud_one_seconds() {
        let cpu = summary_profile("local", "qwen3.5-4b", false);
        assert_eq!(cpu.device, Device::Cpu);
        assert!((8.0 * 60.0..14.0 * 60.0).contains(&cpu.total(None, 3600.0)));
        let gpu = summary_profile("local", "qwen3.5-4b", true);
        assert!(gpu.total(None, 3600.0) < 90.0);
        let cloud = summary_profile("byok", "claude", true);
        assert_eq!(cloud.device, Device::Remote);
        assert!(cloud.total(None, 3600.0) < 60.0);
        assert_ne!(cpu.key, gpu.key);
    }

    #[test]
    fn a_measured_rate_replaces_the_default() {
        let p = stt_profile(find("whisper-turbo").unwrap(), false);
        assert_eq!(p.total(Some(0.1), 1000.0), 10.0 + 100.0);
    }

    #[test]
    fn learning_starts_from_the_first_job_and_then_averages() {
        assert_eq!(learn(None, 600.0, 3600.0), Some(600.0 / 3600.0));
        let r = learn(Some(0.3), 360.0, 3600.0).unwrap();
        assert!((r - 0.2).abs() < 1e-9);
        // Too short to say anything, or a broken measurement.
        assert_eq!(learn(Some(0.3), 5.0, 20.0), None);
        assert_eq!(learn(Some(0.3), 0.0, 3600.0), None);
        assert_eq!(learn(Some(0.3), f64::NAN, 3600.0), None);
        // A garbage stored value doesn't poison the next one.
        assert_eq!(learn(Some(f64::NAN), 360.0, 3600.0), Some(0.1));
    }

    #[test]
    fn remaining_counts_down_then_follows_the_pace() {
        // Nothing done yet: the estimate counts down and never goes below zero.
        assert_eq!(remaining(600.0, 100.0, 0.0, 0.0), 500.0);
        assert_eq!(remaining(60.0, 100.0, 0.0, 0.0), 0.0);
        // Half done after 100 s of work: about 100 s left, whatever the estimate said.
        assert!((remaining(2000.0, 110.0, 0.5, 100.0) - 100.0).abs() < 1e-9);
        // Early on, a mix of both.
        let early = remaining(600.0, 20.0, 0.1, 15.0);
        assert!(early > 135.0 && early < 580.0, "{early}");
        assert_eq!(remaining(600.0, 20.0, 1.0, 15.0), 0.0);
    }
}
