//! "Transcribe again" keeps a backup: the previous transcript, the edits to it, the summary (with its edits and ticks)
//! and the few meeting fields the new run changes wait in `retranscribe.bak/` until the new transcription and its
//! summary have succeeded. A failed run puts all of it back exactly and says why.

use super::io::{read_json_opt, remove_if_exists, write_json_atomic};
use super::model::{MeetingStatus, ProcessingError, TranscriptionInfo};
use super::paths::*;
use super::{Result, Storage};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const BACKUP_DIR: &str = "retranscribe.bak";
const FIELDS: &str = "fields.json";
/// Left in place by the new run (it is rewritten): copied. The others are moved, since the run doesn't write them.
const COPIED: [&str; 1] = [TRANSCRIPT];
const MOVED: [&str; 4] = [TRANSCRIPT_EDITS, SUMMARY, SUMMARY_EDITED, SUMMARY_DONE];

/// The `meeting.json` fields a new run changes.
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Fields {
    status: Option<MeetingStatus>,
    language: String,
    engine: Option<String>,
    people_count: u32,
    transcription: Option<TranscriptionInfo>,
    summary_error: Option<ProcessingError>,
    error: Option<ProcessingError>,
}

fn move_file(from: &Path, to: &Path) -> Result<()> {
    if from.exists() {
        // A file already there is a leftover of an earlier attempt: this one is the newer.
        remove_if_exists(to)?;
        fs::rename(from, to)?;
    }
    Ok(())
}

impl Storage {
    /// Whether a "Transcribe again" backup is waiting for its run to end.
    pub fn has_retranscribe_backup(&self, id: &str) -> bool {
        self.dir(id).is_ok_and(|d| d.join(BACKUP_DIR).join(FIELDS).exists())
    }

    /// Puts the previous transcript, edits and summary aside. A new run only starts on a Ready or Failed meeting, and
    /// an interrupted run resumes without coming here, so a backup found now is a stale leftover (after a run that
    /// worked there is none): it is replaced by what the meeting is now. The visible meeting then shows no summary
    /// and no edits while the new run works.
    pub(crate) fn backup_for_retranscribe(&self, id: &str) -> Result<()> {
        let dir = self.dir(id)?;
        let bak = dir.join(BACKUP_DIR);
        if bak.exists() {
            fs::remove_dir_all(&bak)?;
        }
        fs::create_dir_all(&bak)?;
        let file = self.read_meeting_file(id)?;
        for name in COPIED {
            if dir.join(name).exists() {
                fs::copy(dir.join(name), bak.join(name))?;
            }
        }
        for name in MOVED {
            move_file(&dir.join(name), &bak.join(name))?;
        }
        // Written last: its presence says the backup is complete.
        write_json_atomic(
            &bak.join(FIELDS),
            &Fields {
                status: Some(file.status),
                language: file.language,
                engine: file.engine,
                people_count: file.people_count,
                transcription: file.transcription,
                summary_error: file.summary_error,
                error: file.error,
            },
        )
    }

    /// The new run succeeded (transcript and, unless the meeting is transcript only, summary): the backup goes.
    pub fn finish_retranscribe(&self, id: &str) {
        if let Ok(dir) = self.dir(id) {
            let _ = fs::remove_dir_all(dir.join(BACKUP_DIR));
        }
    }

    /// The new run failed: puts the previous transcript, edits, summary and fields back exactly, and records `why`
    /// for the meeting page. False when there was no backup (a plain failure, not a "Transcribe again").
    pub fn restore_retranscribe(&self, id: &str, why: ProcessingError) -> Result<bool> {
        let dir = self.dir(id)?;
        let bak = dir.join(BACKUP_DIR);
        let Some(fields) = read_json_opt::<Fields>(&bak.join(FIELDS))? else { return Ok(false) };
        for name in COPIED {
            if bak.join(name).exists() {
                fs::copy(bak.join(name), dir.join(name))?;
            }
        }
        for name in MOVED {
            // What the failed run wrote goes; what was there before comes back.
            remove_if_exists(&dir.join(name))?;
            move_file(&bak.join(name), &dir.join(name))?;
        }
        self.update_meeting_file(id, |f| {
            f.status = fields.status.clone().filter(|s| *s != MeetingStatus::Transcribing && *s != MeetingStatus::Summarizing).unwrap_or(MeetingStatus::Ready);
            f.language = fields.language.clone();
            f.engine = fields.engine.clone();
            f.people_count = fields.people_count;
            f.transcription = fields.transcription.clone();
            f.summary_error = fields.summary_error.clone();
            f.error = fields.error.clone();
            f.progress = None;
            f.eta = None;
            f.retranscribe_model = None;
            f.retranscribe_error = Some(why);
        })?;
        let _ = fs::remove_dir_all(&bak);
        self.regenerate_markdown(id)?;
        Ok(true)
    }

