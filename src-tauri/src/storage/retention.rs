//! How long audio and transcripts are kept. The pass that enforces it is added with the retention task.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioPolicy {
    Days(u32),
    Forever,
    /// "Don't keep": audio is deleted as soon as the retention pass runs.
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptPolicy {
    Forever,
    Days(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub audio: AudioPolicy,
    pub transcripts: TranscriptPolicy,
}

impl Default for Policy {
    fn default() -> Self {
        Policy { audio: AudioPolicy::Days(7), transcripts: TranscriptPolicy::Forever }
    }
}

impl Policy {
    /// From the frontend settings values (`retention`, `transcriptRetention`). Unknown values fall back to the defaults.
    pub fn from_settings(audio: &str, transcripts: &str) -> Policy {
        let d = Policy::default();
        Policy {
            audio: match audio {
                "1d" => AudioPolicy::Days(1),
                "7d" => AudioPolicy::Days(7),
                "30d" => AudioPolicy::Days(30),
                "forever" => AudioPolicy::Forever,
                "none" => AudioPolicy::Never,
                _ => d.audio,
            },
            transcripts: match transcripts {
                "forever" => TranscriptPolicy::Forever,
                "1y" => TranscriptPolicy::Days(365),
                "90d" => TranscriptPolicy::Days(90),
                _ => d.transcripts,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_settings_values() {
        let p = Policy::from_settings("30d", "90d");
        assert_eq!(p.audio, AudioPolicy::Days(30));
        assert_eq!(p.transcripts, TranscriptPolicy::Days(90));
        assert_eq!(Policy::from_settings("none", "1y").audio, AudioPolicy::Never);
        assert_eq!(Policy::from_settings("forever", "forever").audio, AudioPolicy::Forever);
    }

    #[test]
    fn unknown_values_use_defaults() {
        assert_eq!(Policy::from_settings("???", "???"), Policy::default());
    }
}
