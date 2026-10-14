//! Simple edits to a meeting's metadata, and deletion.

use super::model::MeetingStatus;
use super::paths::{is_valid_id, ME_WAV, PLAYBACK_WAV, THEM_WAV};
use super::{Result, Storage, StorageError};
use std::fs;

fn valid_speaker(key: &str) -> bool {
    key == "me" || key.strip_prefix("them:").is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

impl Storage {
    /// A blank title is ignored. Setting a title clears the placeholder flag.
    pub fn rename_meeting(&self, id: &str, title: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Ok(());
        }
        self.update_meeting_file(id, |f| {
            f.title = title.to_string();
            f.title_is_placeholder = false;
        })?;
        self.regenerate_markdown(id)
    }

    pub fn set_pinned(&self, id: &str, pinned: bool) -> Result<()> {
        self.update_meeting_file(id, |f| f.pinned = pinned)
    }

    pub fn set_tags(&self, id: &str, tags: &[String]) -> Result<()> {
        let mut clean: Vec<String> = vec![];
        for t in tags {
            let t = t.trim().trim_start_matches('#').to_string();
            if !t.is_empty() && !clean.contains(&t) {
                clean.push(t);
            }
        }
        self.update_meeting_file(id, |f| f.tags = clean)
    }

    pub fn move_to_folder(&self, id: &str, folder_id: Option<&str>) -> Result<()> {
        self.update_meeting_file(id, |f| f.folder_id = folder_id.map(str::to_string))
    }

    pub fn rename_speaker(&self, id: &str, speaker: &str, name: &str) -> Result<()> {
        if !valid_speaker(speaker) {
            return Err(StorageError::InvalidId(speaker.to_string()));
        }
        let name = name.trim();
        if name.is_empty() {
            return Ok(());
        }
        self.update_meeting_file(id, |f| {
            f.speakers.insert(speaker.to_string(), name.to_string());
        })?;
        self.regenerate_markdown(id)
    }

    pub fn set_status(&self, id: &str, status: MeetingStatus) -> Result<()> {
        self.update_meeting_file(id, |f| f.status = status)
    }

    pub fn delete_meeting(&self, id: &str) -> Result<()> {
        if !is_valid_id(id) {
            return Err(StorageError::InvalidId(id.to_string()));
        }
        let dir = self.dir(id)?;
        fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// Deletes a meeting's audio now (both tracks and the playback mix). Transcript and summary stay.
    /// Refused while the audio is still needed (recording, transcribing, waiting for recovery).
    pub fn delete_audio(&self, id: &str) -> Result<()> {
        let file = self.read_meeting_file(id)?;
        if matches!(file.status, MeetingStatus::Recording | MeetingStatus::Transcribing | MeetingStatus::NeedsRecovery) {
            return Err(StorageError::Refused("The audio is still needed for the transcript.".into()));
        }
        let dir = self.dir(id)?;
        for name in [ME_WAV, THEM_WAV, PLAYBACK_WAV] {
            match fs::remove_file(dir.join(name)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        self.update_meeting_file(id, |f| f.audio_deleted = true)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::tests::{at, line, storage};
    use super::super::paths::{META, MEETINGS_DIR};
    use super::*;

    fn make(s: &Storage) -> String {
        s.create_meeting("zoom", "Zoom", at("2026-10-14T14:30:00-07:00")).unwrap().id
    }

    #[test]
    fn rename_clears_the_placeholder_flag_and_ignores_blank() {
        let (_d, s) = storage();
        let id = make(&s);
        s.rename_meeting(&id, "  Q4 roadmap sync  ").unwrap();
        let f = s.read_meeting_file(&id).unwrap();
        assert_eq!(f.title, "Q4 roadmap sync");
        assert!(!f.title_is_placeholder);
        s.rename_meeting(&id, "   ").unwrap();
        assert_eq!(s.read_meeting_file(&id).unwrap().title, "Q4 roadmap sync");
    }

    #[test]
    fn pin_and_unpin() {
        let (_d, s) = storage();
        let id = make(&s);
        s.set_pinned(&id, true).unwrap();
        assert!(s.get_meeting(&id).unwrap().item.pinned);
        s.set_pinned(&id, false).unwrap();
        assert!(!s.get_meeting(&id).unwrap().item.pinned);
    }

    #[test]
    fn tags_are_cleaned_and_deduplicated() {
        let (_d, s) = storage();
        let id = make(&s);
        s.set_tags(&id, &["#roadmap".into(), " zoom ".into(), "roadmap".into(), "".into()]).unwrap();
        assert_eq!(s.get_meeting(&id).unwrap().item.tags, vec!["roadmap", "zoom"]);
    }

    #[test]
    fn move_to_folder_and_back_out() {
        let (_d, s) = storage();
        let id = make(&s);
        s.move_to_folder(&id, Some("work")).unwrap();
        assert_eq!(s.get_meeting(&id).unwrap().item.folder_id.as_deref(), Some("work"));
        s.move_to_folder(&id, None).unwrap();
        assert!(s.get_meeting(&id).unwrap().item.folder_id.is_none());
    }

    #[test]
    fn speakers_can_be_renamed_but_keys_are_checked() {
        let (_d, s) = storage();
        let id = make(&s);
        s.rename_speaker(&id, "them:1", "Dana Reyes").unwrap();
        assert_eq!(s.get_meeting(&id).unwrap().speakers["them:1"], "Dana Reyes");
        assert!(s.rename_speaker(&id, "../etc", "x").is_err());
        assert!(s.rename_speaker(&id, "them:", "x").is_err());
    }

    #[test]
    fn unknown_fields_survive_an_update() {
        let (dir, s) = storage();
        let id = make(&s);
        let path = dir.path().join(MEETINGS_DIR).join(&id).join(META);
        let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        v["fromTheFuture"] = serde_json::json!({"keep": true});
        std::fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();
        s.set_pinned(&id, true).unwrap();
        let after: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(after["fromTheFuture"]["keep"], true);
        assert_eq!(after["pinned"], true);
    }

    #[test]
    fn delete_removes_the_whole_folder() {
        let (dir, s) = storage();
        let id = make(&s);
        std::fs::write(dir.path().join(MEETINGS_DIR).join(&id).join("me.wav"), b"x").unwrap();
        s.delete_meeting(&id).unwrap();
        assert!(!dir.path().join(MEETINGS_DIR).join(&id).exists());
        assert!(matches!(s.get_meeting(&id), Err(StorageError::NotFound(_))));
        assert!(matches!(s.delete_meeting(&id), Err(StorageError::NotFound(_))));
        assert!(s.delete_meeting("../x").is_err());
    }

    #[test]
    fn set_status_changes_only_the_status() {
        let (_d, s) = storage();
        let id = make(&s);
        s.set_status(&id, MeetingStatus::Ready).unwrap();
        assert_eq!(s.get_meeting(&id).unwrap().item.status, MeetingStatus::Ready);
    }

    #[test]
    fn delete_audio_keeps_the_words() {
        let (_d, s) = storage();
        let f = s.create_meeting("zoom", "Zoom", at("2026-10-14T14:30:00-07:00")).unwrap();
        let dir = s.dir(&f.id).unwrap();
        fs::write(dir.join(ME_WAV), b"x").unwrap();
        fs::write(dir.join(PLAYBACK_WAV), b"x").unwrap();
        s.save_transcript(&f.id, &[line("l0", 0, "Hello.")]).unwrap();
        // Still recording: refused.
        assert!(matches!(s.delete_audio(&f.id), Err(StorageError::Refused(_))));
        s.update_meeting_file(&f.id, |m| m.status = MeetingStatus::Ready).unwrap();
        s.delete_audio(&f.id).unwrap();
        assert!(!dir.join(ME_WAV).exists() && !dir.join(PLAYBACK_WAV).exists());
        let m = s.get_meeting(&f.id).unwrap();
        assert!(!m.has_audio);
        assert_eq!(m.transcript.len(), 1);
    }
}
