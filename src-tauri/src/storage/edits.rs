//! Edits live beside the originals; originals are never rewritten. Revert deletes the edit files.

use super::io::{read_json_opt, remove_if_exists, write_json_atomic, write_text_atomic};
use super::markdown::{notes_md, summary_md, transcript_md};
use super::model::{MeetingStatus, Summary, TranscriptLine};
use super::paths::*;
use super::{Result, Storage, StorageError};
use std::collections::BTreeMap;

/// Everything except the action items' `done` flags and the computed `edited` flag.
fn text_only(s: &Summary) -> Summary {
    let mut c = s.clone();
    c.edited = false;
    for a in &mut c.action_items {
        a.done = false;
    }
    c
}

impl Storage {
    pub fn update_transcript_line(&self, id: &str, line_id: &str, text: &str) -> Result<()> {
        let dir = self.dir(id)?;
        let original: Vec<TranscriptLine> = read_json_opt(&dir.join(TRANSCRIPT))?.unwrap_or_default();
        let line = original.iter().find(|l| l.id == line_id).ok_or_else(|| StorageError::NotFound(format!("line {line_id}")))?;

        let mut edits: BTreeMap<String, String> = read_json_opt(&dir.join(TRANSCRIPT_EDITS))?.unwrap_or_default();
        let text = text.trim();
        if text.is_empty() || text == line.text {
            edits.remove(line_id);
        } else {
            edits.insert(line_id.to_string(), text.to_string());
        }
        if edits.is_empty() {
            remove_if_exists(&dir.join(TRANSCRIPT_EDITS))?;
        } else {
            write_json_atomic(&dir.join(TRANSCRIPT_EDITS), &edits)?;
        }
        self.regenerate_markdown(id)
    }

    /// Saves the user's version of the summary. Done flags are kept separately so ticking a box isn't an "edit".
    pub fn update_summary(&self, id: &str, summary: &Summary) -> Result<()> {
        let dir = self.dir(id)?;
        let original: Summary = read_json_opt(&dir.join(SUMMARY))?.ok_or_else(|| StorageError::NotFound("summary".into()))?;

        let done: BTreeMap<String, bool> = summary.action_items.iter().filter(|a| a.done).map(|a| (a.id.clone(), true)).collect();
        if done.is_empty() {
            remove_if_exists(&dir.join(SUMMARY_DONE))?;
        } else {
            write_json_atomic(&dir.join(SUMMARY_DONE), &done)?;
        }

        if text_only(summary) == text_only(&original) {
            remove_if_exists(&dir.join(SUMMARY_EDITED))?;
        } else {
            let mut stored = text_only(summary);
            for a in &mut stored.action_items {
                a.done = false;
            }
            write_json_atomic(&dir.join(SUMMARY_EDITED), &stored)?;
        }
        self.regenerate_markdown(id)
    }

    /// Drops every edit. The originals were never touched.
    pub fn revert_edits(&self, id: &str) -> Result<()> {
        let dir = self.dir(id)?;
        remove_if_exists(&dir.join(TRANSCRIPT_EDITS))?;
        remove_if_exists(&dir.join(SUMMARY_EDITED))?;
        self.regenerate_markdown(id)
    }

    /// "Transcribe again" with `model`: back into the transcriber's queue. Transcript edits are dropped (the new
    /// lines have new ids) and so is the summary with its edits and ticks, because it is written again from the
    /// new transcript. Speaker names, title, tags, folder and marks stay. The old transcript stays until the new
    /// one replaces it, so a failed run loses nothing but the summary.
    pub fn start_retranscribe(&self, id: &str, model: &str) -> Result<()> {
        let dir = self.dir(id)?;
        let file = self.read_meeting_file(id)?;
        if !matches!(file.status, MeetingStatus::Ready | MeetingStatus::Failed) {
            return Err(StorageError::Refused(crate::i18n::tr("error.wait_meeting")));
        }
        if !self.has_audio(&dir, &file) {
            return Err(StorageError::Refused(crate::i18n::tr("error.audio_gone")));
        }
        for name in [TRANSCRIPT_EDITS, SUMMARY, SUMMARY_EDITED, SUMMARY_DONE] {
            remove_if_exists(&dir.join(name))?;
        }
        self.update_meeting_file(id, |f| {
            f.status = MeetingStatus::Transcribing;
            f.progress = Some(0.0);
            f.error = None;
            f.summary_error = None;
            f.retranscribe_model = Some(model.to_string());
        })?;
        self.regenerate_markdown(id)
    }

