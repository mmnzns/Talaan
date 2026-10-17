//! `speed.json`: how fast this computer transcribes and summarizes, learned after each job (`crate::eta`).
//! `{ "rates": { "stt:whisper-turbo:cpu": 0.31, ... } }`, seconds of work per second of audio.

use super::io::{read_json_opt, write_json_atomic};
use super::model::{Eta, MeetingFile};
use super::paths::SPEED;
use super::Storage;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Default)]
struct Speeds {
    #[serde(default)]
    rates: BTreeMap<String, f64>,
}

impl Storage {
    fn speeds(&self) -> Speeds {
        read_json_opt::<Speeds>(&self.root.join(SPEED)).ok().flatten().unwrap_or_default()
    }

    /// The rate this computer measured for `key`, if any.
    pub fn speed_rate(&self, key: &str) -> Option<f64> {
        self.speeds().rates.get(key).copied().filter(|r| r.is_finite() && *r > 0.0)
    }

    /// Records a finished job (`eta::learn`). Never fails the caller: a lost measurement only costs accuracy.
    pub fn learn_speed(&self, key: &str, work_secs: f64, audio_secs: f64) {
        let mut s = self.speeds();
        if let Some(rate) = crate::eta::learn(s.rates.get(key).copied(), work_secs, audio_secs) {
            s.rates.insert(key.to_string(), rate);
            let _ = write_json_atomic(&self.root.join(SPEED), &s);
        }
    }
}

/// When something `secs_left` from now should be done.
pub fn eta_in(secs_left: f64, on_cpu: bool) -> Eta {
    let left = chrono::Duration::milliseconds((secs_left.max(0.0) * 1000.0).min(48.0 * 3600.0 * 1000.0) as i64);
    Eta { finish_at: (chrono::Local::now() + left).fixed_offset().to_rfc3339(), on_cpu }
}

/// Sets when the current step should be done, `secs_left` from now.
pub fn set_eta(f: &mut MeetingFile, secs_left: f64, on_cpu: bool) {
    f.eta = Some(eta_in(secs_left, on_cpu));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_are_learned_and_kept_per_key() {
        let dir = tempfile::tempdir().unwrap();
        let s = Storage::new(dir.path().to_path_buf()).unwrap();
        assert_eq!(s.speed_rate("stt:whisper-turbo:cpu"), None);
        s.learn_speed("stt:whisper-turbo:cpu", 1200.0, 3600.0);
        s.learn_speed("summary:local:qwen3.5-4b:cpu", 10.0, 10.0); // too short: ignored
        let s = Storage::new(dir.path().to_path_buf()).unwrap();
        assert!((s.speed_rate("stt:whisper-turbo:cpu").unwrap() - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(s.speed_rate("summary:local:qwen3.5-4b:cpu"), None);
        s.learn_speed("stt:whisper-turbo:cpu", 0.0, 3600.0);
        assert!(s.speed_rate("stt:whisper-turbo:cpu").is_some());
    }

    #[test]
    fn a_broken_file_reads_as_no_measurements() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(SPEED), "not json").unwrap();
        let s = Storage::new(dir.path().to_path_buf()).unwrap();
        assert_eq!(s.speed_rate("x"), None);
        s.learn_speed("x", 120.0, 120.0);
        assert_eq!(s.speed_rate("x"), Some(1.0));
    }
}