    /// The user closed the "didn't work" notice.
    pub fn dismiss_retranscribe_error(&self, id: &str) -> Result<()> {
        self.update_meeting_file(id, |f| f.retranscribe_error = None)
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::tests::{at, line, storage, summary};
    use super::*;

    /// A ready meeting with audio, an edited transcript and summary, a ticked action item.
    fn setup() -> (tempfile::TempDir, Storage, String) {
        let (d, s) = storage();
        let id = s.create_meeting("zoom", "Zoom", at("2026-10-14T14:30:00-07:00")).unwrap().id;
        s.save_transcript(&id, &[line("l0", 0, "Okay, looks like everyone is here."), line("l1", 6000, "Sounds good.")]).unwrap();
        s.save_summary(&id, &summary("Original recap.")).unwrap();
        s.update_meeting_file(&id, |f| {
            f.status = MeetingStatus::Ready;
            f.language = "en".into();
            f.engine = Some("parakeet-v3".into());
        })
        .unwrap();
        fs::write(d.path().join("meetings").join(&id).join(ME_WAV), b"x").unwrap();
        s.update_transcript_line(&id, "l0", "Everyone's here.").unwrap();
        let mut edited = summary("Edited recap.");
        edited.action_items[0].done = true;
        s.update_summary(&id, &edited).unwrap();
        (d, s, id)
    }

    fn snapshot(d: &tempfile::TempDir, id: &str) -> Vec<(String, Option<Vec<u8>>)> {
        let dir = d.path().join("meetings").join(id);
        [TRANSCRIPT, TRANSCRIPT_EDITS, TRANSCRIPT_MD, SUMMARY, SUMMARY_EDITED, SUMMARY_DONE, SUMMARY_MD].iter().map(|n| (n.to_string(), fs::read(dir.join(n)).ok())).collect()
    }

    fn why() -> ProcessingError {
        ProcessingError { code: "failed".into(), message: "The engine broke.".into(), detail: None }
    }

    #[test]
    fn a_failed_run_puts_the_transcript_edits_and_summary_back_exactly() {
        let (d, s, id) = setup();
        let before = snapshot(&d, &id);
        let m = s.get_meeting(&id).unwrap();
        s.start_retranscribe(&id, "whisper-small").unwrap();
        // While it works: no summary, no edits, but the backup is there.
        assert!(s.has_retranscribe_backup(&id));
        assert!(s.get_meeting(&id).unwrap().summary.is_none());
        // The new run saves a different transcript and changes the fields.
        s.save_transcript(&id, &[line("n0", 0, "A different transcript.")]).unwrap();
        s.update_meeting_file(&id, |f| {
            f.language = "es".into();
            f.engine = Some("whisper-small".into());
        })
        .unwrap();

        assert!(s.restore_retranscribe(&id, why()).unwrap());
        assert_eq!(snapshot(&d, &id), before);
        let back = s.get_meeting(&id).unwrap();
        assert_eq!(back.item.status, MeetingStatus::Ready);
        assert_eq!((back.transcript, back.summary), (m.transcript, m.summary));
        assert_eq!(back.language, "en");
        let f = s.read_meeting_file(&id).unwrap();
        assert_eq!((f.engine.as_deref(), f.retranscribe_model), (Some("parakeet-v3"), None));
        assert_eq!(f.retranscribe_error.unwrap().message, "The engine broke.");
        assert!(!s.has_retranscribe_backup(&id));
        // Dismissing the notice clears it.
        s.dismiss_retranscribe_error(&id).unwrap();
        assert!(s.read_meeting_file(&id).unwrap().retranscribe_error.is_none());
    }

    #[test]
    fn a_finished_run_drops_the_backup_and_the_old_summary_stays_gone() {
        let (d, s, id) = setup();
        s.start_retranscribe(&id, "whisper-small").unwrap();
        s.save_transcript(&id, &[line("n0", 0, "New.")]).unwrap();
        s.finish_retranscribe(&id);
        assert!(!s.has_retranscribe_backup(&id));
        assert!(!d.path().join("meetings").join(&id).join("retranscribe.bak").exists());
        assert!(s.get_meeting(&id).unwrap().summary.is_none());
        // Nothing to restore any more: a plain failure.
        assert!(!s.restore_retranscribe(&id, why()).unwrap());
    }

    #[test]
    fn a_stale_backup_is_replaced_by_the_current_state_when_a_new_run_starts() {
        let (d, s, id) = setup();
        s.start_retranscribe(&id, "whisper-small").unwrap();
        // The run ended without clearing its backup (the new transcript is what the meeting has now).
        s.save_transcript(&id, &[line("n0", 0, "The transcript that is good.")]).unwrap();
        s.update_meeting_file(&id, |f| f.status = MeetingStatus::Ready).unwrap();
        fs::write(d.path().join("meetings").join(&id).join(ME_WAV), b"x").unwrap();
        s.start_retranscribe(&id, "whisper-small").unwrap();
        // A failure of this run restores the good transcript, not the one from before the stale backup.
        assert!(s.restore_retranscribe(&id, why()).unwrap());
        assert_eq!(s.get_meeting(&id).unwrap().transcript[0].text, "The transcript that is good.");
        assert!(!s.has_retranscribe_backup(&id));
    }

    #[test]
    fn a_failed_one_that_was_not_a_transcribe_again_has_nothing_to_restore() {
        let (_d, s, id) = setup();
        assert!(!s.restore_retranscribe(&id, why()).unwrap());
        assert_eq!(s.get_meeting(&id).unwrap().item.status, MeetingStatus::Ready);
    }
}