    /// Rewrites `transcript.md` and `summary.md` from what the user currently sees.
    pub fn regenerate_markdown(&self, id: &str) -> Result<()> {
        let dir = self.dir(id)?;
        let m = self.get_meeting(id)?;
        if m.transcript.is_empty() {
            remove_if_exists(&dir.join(TRANSCRIPT_MD))?;
        } else {
            write_text_atomic(&dir.join(TRANSCRIPT_MD), &transcript_md(&m.item.title, &m.transcript, &m.speakers))?;
        }
        match &m.summary {
            Some(s) => write_text_atomic(&dir.join(SUMMARY_MD), &summary_md(&m.item.title, s, &m.marks))?,
            None => match notes_md(&m.item.title, &m.marks) {
                Some(md) => write_text_atomic(&dir.join(SUMMARY_MD), &md)?,
                None => remove_if_exists(&dir.join(SUMMARY_MD))?,
            },
        }
        Ok(())
    }

    pub(crate) fn read_done(&self, id: &str) -> Result<BTreeMap<String, bool>> {
        let dir = self.dir(id)?;
        Ok(read_json_opt(&dir.join(SUMMARY_DONE))?.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::tests::{at, line, storage, summary};
    use super::*;
    use std::fs;

    fn setup() -> (tempfile::TempDir, Storage, String) {
        let (d, s) = storage();
        let id = s.create_meeting("zoom", "Zoom", at("2026-10-14T14:30:00-07:00")).unwrap().id;
        s.save_transcript(&id, &[line("l0", 0, "Okay, looks like everyone is here."), line("l1", 6000, "Sounds good.")]).unwrap();
        s.save_summary(&id, &summary("Original recap.")).unwrap();
        (d, s, id)
    }

    fn path(d: &tempfile::TempDir, id: &str, name: &str) -> std::path::PathBuf {
        d.path().join("meetings").join(id).join(name)
    }

    #[test]
    fn transcribe_again_drops_edits_and_summary_but_keeps_names() {
        let (d, s, id) = setup();
        s.update_meeting_file(&id, |f| {
            f.status = MeetingStatus::Ready;
            f.title = "Dana 1:1".into();
            f.title_is_placeholder = false;
        })
        .unwrap();
        s.rename_speaker(&id, "them:0", "Dana").unwrap();
        s.update_transcript_line(&id, "l0", "Everyone's here.").unwrap();
        fs::write(path(&d, &id, SUMMARY_EDITED), serde_json::to_vec(&summary("Edited.")).unwrap()).unwrap();
        fs::write(path(&d, &id, SUMMARY_DONE), r#"{"a0":true}"#).unwrap();
        // No audio: refused, nothing touched.
        assert!(matches!(s.start_retranscribe(&id, "parakeet-v3"), Err(StorageError::Refused(_))));
        assert!(path(&d, &id, TRANSCRIPT_EDITS).exists());

        fs::write(path(&d, &id, ME_WAV), b"x").unwrap();
        s.start_retranscribe(&id, "parakeet-v3").unwrap();
        for gone in [TRANSCRIPT_EDITS, SUMMARY, SUMMARY_EDITED, SUMMARY_DONE, SUMMARY_MD] {
            assert!(!path(&d, &id, gone).exists(), "{gone} should be gone");
        }
        let f = s.read_meeting_file(&id).unwrap();
        assert_eq!((f.status, f.retranscribe_model.as_deref()), (MeetingStatus::Transcribing, Some("parakeet-v3")));
        assert_eq!((f.title.as_str(), f.speakers["them:0"].as_str()), ("Dana 1:1", "Dana"));
        // The old transcript stays until the new one replaces it.
        assert_eq!(s.get_meeting(&id).unwrap().transcript[0].text, "Okay, looks like everyone is here.");
        // Not while it is busy.
        assert!(s.start_retranscribe(&id, "whisper-turbo").is_err());
    }

    #[test]
    fn transcript_edit_keeps_the_original_byte_for_byte() {
        let (d, s, id) = setup();
        let before = fs::read(path(&d, &id, TRANSCRIPT)).unwrap();
        s.update_transcript_line(&id, "l1", "Sounds great.").unwrap();
        assert_eq!(fs::read(path(&d, &id, TRANSCRIPT)).unwrap(), before);
        let m = s.get_meeting(&id).unwrap();
        assert_eq!(m.transcript[1].text, "Sounds great.");
        assert!(m.transcript[1].edited);
        assert!(!m.transcript[0].edited);
        assert!(path(&d, &id, TRANSCRIPT_EDITS).exists());
    }

    #[test]
    fn editing_back_to_the_original_text_removes_the_edit() {
        let (d, s, id) = setup();
        s.update_transcript_line(&id, "l1", "Changed").unwrap();
        s.update_transcript_line(&id, "l1", "Sounds good.").unwrap();
        assert!(!path(&d, &id, TRANSCRIPT_EDITS).exists());
        assert!(!s.get_meeting(&id).unwrap().transcript[1].edited);
    }

    #[test]
    fn unknown_line_is_an_error() {
        let (_d, s, id) = setup();
        assert!(matches!(s.update_transcript_line(&id, "nope", "x"), Err(StorageError::NotFound(_))));
    }

    #[test]
    fn summary_edit_keeps_the_original_and_marks_edited() {
        let (d, s, id) = setup();
        let before = fs::read(path(&d, &id, SUMMARY)).unwrap();
        let mut edited = s.get_meeting(&id).unwrap().summary.unwrap();
        edited.recap = "My recap.".into();
        s.update_summary(&id, &edited).unwrap();
        assert_eq!(fs::read(path(&d, &id, SUMMARY)).unwrap(), before);
        let m = s.get_meeting(&id).unwrap().summary.unwrap();
        assert_eq!(m.recap, "My recap.");
        assert!(m.edited);
    }

    #[test]
    fn an_edited_summary_keeps_the_language_it_was_written_in() {
        let (_d, s, id) = setup();
        let mut first = s.get_meeting(&id).unwrap().summary.unwrap();
        first.language = Some("fil".into());
        s.save_summary(&id, &first).unwrap();
        let mut edited = s.get_meeting(&id).unwrap().summary.unwrap();
        assert_eq!(edited.language.as_deref(), Some("fil"));
        edited.recap = "My recap.".into();
        s.update_summary(&id, &edited).unwrap();
        let got = s.get_meeting(&id).unwrap().summary.unwrap();
        assert_eq!((got.edited, got.language.as_deref()), (true, Some("fil")));
    }

    #[test]
    fn ticking_an_action_item_is_not_an_edit_but_persists() {
        let (d, s, id) = setup();
        let mut sum = s.get_meeting(&id).unwrap().summary.unwrap();
        sum.action_items[0].done = true;
        s.update_summary(&id, &sum).unwrap();
        let back = s.get_meeting(&id).unwrap().summary.unwrap();
        assert!(back.action_items[0].done);
        assert!(!back.edited);
        assert!(!path(&d, &id, SUMMARY_EDITED).exists());
        sum.action_items[0].done = false;
        s.update_summary(&id, &sum).unwrap();
        assert!(!s.get_meeting(&id).unwrap().summary.unwrap().action_items[0].done);
    }

    #[test]
    fn retyping_the_original_summary_clears_the_edit() {
        let (d, s, id) = setup();
        let mut sum = s.get_meeting(&id).unwrap().summary.unwrap();
        sum.recap = "Different".into();
        s.update_summary(&id, &sum).unwrap();
        assert!(path(&d, &id, SUMMARY_EDITED).exists());
        sum.recap = "Original recap.".into();
        s.update_summary(&id, &sum).unwrap();
        assert!(!path(&d, &id, SUMMARY_EDITED).exists());
    }

    #[test]
    fn revert_removes_edit_files_and_restores_markdown() {
        let (d, s, id) = setup();
        s.update_transcript_line(&id, "l1", "Sounds great.").unwrap();
        let mut sum = s.get_meeting(&id).unwrap().summary.unwrap();
        sum.recap = "My recap.".into();
        s.update_summary(&id, &sum).unwrap();
        assert!(fs::read_to_string(path(&d, &id, TRANSCRIPT_MD)).unwrap().contains("Sounds great."));
        assert!(fs::read_to_string(path(&d, &id, SUMMARY_MD)).unwrap().contains("My recap."));

        s.revert_edits(&id).unwrap();
        assert!(!path(&d, &id, TRANSCRIPT_EDITS).exists());
        assert!(!path(&d, &id, SUMMARY_EDITED).exists());
        let m = s.get_meeting(&id).unwrap();
        assert_eq!(m.transcript[1].text, "Sounds good.");
        assert_eq!(m.summary.as_ref().unwrap().recap, "Original recap.");
        assert!(fs::read_to_string(path(&d, &id, TRANSCRIPT_MD)).unwrap().contains("Sounds good."));
        assert!(fs::read_to_string(path(&d, &id, SUMMARY_MD)).unwrap().contains("Original recap."));
    }

    #[test]
    fn markdown_files_exist_after_saving_originals_and_follow_renames() {
        let (d, s, id) = setup();
        s.rename_meeting(&id, "Q4 roadmap sync").unwrap();
        s.rename_speaker(&id, "me", "Miguel").unwrap();
        let t = fs::read_to_string(path(&d, &id, TRANSCRIPT_MD)).unwrap();
        assert!(t.starts_with("# Q4 roadmap sync"));
        assert!(t.contains("Miguel:"));
        assert!(fs::read_to_string(path(&d, &id, SUMMARY_MD)).unwrap().starts_with("# Q4 roadmap sync"));
    }
}
