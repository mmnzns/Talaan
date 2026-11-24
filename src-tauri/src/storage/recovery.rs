//! Crash recovery: a meeting still marked `recording` at launch was interrupted.

use super::model::{MeetingFile, MeetingStatus, ProcessingError};
use super::paths::{is_valid_id, meetings_dir};
use super::{Result, Storage};
use std::fs;

/// Which worker an attempt belongs to: each keeps its own count on the meeting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Attempt {
    Transcribe,
    Summarize,
}

impl Attempt {
    fn counter(self, f: &mut MeetingFile) -> &mut u32 {
        match self {
            Attempt::Transcribe => &mut f.attempts,
            Attempt::Summarize => &mut f.summary_attempts,
        }
    }
}

impl Storage {
    /// Flips every `recording` meeting to `needs_recovery` and returns their ids.
    /// Nothing can legitimately be recording when the app starts. Unreadable meetings are skipped, and so is one
    /// that can't be saved (logged by id): only an unreadable meetings folder is an error.
    pub fn scan_for_interrupted(&self) -> Result<Vec<String>> {
        let mut flipped = vec![];
        for entry in fs::read_dir(meetings_dir(&self.root))? {
            let Ok(entry) = entry else { continue };
            let id = entry.file_name().to_string_lossy().to_string();
            if !entry.file_type().is_ok_and(|t| t.is_dir()) || !is_valid_id(&id) {
                continue;
            }
            let Ok(file) = self.read_meeting_file(&id) else { continue };
            if file.status == MeetingStatus::Recording {
                match self.set_status(&id, MeetingStatus::NeedsRecovery) {
                    Ok(()) => flipped.push(id),
                    Err(e) => log::warn!("recovery scan: couldn't mark meeting {id}: {}", e.for_log()),
                }
            }
        }
        flipped.sort();
        Ok(flipped)
    }

    /// A worker is starting on this meeting. A crash before `end_attempt` leaves the mark behind; quitting Talaan
    /// clears it (`clear_attempts`), so only a real crash counts at the next launch.
    pub fn begin_attempt(&self, id: &str, worker: Attempt) {
        let _ = self.update_meeting_file(id, |f| {
            let n = worker.counter(f);
            *n = n.saturating_add(1);
        });
    }

    /// The worker is done with this meeting, however it went (finished, failed, left for the next launch because
    /// Talaan is quitting, or a panic that was caught): the next start is a fresh one. Only that worker's count.
    pub fn end_attempt(&self, id: &str, worker: Attempt) {
        if self.read_meeting_file(id).is_ok_and(|mut f| *worker.counter(&mut f) > 0) {
            let _ = self.update_meeting_file(id, |f| *worker.counter(f) = 0);
        }
    }

    /// Talaan is quitting: a meeting a worker was in the middle of is not a crash, it just resumes at the next launch.
    /// Clears both workers' marks on every meeting (a worker that gives up on stopping, or one that is still running
    /// while the process exits, would otherwise leave them behind).
    pub fn clear_attempts(&self) {
        for status in [MeetingStatus::Transcribing, MeetingStatus::Summarizing] {
            for id in self.ids_with_status(status).unwrap_or_default() {
                if self.read_meeting_file(&id).is_ok_and(|f| f.attempts > 0 || f.summary_attempts > 0) {
                    let _ = self.update_meeting_file(&id, |f| {
                        f.attempts = 0;
                        f.summary_attempts = 0;
                    });
                }
            }
        }
    }

