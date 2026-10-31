//! Simple edits to a meeting's metadata, and deletion.

use super::model::{Mark, MeetingStatus};
use super::io::{read_json_opt, write_json_atomic};
use super::model::Summary;
use super::paths::{is_valid_id, ME_WAV, PLAYBACK_WAV, SUMMARY, SUMMARY_EDITED, THEM_WAV};
use super::{Result, Storage, StorageError};
use std::fs;

fn valid_speaker(key: &str) -> bool {
    key == "me" || key.strip_prefix("them:").is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// What a speaker is called before anyone names them (as the transcript shows it).
fn default_name(speaker: &str) -> String {
    match speaker {
        "me" => "Me".into(),
        "them:0" => "Them".into(),
        s => s.strip_prefix("them:").map(|n| format!("Speaker {n}")).unwrap_or_else(|| s.to_string()),
    }
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

    /// Transcript only: this meeting waits for Generate summary instead of being summarized by itself.
    pub fn set_transcript_only(&self, id: &str, on: bool) -> Result<()> {
        self.update_meeting_file(id, |f| f.transcript_only = on)
    }

    /// The meeting type to summarize it as (`summarizer::templates` id), or none for the Settings default.
    pub fn set_template(&self, id: &str, template: Option<&str>) -> Result<()> {
        if template.is_some_and(|t| !crate::summarizer::templates::exists(t)) {
            return Err(StorageError::Refused(crate::i18n::tr("error.unknown_template")));
        }
        self.update_meeting_file(id, |f| f.template = template.map(str::to_string))
    }

    /// Puts a meeting back in `summarizing` so the summarizer writes its summary again from the transcript (Generate
    /// summary, Summarize as…, Summarize in…). `template` and `language`, when given, are remembered on the meeting
    /// (the type and the language it is written in from now on). Refused, with nothing changed, for a language outside
    /// the summary languages, a meeting without a transcript and one still being recorded or transcribed.
    pub fn begin_summary(&self, id: &str, template: Option<&str>, language: Option<&str>) -> Result<()> {
        if language.is_some_and(|l| !crate::summarizer::prompt::is_summary_language(l)) {
            return Err(StorageError::Refused(crate::i18n::tr("error.unknown_language")));
        }
        if template.is_some_and(|t| !crate::summarizer::templates::exists(t)) {
            return Err(StorageError::Refused(crate::i18n::tr("error.unknown_template")));
        }
        let meeting = self.get_meeting(id)?;
        if meeting.transcript.is_empty() {
            return Err(StorageError::Refused(crate::i18n::tr("error.no_transcript_yet")));
        }
        if matches!(meeting.item.status, MeetingStatus::Recording | MeetingStatus::Transcribing) {
            return Err(StorageError::Refused(crate::i18n::tr("error.wait_transcript")));
        }
        self.update_meeting_file(id, |f| {
            f.status = MeetingStatus::Summarizing;
            f.progress = Some(0.0);
            f.summary_error = None;
            if let Some(t) = template {
                f.template = Some(t.to_string());
            }
            if let Some(l) = language {
                f.summary_language = Some(l.to_string());
            }
        })
    }

    /// The user's notes, edited in the meeting view: they replace every mark with text, and marked moments stay.
    /// Blank notes are dropped, a note without a (unique) id gets one, and timed notes come first, in time order.
    /// Refused while recording: the recorder owns the marks until Stop.
    pub fn set_notes(&self, id: &str, notes: &[Mark]) -> Result<()> {
        let file = self.read_meeting_file(id)?;
        if file.status == MeetingStatus::Recording {
            return Err(StorageError::Refused(crate::i18n::tr("error.notes_while_recording")));
        }
        let mut marks: Vec<Mark> = file.marks.iter().filter(|m| m.note_text().is_none()).cloned().collect();
        let mut kept: Vec<Mark> = vec![];
        for n in notes {
            let Some(text) = n.note_text().map(|t| t.chars().take(2_000).collect::<String>()) else { continue };
            let taken = |x: &str| marks.iter().chain(kept.iter()).any(|m| m.id == x);
            let id = if n.id.trim().is_empty() || taken(&n.id) { (0..).map(|i| format!("n{i}")).find(|c| !taken(c)).expect("a free id") } else { n.id.clone() };
            let at = if n.later { 0 } else { n.at };
            kept.push(Mark { id, at, note: Some(text), later: n.later });
        }
        kept.sort_by_key(|m| (m.later, m.at));
        marks.extend(kept);
        marks.sort_by_key(|m| (m.later, m.at));
        self.update_meeting_file(id, |f| f.marks = marks)?;
        self.regenerate_markdown(id)
    }

    pub fn rename_speaker(&self, id: &str, speaker: &str, name: &str) -> Result<()> {
        if !valid_speaker(speaker) {
            return Err(StorageError::InvalidId(speaker.to_string()));
        }
        let name = name.trim();
        if name.is_empty() {
            return Ok(());
        }
        // The names this speaker went by: as stored, and as shown (Me shows the user's name).
        let mut old: Vec<String> = vec![];
        self.update_meeting_file(id, |f| {
            old.push(f.speakers.get(speaker).cloned().unwrap_or_else(|| default_name(speaker)));
            old.extend(self.named_speakers(&f.speakers).get(speaker).cloned());
            f.speakers.insert(speaker.to_string(), name.to_string());
        })?;
        // Action items the summary gave to "Speaker 2" now belong to the name.
        let dir = self.dir(id)?;
        for file in [SUMMARY, SUMMARY_EDITED] {
            if let Some(mut summary) = read_json_opt::<Summary>(&dir.join(file))? {
                let mut hit = false;
                for a in &mut summary.action_items {
                    if a.owner.as_deref().is_some_and(|o| old.iter().any(|n| o.trim().eq_ignore_ascii_case(n))) {
                        a.owner = Some(name.to_string());
                        hit = true;
                    }
                }
                if hit {
                    write_json_atomic(&dir.join(file), &summary)?;
                }
            }
        }
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
            return Err(StorageError::Refused(crate::i18n::tr("error.audio_needed")));
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
    fn notes_replace_the_old_notes_and_keep_marked_moments() {
        let (_d, s) = storage();
        let id = make(&s);
        let mark = |id: &str, at: u64, note: Option<&str>, later: bool| Mark { id: id.into(), at, note: note.map(String::from), later };
        s.update_meeting_file(&id, |f| {
            f.status = MeetingStatus::Ready;
            f.marks = vec![mark("k0", 5_000, None, false), mark("k1", 9_000, Some("old note"), false)];
        })
        .unwrap();
        s.set_notes(&id, &[mark("k1", 9_000, Some(" edited note "), false), mark("", 0, Some("after the call"), true), mark("k0", 1_000, Some("clashing id"), false), mark("x", 0, Some("  "), false)]).unwrap();
        let marks = s.get_meeting(&id).unwrap().marks;
        assert_eq!(
            marks.iter().map(|m| (m.id.as_str(), m.at, m.note.as_deref(), m.later)).collect::<Vec<_>>(),
            vec![("n1", 1_000, Some("clashing id"), false), ("k0", 5_000, None, false), ("k1", 9_000, Some("edited note"), false), ("n0", 0, Some("after the call"), true)]
        );
        s.set_status(&id, MeetingStatus::Recording).unwrap();
        assert!(matches!(s.set_notes(&id, &[]), Err(StorageError::Refused(_))));
    }

    #[test]
    fn a_meeting_type_is_set_and_checked() {
        let (_d, s) = storage();
        let id = make(&s);
        s.set_template(&id, Some("interview")).unwrap();
        assert_eq!(s.get_meeting(&id).unwrap().template.as_deref(), Some("interview"));
        assert!(s.set_template(&id, Some("nope")).is_err());
        s.set_template(&id, None).unwrap();
        assert_eq!(s.get_meeting(&id).unwrap().template, None);
    }

    #[test]
    fn beginning_a_summary_saves_the_language_and_the_meeting_goes_to_summarizing() {
        let (_d, s) = storage();
        let id = make(&s);
        s.save_transcript(&id, &[line("l0", 1_000, "Hello.")]).unwrap();
        s.set_status(&id, MeetingStatus::Ready).unwrap();
        s.begin_summary(&id, None, Some("fil")).unwrap();
        let m = s.get_meeting(&id).unwrap();
        assert_eq!((m.item.status, m.summary_language.as_deref(), m.template), (MeetingStatus::Summarizing, Some("fil"), None));
        assert_eq!(m.item.progress, Some(0.0));
        // Regenerate (no language given) keeps the one chosen before; Summarize as… keeps it too.
        s.set_status(&id, MeetingStatus::Ready).unwrap();
        s.begin_summary(&id, Some("interview"), None).unwrap();
        let m = s.get_meeting(&id).unwrap();
        assert_eq!((m.summary_language.as_deref(), m.template.as_deref()), (Some("fil"), Some("interview")));
        // The language is saved in meeting.json under its own name.
        let raw = std::fs::read_to_string(s.dir(&id).unwrap().join(META)).unwrap();
        assert!(raw.contains("\"summaryLanguage\": \"fil\""), "{raw}");
    }

    #[test]
    fn an_unknown_language_is_refused_and_changes_nothing() {
        let (_d, s) = storage();
        let id = make(&s);
        s.save_transcript(&id, &[line("l0", 1_000, "Hello.")]).unwrap();
        s.set_status(&id, MeetingStatus::Ready).unwrap();
        for bad in ["klingon", "", "tl", "zh", "EN"] {
            assert!(matches!(s.begin_summary(&id, None, Some(bad)), Err(StorageError::Refused(_))), "{bad}");
        }
        let m = s.get_meeting(&id).unwrap();
        assert_eq!((m.item.status, m.summary_language, m.item.progress), (MeetingStatus::Ready, None, None));
    }

    #[test]
    fn a_summary_waits_for_the_transcript() {
        let (_d, s) = storage();
        let id = make(&s);
        // No transcript yet.
        s.set_status(&id, MeetingStatus::Ready).unwrap();
        assert!(matches!(s.begin_summary(&id, None, Some("fr")), Err(StorageError::Refused(_))));
        // A transcript, but recording or transcribing is not over.
        s.save_transcript(&id, &[line("l0", 1_000, "Hello.")]).unwrap();
        for st in [MeetingStatus::Recording, MeetingStatus::Transcribing] {
            s.set_status(&id, st).unwrap();
            assert!(matches!(s.begin_summary(&id, None, Some("fr")), Err(StorageError::Refused(_))));
        }
        let m = s.get_meeting(&id).unwrap();
        assert_eq!((m.item.status, m.summary_language), (MeetingStatus::Transcribing, None));
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
    fn renaming_a_speaker_moves_their_action_items() {
        let (_d, s) = storage();
        let id = make(&s);
        let mut sum = crate::storage::store::tests::summary("Recap.");
        sum.action_items[0].owner = Some("Speaker 2".into());
        s.save_summary(&id, &sum).unwrap();
        s.rename_speaker(&id, "them:2", "Priya").unwrap();
        let got = s.get_meeting(&id).unwrap().summary.unwrap();
        assert_eq!(got.action_items[0].owner.as_deref(), Some("Priya"));
        // Renaming again follows the new name.
        s.rename_speaker(&id, "them:2", "Priya Shah").unwrap();
        assert_eq!(s.get_meeting(&id).unwrap().summary.unwrap().action_items[0].owner.as_deref(), Some("Priya Shah"));
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
    fn me_and_them_can_be_renamed_and_their_action_items_follow() {
        let (_d, mut s) = storage();
        let id = make(&s);
        s.save_summary(&id, &super::super::store::tests::summary("recap")).unwrap(); // owner "Me"
        s.settings_set(serde_json::json!({ "yourName": "Miguel" })).unwrap();
        s.rename_speaker(&id, "me", "Miguel M.").unwrap();
        s.rename_speaker(&id, "them:0", "Dana").unwrap();
        let m = s.get_meeting(&id).unwrap();
        assert_eq!((m.speakers["me"].as_str(), m.speakers["them:0"].as_str()), ("Miguel M.", "Dana"));
        assert_eq!(m.summary.unwrap().action_items[0].owner.as_deref(), Some("Miguel M."));
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
