//! Crash recovery: a meeting still marked `recording` at launch was interrupted.

use super::model::MeetingStatus;
use super::paths::{is_valid_id, meetings_dir};
use super::{Result, Storage};
use std::fs;

impl Storage {
    /// Flips every `recording` meeting to `needs_recovery` and returns their ids.
    /// Nothing can legitimately be recording when the app starts. Unreadable meetings are skipped.
    pub fn scan_for_interrupted(&self) -> Result<Vec<String>> {
        let mut flipped = vec![];
        for entry in fs::read_dir(meetings_dir(&self.root))? {
            let entry = entry?;
            let id = entry.file_name().to_string_lossy().to_string();
            if !entry.file_type()?.is_dir() || !is_valid_id(&id) {
                continue;
            }
            let Ok(file) = self.read_meeting_file(&id) else { continue };
            if file.status == MeetingStatus::Recording {
                self.set_status(&id, MeetingStatus::NeedsRecovery)?;
                flipped.push(id);
            }
        }
        flipped.sort();
        Ok(flipped)
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

    #[test]
    fn an_empty_library_is_fine() {
        let (_d, s) = storage();
        assert!(s.scan_for_interrupted().unwrap().is_empty());
    }
}
