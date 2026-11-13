//! How long audio and transcripts are kept, and the pass that enforces it.
//!
//! Rules: pinned meetings are never touched. Meetings that still need their audio (recording, needing
//! recovery, transcribing, summarizing) are never touched. "Don't keep" also spares failed meetings so
//! they can be retried, and so does the day-based limit, which also spares meetings with no transcript yet. A
//! meeting's age is from the later of when it was recorded and when it was imported.

use super::io::write_json_atomic;
use super::model::MeetingStatus;
use super::paths::{is_valid_id, meetings_dir, ME_WAV, META, PLAYBACK_WAV, THEM_WAV, TRANSCRIPT};
use super::{Result, Storage};
use chrono::{DateTime, Duration, Utc};
use std::fs;

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

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RetentionReport {
    pub audio_deleted: Vec<String>,
    pub meetings_deleted: Vec<String>,
}

impl Storage {
    pub fn apply_retention(&self, now: DateTime<Utc>) -> Result<RetentionReport> {
        let mut report = RetentionReport::default();
        for entry in fs::read_dir(meetings_dir(&self.root))? {
            let entry = entry?;
            let id = entry.file_name().to_string_lossy().to_string();
            if !entry.file_type()?.is_dir() || !is_valid_id(&id) {
                continue;
            }
            let Ok(mut file) = self.read_meeting_file(&id) else { continue };
            let busy = matches!(file.status, MeetingStatus::Recording | MeetingStatus::NeedsRecovery | MeetingStatus::Transcribing | MeetingStatus::Summarizing);
            if file.pinned || busy {
                continue;
            }
            let Ok(started) = DateTime::parse_from_rfc3339(&file.started_at) else { continue };
            // An import is as old as the day it came in, not the day the audio was recorded.
            let came_in = file.imported_at.as_deref().and_then(|t| DateTime::parse_from_rfc3339(t).ok()).map_or(started, |i| started.max(i));
            let age_ends = |days: u32| came_in.with_timezone(&Utc) + Duration::days(days as i64) <= now;

            if let TranscriptPolicy::Days(d) = self.policy.transcripts {
                if age_ends(d) {
                    fs::remove_dir_all(entry.path())?;
                    report.meetings_deleted.push(id);
                    continue;
                }
            }

            let expired = match self.policy.audio {
                AudioPolicy::Forever => false,
                // Audio that has no transcript yet (a failed first try waiting for a model) is the only way to make one.
                AudioPolicy::Days(d) => age_ends(d) && file.status != MeetingStatus::Failed && entry.path().join(TRANSCRIPT).exists(),
                AudioPolicy::Never => file.status != MeetingStatus::Failed,
            };
            if expired && !file.audio_deleted {
                let mut removed = false;
                for name in [ME_WAV, THEM_WAV, PLAYBACK_WAV] {
                    match fs::remove_file(entry.path().join(name)) {
                        Ok(()) => removed = true,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => return Err(e.into()),
                    }
                }
                file.audio_deleted = true;
                write_json_atomic(&entry.path().join(META), &file)?;
                if removed {
                    report.audio_deleted.push(id);
                }
            }
        }
        report.audio_deleted.sort();
        report.meetings_deleted.sort();
        Ok(report)
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

#[cfg(test)]
mod pass_tests {
    use super::super::store::tests::storage;
    use super::*;
    use std::path::PathBuf;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-22T12:00:00-07:00").unwrap().with_timezone(&Utc)
    }

    /// A ready meeting that started `days_old` days before `now()`, with both audio tracks.
    fn meeting(s: &Storage, days_old: i64) -> (String, PathBuf) {
        let started = now() - Duration::days(days_old);
        let f = s.create_meeting("zoom", "Zoom", started.fixed_offset()).unwrap();
        s.set_status(&f.id, MeetingStatus::Ready).unwrap();
        let dir = meetings_dir(s.root()).join(&f.id);
        fs::write(dir.join(ME_WAV), b"me").unwrap();
        fs::write(dir.join(THEM_WAV), b"them").unwrap();
        (f.id, dir)
    }

    fn policy(audio: &str, transcripts: &str) -> Policy {
        Policy::from_settings(audio, transcripts)
    }

    #[test]
    fn expired_audio_is_removed_but_the_transcript_stays() {
        let (_d, mut s) = storage();
        s.set_policy(policy("7d", "forever"));
        let (id, dir) = meeting(&s, 8);
        s.save_transcript(&id, &[]).unwrap();
        let r = s.apply_retention(now()).unwrap();
        assert_eq!(r.audio_deleted, vec![id.clone()]);
        assert!(!dir.join(ME_WAV).exists() && !dir.join(THEM_WAV).exists());
        assert!(dir.join("transcript.json").exists());
        let m = s.get_meeting(&id).unwrap();
        assert!(!m.has_audio);
        assert!(m.audio_kept_until.is_none());
    }

    #[test]
    fn recent_audio_is_kept() {
        let (_d, mut s) = storage();
        s.set_policy(policy("7d", "forever"));
        let (id, dir) = meeting(&s, 3);
        assert_eq!(s.apply_retention(now()).unwrap(), RetentionReport::default());
        assert!(dir.join(ME_WAV).exists());
        assert!(s.get_meeting(&id).unwrap().has_audio);
    }

    #[test]
    fn audio_expires_exactly_at_the_boundary() {
        let (_d, mut s) = storage();
        s.set_policy(policy("7d", "forever"));
        let (id, dir) = meeting(&s, 7);
        s.save_transcript(&id, &[]).unwrap();
        s.apply_retention(now()).unwrap();
        assert!(!dir.join(ME_WAV).exists());
    }

    #[test]
    fn pinned_meetings_keep_their_audio_and_transcript() {
        let (_d, mut s) = storage();
        s.set_policy(policy("1d", "90d"));
        let (id, dir) = meeting(&s, 200);
        s.set_pinned(&id, true).unwrap();
        assert_eq!(s.apply_retention(now()).unwrap(), RetentionReport::default());
        assert!(dir.join(ME_WAV).exists());
    }

    #[test]
    fn meetings_that_still_need_their_audio_are_untouched() {
        let (_d, mut s) = storage();
        s.set_policy(policy("none", "90d"));
        for status in [MeetingStatus::Recording, MeetingStatus::NeedsRecovery, MeetingStatus::Transcribing, MeetingStatus::Summarizing] {
            let (id, dir) = meeting(&s, 400);
            s.set_status(&id, status).unwrap();
            s.apply_retention(now()).unwrap();
            assert!(dir.join(ME_WAV).exists(), "{status:?}");
        }
    }

    #[test]
    fn forever_never_deletes_audio() {
        let (_d, mut s) = storage();
        s.set_policy(policy("forever", "forever"));
        let (_id, dir) = meeting(&s, 1000);
        s.apply_retention(now()).unwrap();
        assert!(dir.join(ME_WAV).exists());
    }

    #[test]
    fn dont_keep_removes_audio_of_every_unpinned_ready_meeting_but_spares_failed_ones() {
        let (_d, mut s) = storage();
        s.set_policy(policy("none", "forever"));
        let (ready, rdir) = meeting(&s, 0);
        let (failed, fdir) = meeting(&s, 0);
        s.set_status(&failed, MeetingStatus::Failed).unwrap();
        let (pinned, pdir) = meeting(&s, 0);
        s.set_pinned(&pinned, true).unwrap();
        let r = s.apply_retention(now()).unwrap();
        assert_eq!(r.audio_deleted, vec![ready]);
        assert!(!rdir.join(ME_WAV).exists());
        assert!(fdir.join(ME_WAV).exists());
        assert!(pdir.join(ME_WAV).exists());
    }

    #[test]
    fn old_unpinned_transcripts_are_removed_with_their_meeting() {
        let (_d, mut s) = storage();
        s.set_policy(policy("forever", "90d"));
        let (old, old_dir) = meeting(&s, 120);
        let (recent, recent_dir) = meeting(&s, 10);
        let (pinned, pinned_dir) = meeting(&s, 120);
        s.set_pinned(&pinned, true).unwrap();
        let r = s.apply_retention(now()).unwrap();
        assert_eq!(r.meetings_deleted, vec![old]);
        assert!(!old_dir.exists());
        assert!(recent_dir.exists() && pinned_dir.exists());
        assert!(s.get_meeting(&recent).is_ok());
    }

    #[test]
    fn a_second_pass_finds_nothing_to_do() {
        let (_d, mut s) = storage();
        s.set_policy(policy("7d", "forever"));
        let (id, _dir) = meeting(&s, 30);
        s.save_transcript(&id, &[]).unwrap();
        assert_eq!(s.apply_retention(now()).unwrap().audio_deleted.len(), 1);
        assert_eq!(s.apply_retention(now()).unwrap(), RetentionReport::default());
    }

    #[test]
    fn unreadable_meetings_are_skipped() {
        let (d, mut s) = storage();
        s.set_policy(policy("1d", "90d"));
        let bad = d.path().join("meetings").join("2020-01-01_0900_bad1");
        fs::create_dir(&bad).unwrap();
        fs::write(bad.join("meeting.json"), "garbage").unwrap();
        assert_eq!(s.apply_retention(now()).unwrap(), RetentionReport::default());
        assert!(bad.exists());
    }

    /// An imported memo: it started long ago, but it came into Talaan `imported_days_ago` days before `now()`.
    fn imported(s: &Storage, started_days_old: i64, imported_days_ago: i64) -> (String, PathBuf) {
        let (id, dir) = meeting(s, started_days_old);
        let at = (now() - Duration::days(imported_days_ago)).fixed_offset().to_rfc3339();
        s.update_meeting_file(&id, |m| m.imported_at = Some(at)).unwrap();
        s.save_transcript(&id, &[]).unwrap();
        (id, dir)
    }

    #[test]
    fn a_recent_import_of_an_old_recording_survives_both_policies() {
        let (_d, mut s) = storage();
        s.set_policy(policy("7d", "90d"));
        let (id, dir) = imported(&s, 180, 0);
        assert_eq!(s.apply_retention(now()).unwrap(), RetentionReport::default());
        assert!(dir.join(ME_WAV).exists() && dir.join("transcript.json").exists());
        assert!(s.get_meeting(&id).is_ok());
    }

    #[test]
    fn an_import_is_cleaned_up_once_its_import_date_is_old_enough() {
        let (_d, mut s) = storage();
        s.set_policy(policy("7d", "90d"));
        let (_id, dir) = imported(&s, 400, 8);
        let r = s.apply_retention(now()).unwrap();
        assert_eq!(r.audio_deleted.len(), 1);
        assert!(!dir.join(ME_WAV).exists() && dir.exists());
        let (old, old_dir) = imported(&s, 400, 100);
        assert_eq!(s.apply_retention(now()).unwrap().meetings_deleted, vec![old]);
        assert!(!old_dir.exists());
    }

    #[test]
    fn a_normal_old_meeting_is_still_cleaned_up() {
        let (_d, mut s) = storage();
        s.set_policy(policy("7d", "90d"));
        let (old, dir) = meeting(&s, 120);
        let r = s.apply_retention(now()).unwrap();
        assert_eq!(r.meetings_deleted, vec![old]);
        assert!(!dir.exists());
    }

    #[test]
    fn day_based_audio_keeps_failed_meetings_and_those_without_a_transcript() {
        let (_d, mut s) = storage();
        s.set_policy(policy("7d", "forever"));
        let (failed, fdir) = meeting(&s, 30);
        s.set_status(&failed, MeetingStatus::Failed).unwrap();
        let (_waiting, wdir) = meeting(&s, 30);
        let (ready, rdir) = meeting(&s, 30);
        s.save_transcript(&ready, &[]).unwrap();
        let r = s.apply_retention(now()).unwrap();
        assert_eq!(r.audio_deleted, vec![ready]);
        assert!(fdir.join(ME_WAV).exists() && wdir.join(ME_WAV).exists());
        assert!(!rdir.join(ME_WAV).exists());
        // Once the failed one is retried and has its words, the audio can go.
        s.set_status(&failed, MeetingStatus::Ready).unwrap();
        s.save_transcript(&failed, &[]).unwrap();
        assert_eq!(s.apply_retention(now()).unwrap().audio_deleted, vec![failed]);
    }
}