    /// At launch: a meeting still `transcribing` or `summarizing` is resumed (the workers queue it), unless a worker
    /// already started on it `max_attempts` times without ending cleanly, so it must be crashing the app. That one
    /// is put in a state the user can act on: a failed transcription shows Try again, a summary that never came
    /// shows Generate summary, and a "Transcribe again" gets its previous transcript and summary back. Returns the
    /// ids that were stopped.
    pub fn stop_crash_loops(&self, max_attempts: u32) -> Vec<String> {
        let mut stopped = vec![];
        for status in [MeetingStatus::Transcribing, MeetingStatus::Summarizing] {
            for id in self.ids_with_status(status).unwrap_or_default() {
                let Ok(file) = self.read_meeting_file(&id) else { continue };
                let transcribing = status == MeetingStatus::Transcribing;
                let attempts = if transcribing { file.attempts } else { file.summary_attempts };
                if attempts < max_attempts {
                    continue;
                }
                let code = if transcribing { "interrupted" } else { "failed" };
                let message = crate::i18n::tr(if transcribing { "error.transcribe_interrupted" } else { "error.summary_interrupted" });
                let why = ProcessingError { code: code.into(), message, detail: None };
                let restored = self.restore_retranscribe(&id, why.clone()).unwrap_or(false);
                let saved = self.update_meeting_file(&id, |f| {
                    f.attempts = 0;
                    f.summary_attempts = 0;
                    f.progress = None;
                    f.eta = None;
                    if !restored {
                        if transcribing {
                            f.status = MeetingStatus::Failed;
                            f.error = Some(why);
                        } else {
                            f.status = MeetingStatus::Ready;
                            f.summary_error = Some(why);
                        }
                    }
                });
                match saved {
                    Ok(()) => {
                        log::warn!("meeting {id} was stuck {} after {} starts: stopped it", if transcribing { "transcribing" } else { "summarizing" }, attempts);
                        stopped.push(id);
                    }
                    Err(e) => log::warn!("couldn't stop the stuck meeting {id}: {}", e.for_log()),
                }
            }
        }
        stopped
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::tests::{at, storage};
    use super::*;

    #[test]
    fn a_meeting_left_recording_becomes_needs_recovery() {
        let (_d, s) = storage();
        let stuck = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap();
        let done = s.create_meeting("zoom", "Zoom", at("2026-10-13T09:00:00-07:00")).unwrap();
        s.set_status(&done.id, MeetingStatus::Ready).unwrap();

        assert_eq!(s.scan_for_interrupted().unwrap(), vec![stuck.id.clone()]);
        assert_eq!(s.get_meeting(&stuck.id).unwrap().item.status, MeetingStatus::NeedsRecovery);
        assert_eq!(s.get_meeting(&done.id).unwrap().item.status, MeetingStatus::Ready);
    }

    #[test]
    fn scanning_twice_changes_nothing_the_second_time() {
        let (_d, s) = storage();
        s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap();
        assert_eq!(s.scan_for_interrupted().unwrap().len(), 1);
        assert!(s.scan_for_interrupted().unwrap().is_empty());
    }

    #[test]
    fn an_unreadable_meeting_does_not_stop_the_scan() {
        let (d, s) = storage();
        let bad = d.path().join("meetings").join("2026-10-13_0900_bad1");
        fs::create_dir(&bad).unwrap();
        fs::write(bad.join("meeting.json"), "garbage").unwrap();
        let stuck = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap();
        assert_eq!(s.scan_for_interrupted().unwrap(), vec![stuck.id]);
        assert_eq!(fs::read_to_string(bad.join("meeting.json")).unwrap(), "garbage");
    }

    fn stuck(s: &Storage, status: MeetingStatus, attempts: u32) -> String {
        let id = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap().id;
        s.update_meeting_file(&id, |f| {
            f.status = status.clone();
            f.attempts = if status == MeetingStatus::Transcribing { attempts } else { 0 };
            f.summary_attempts = if status == MeetingStatus::Summarizing { attempts } else { 0 };
            f.progress = Some(0.4);
        })
        .unwrap();
        id
    }

    #[test]
    fn a_meeting_that_crashed_its_worker_twice_is_stopped_with_a_way_forward() {
        let (_d, s) = storage();
        let once = stuck(&s, MeetingStatus::Transcribing, 1);
        let twice = stuck(&s, MeetingStatus::Transcribing, 2);
        let summary_twice = stuck(&s, MeetingStatus::Summarizing, 3);
        let summary_once = stuck(&s, MeetingStatus::Summarizing, 1);
        let mut stopped = s.stop_crash_loops(2);
        stopped.sort();
        let mut expected = vec![twice.clone(), summary_twice.clone()];
        expected.sort();
        assert_eq!(stopped, expected);

        // Once is resumed as before.
        for id in [&once, &summary_once] {
            assert_ne!(s.read_meeting_file(id).unwrap().status, MeetingStatus::Failed);
        }
        assert_eq!(s.read_meeting_file(&once).unwrap().status, MeetingStatus::Transcribing);
        assert_eq!(s.read_meeting_file(&summary_once).unwrap().status, MeetingStatus::Summarizing);
        // A transcription: failed, Try again.
        let f = s.read_meeting_file(&twice).unwrap();
        assert_eq!((f.status, f.attempts, f.progress), (MeetingStatus::Failed, 0, None));
        assert_eq!(f.error.unwrap().code, "interrupted");
        // A summary: ready, transcript only, Generate summary.
        let f = s.read_meeting_file(&summary_twice).unwrap();
        assert_eq!((f.status, f.attempts), (MeetingStatus::Ready, 0));
        assert_eq!(f.summary_error.unwrap().code, "failed");
        // A second scan finds nothing more.
        assert!(s.stop_crash_loops(2).is_empty());
    }

    #[test]
    fn an_attempt_that_ends_cleanly_leaves_no_mark() {
        let (_d, s) = storage();
        let id = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap().id;
        s.begin_attempt(&id, Attempt::Transcribe);
        s.begin_attempt(&id, Attempt::Transcribe);
        assert_eq!(s.read_meeting_file(&id).unwrap().attempts, 2);
        s.end_attempt(&id, Attempt::Transcribe);
        assert_eq!(s.read_meeting_file(&id).unwrap().attempts, 0);
    }

    #[test]
    fn each_worker_counts_and_clears_only_its_own_attempts() {
        let (_d, s) = storage();
        let id = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap().id;
        s.begin_attempt(&id, Attempt::Summarize);
        s.begin_attempt(&id, Attempt::Summarize);
        // The transcriber finishing (or starting) does not touch the summarizer's count.
        s.begin_attempt(&id, Attempt::Transcribe);
        s.end_attempt(&id, Attempt::Transcribe);
        let f = s.read_meeting_file(&id).unwrap();
        assert_eq!((f.attempts, f.summary_attempts), (0, 2));
        s.end_attempt(&id, Attempt::Summarize);
        assert_eq!(s.read_meeting_file(&id).unwrap().summary_attempts, 0);
    }

    #[test]
    fn quitting_in_the_middle_of_a_job_twice_is_not_a_crash() {
        let (_d, s) = storage();
        let t = stuck(&s, MeetingStatus::Transcribing, 0);
        let m = stuck(&s, MeetingStatus::Summarizing, 0);
        // Launch 1 and launch 2: a worker starts, Talaan quits (workers may not even have stopped), marks cleared.
        for _ in 0..2 {
            s.begin_attempt(&t, Attempt::Transcribe);
            s.begin_attempt(&m, Attempt::Summarize);
            s.clear_attempts();
            assert!(s.stop_crash_loops(2).is_empty());
        }
        assert_eq!(s.read_meeting_file(&t).unwrap().status, MeetingStatus::Transcribing);
        assert_eq!(s.read_meeting_file(&m).unwrap().status, MeetingStatus::Summarizing);
        // A summary that crashed twice is stopped without touching a transcription that crashed once.
        s.begin_attempt(&m, Attempt::Summarize);
        s.begin_attempt(&m, Attempt::Summarize);
        s.begin_attempt(&t, Attempt::Transcribe);
        assert_eq!(s.stop_crash_loops(2), vec![m.clone()]);
        assert_eq!(s.read_meeting_file(&t).unwrap().status, MeetingStatus::Transcribing);
    }

    #[test]
    fn a_transcribe_again_that_keeps_crashing_gets_its_previous_transcript_back() {
        use super::super::store::tests::{line, summary};
        let (d, s) = storage();
        let id = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap().id;
        s.save_transcript(&id, &[line("l0", 0, "Old transcript.")]).unwrap();
        s.save_summary(&id, &summary("Old recap.")).unwrap();
        s.update_meeting_file(&id, |f| f.status = MeetingStatus::Ready).unwrap();
        fs::write(d.path().join("meetings").join(&id).join("me.wav"), b"x").unwrap();
        s.start_retranscribe(&id, "whisper-small").unwrap();
        s.update_meeting_file(&id, |f| f.attempts = 2).unwrap();
        assert_eq!(s.stop_crash_loops(2), vec![id.clone()]);
        let m = s.get_meeting(&id).unwrap();
        assert_eq!(m.item.status, MeetingStatus::Ready);
        assert_eq!((m.transcript[0].text.as_str(), m.summary.unwrap().recap.as_str()), ("Old transcript.", "Old recap."));
        assert_eq!(m.retranscribe_error.unwrap().code, "interrupted");
    }

    #[test]
    fn an_empty_library_is_fine() {
        let (_d, s) = storage();
        assert!(s.scan_for_interrupted().unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_meeting_that_cant_be_saved_does_not_stop_the_scan() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, s) = storage();
        let stuck = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap();
        let other = s.create_meeting("zoom", "Zoom", at("2026-10-15T09:00:00-07:00")).unwrap();
        let locked = s.dir(&stuck.id).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
        // Running as root ignores permissions: nothing to force a failure with.
        let forced = fs::File::create(locked.join("probe")).is_err();
        let result = s.scan_for_interrupted();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        if !forced {
            return;
        }
        assert_eq!(result.unwrap(), vec![other.id.clone()]);
        assert_eq!(s.get_meeting(&other.id).unwrap().item.status, MeetingStatus::NeedsRecovery);
        assert_eq!(s.get_meeting(&stuck.id).unwrap().item.status, MeetingStatus::Recording);
    }
}
