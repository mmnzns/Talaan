//! The `Storage` type: create, read and list meetings on disk.

use super::io::{read_json, read_json_opt, remove_if_exists, write_json_atomic};
use super::model::*;
use super::paths::{self, *};
use super::retention::{AudioPolicy, Policy};
use super::{Result, StorageError};
use chrono::{DateTime, Duration, FixedOffset};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone)]
pub struct Filter {
    pub folder_id: Option<String>,
    pub tag: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug)]
pub struct Storage {
    pub(crate) root: PathBuf,
    pub(crate) policy: Policy,
}

impl Storage {
    pub fn new(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(paths::meetings_dir(&root))?;
        Ok(Storage { root, policy: Policy::default() })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn policy(&self) -> Policy {
        self.policy
    }

    pub fn set_policy(&mut self, policy: Policy) {
        self.policy = policy;
    }

    pub(crate) fn dir(&self, id: &str) -> Result<PathBuf> {
        if !is_valid_id(id) {
            return Err(StorageError::InvalidId(id.to_string()));
        }
        let dir = meeting_dir(&self.root, id);
        if dir.is_dir() {
            Ok(dir)
        } else {
            Err(StorageError::NotFound(id.to_string()))
        }
    }

    /// Creates the folder the moment recording starts, marked `recording`.
    pub fn create_meeting(&self, app: &str, app_label: &str, started_at: DateTime<FixedOffset>) -> Result<MeetingFile> {
        let meetings = meetings_dir(&self.root);
        fs::create_dir_all(&meetings)?;
        let mut last_err = None;
        for _ in 0..20 {
            let id = new_meeting_id(&started_at);
            match fs::create_dir(meetings.join(&id)) {
                Ok(()) => {
                    let file = MeetingFile {
                        schema_version: SCHEMA_VERSION,
                        id: id.clone(),
                        title: placeholder_title(app_label, &started_at),
                        title_is_placeholder: true,
                        app: app.to_string(),
                        app_label: app_label.to_string(),
                        started_at: started_at.to_rfc3339(),
                        duration_ms: 0,
                        language: "en".to_string(),
                        status: MeetingStatus::Recording,
                        pinned: false,
                        folder_id: None,
                        tags: vec![],
                        people_count: 2,
                        calendar_event: None,
                        speakers: BTreeMap::from([("me".to_string(), "Me".to_string()), ("them:0".to_string(), "Them".to_string())]),
                        marks: vec![],
                        pauses: vec![],
                        engine: None,
                        audio_deleted: false,
                        progress: None,
                        eta: None,
                        error: None,
                        transcription: None,
                        summary_error: None,
                        transcript_only: false,
                        template: None,
                        retranscribe_model: None,
                        exported_copy: None,
                        autosave_pending: None,
                        extra: Default::default(),
                    };
                    write_json_atomic(&meetings.join(&id).join(META), &file)?;
                    return Ok(file);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last_err = Some(e),
                Err(e) => return Err(e.into()),
            }
        }
        Err(last_err.map(Into::into).unwrap_or_else(|| StorageError::NotFound("could not allocate a meeting id".into())))
    }

    pub fn read_meeting_file(&self, id: &str) -> Result<MeetingFile> {
        let dir = self.dir(id)?;
        let file: MeetingFile = read_json(&dir.join(META))?;
        if file.schema_version > SCHEMA_VERSION {
            return Err(StorageError::UnsupportedSchema { id: id.to_string(), found: file.schema_version });
        }
        Ok(file)
    }

    pub fn write_meeting_file(&self, file: &MeetingFile) -> Result<()> {
        let dir = self.dir(&file.id)?;
        write_json_atomic(&dir.join(META), file)
    }

    /// Reads, lets `f` change the file, writes it back. Unknown fields stay in `extra`.
    pub fn update_meeting_file<R>(&self, id: &str, f: impl FnOnce(&mut MeetingFile) -> R) -> Result<R> {
        let mut file = self.read_meeting_file(id)?;
        let out = f(&mut file);
        self.write_meeting_file(&file)?;
        Ok(out)
    }

    pub fn has_audio(&self, dir: &Path, file: &MeetingFile) -> bool {
        !file.audio_deleted && (dir.join(ME_WAV).exists() || dir.join(THEM_WAV).exists())
    }

    pub fn audio_kept_until(&self, file: &MeetingFile, has_audio: bool) -> Option<String> {
        if !has_audio || file.pinned {
            return None;
        }
        match self.policy.audio {
            AudioPolicy::Days(d) => DateTime::parse_from_rfc3339(&file.started_at).ok().map(|t| (t + Duration::days(d as i64)).to_rfc3339()),
            AudioPolicy::Forever | AudioPolicy::Never => None,
        }
    }

    pub fn get_meeting(&self, id: &str) -> Result<Meeting> {
        let file = self.read_meeting_file(id)?;
        let dir = self.dir(id)?;
        let edits: BTreeMap<String, String> = read_json_opt(&dir.join(TRANSCRIPT_EDITS))?.unwrap_or_default();
        let mut transcript: Vec<TranscriptLine> = read_json_opt(&dir.join(TRANSCRIPT))?.unwrap_or_default();
        for line in &mut transcript {
            if let Some(text) = edits.get(&line.id) {
                line.text = text.clone();
                line.edited = true;
            } else {
                line.edited = false;
            }
        }
        let summary = match read_json_opt::<Summary>(&dir.join(SUMMARY_EDITED))? {
            Some(mut s) => {
                s.edited = true;
                Some(s)
            }
            None => read_json_opt::<Summary>(&dir.join(SUMMARY))?.map(|mut s| {
                s.edited = false;
                s
            }),
        };
        let summary = match summary {
            Some(mut s) => {
                let done = self.read_done(id)?;
                for a in &mut s.action_items {
                    a.done = done.get(&a.id).copied().unwrap_or(false);
                }
                Some(s)
            }
            None => None,
        };
        let speakers = self.named_speakers(&file.speakers);
        let summary = summary.map(|mut s| {
            // Summaries written before the user had a name (or by a model that said "Me" anyway).
            if let Some(me) = speakers.get("me").filter(|n| n.as_str() != "Me") {
                for a in &mut s.action_items {
                    if a.owner.as_deref().is_some_and(|o| o.trim() == "Me") {
                        a.owner = Some(me.clone());
                    }
                }
            }
            s
        });
        let has_audio = self.has_audio(&dir, &file);
        Ok(Meeting {
            audio_kept_until: self.audio_kept_until(&file, has_audio),
            has_audio,
            language: file.language.clone(),
            calendar_event: file.calendar_event.clone(),
            speakers,
            summary,
            transcript,
            marks: file.marks.clone(),
            pauses: file.pauses.clone(),
            error: file.error.clone(),
            summary_error: file.summary_error.clone(),
            transcription: file.transcription.clone(),
            transcript_only: file.transcript_only,
            template: file.template.clone(),
            item: list_item(&file),
        })
    }

    /// Settings › General "Your name", when set.
    pub fn your_name(&self) -> Option<String> {
        let v = self.settings_get().ok().flatten()?;
        v["yourName"].as_str().map(str::trim).filter(|n| !n.is_empty()).map(str::to_string)
    }

    /// A meeting's speaker names as everyone sees them: Me is the user's name unless this meeting renamed it.
    /// New meetings store "Me", which counts as not renamed, so changing Your name later updates them all.
    pub fn named_speakers(&self, stored: &BTreeMap<String, String>) -> BTreeMap<String, String> {
        let mut speakers = stored.clone();
        if let Some(name) = self.your_name() {
            if stored.get("me").is_none_or(|n| n.trim().is_empty() || n == "Me") {
                speakers.insert("me".into(), name);
            }
        }
        speakers
    }

    /// Meetings in a given status (the transcriber's queue is rebuilt from `transcribing` at launch).
    pub fn ids_with_status(&self, status: MeetingStatus) -> Result<Vec<String>> {
        let mut ids = vec![];
        for item in self.list_meetings(&Filter::default())? {
            if item.status == status {
                ids.push(item.id);
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Writes the transcript the transcriber produced. Never rewritten after that; edits go beside it.
    pub fn save_transcript(&self, id: &str, lines: &[TranscriptLine]) -> Result<()> {
        let dir = self.dir(id)?;
        write_json_atomic(&dir.join(TRANSCRIPT), &lines)?;
        self.regenerate_markdown(id)
    }

    /// Writes the summary the summarizer produced. A new summary (Generate again, Summarize as…) replaces the old
    /// one's edits and ticks: they belong to items that are gone.
    pub fn save_summary(&self, id: &str, summary: &Summary) -> Result<()> {
        let dir = self.dir(id)?;
        write_json_atomic(&dir.join(SUMMARY), summary)?;
        remove_if_exists(&dir.join(SUMMARY_EDITED))?;
        remove_if_exists(&dir.join(SUMMARY_DONE))?;
        self.regenerate_markdown(id)
    }

    pub fn list_meetings(&self, filter: &Filter) -> Result<Vec<MeetingListItem>> {
        let mut items: Vec<(Option<DateTime<FixedOffset>>, MeetingListItem)> = vec![];
        let query = filter.query.as_deref().map(str::trim).filter(|q| !q.is_empty()).map(str::to_lowercase);

        for entry in fs::read_dir(meetings_dir(&self.root))? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().to_string();
            if !is_valid_id(&id) {
                continue;
            }
            let item = match self.read_meeting_file(&id) {
                Ok(file) => {
                    if filter.folder_id.as_ref().is_some_and(|f| file.folder_id.as_ref() != Some(f)) {
                        continue;
                    }
                    if filter.tag.as_ref().is_some_and(|t| !file.tags.contains(t)) {
                        continue;
                    }
                    if let Some(q) = &query {
                        if !self.matches_query(&entry.path(), &file, q) {
                            continue;
                        }
                    }
                    list_item(&file)
                }
                // Unreadable or too new: still shown (so it isn't silently lost) but never touched.
                Err(_) => {
                    if filter.folder_id.is_some() || filter.tag.is_some() || query.is_some() {
                        continue;
                    }
                    broken_item(&id)
                }
            };
            items.push((DateTime::parse_from_rfc3339(&item.started_at).ok(), item));
        }
        items.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.id.cmp(&a.1.id)));
        Ok(items.into_iter().map(|(_, i)| i).collect())
    }

    fn matches_query(&self, dir: &Path, file: &MeetingFile, q: &str) -> bool {
        if file.title.to_lowercase().contains(q) {
            return true;
        }
        let summary = read_json_opt::<Summary>(&dir.join(SUMMARY_EDITED)).ok().flatten().or_else(|| read_json_opt::<Summary>(&dir.join(SUMMARY)).ok().flatten());
        if let Some(s) = summary {
            let hay = std::iter::once(s.recap.as_str())
                .chain(s.decisions.iter().map(|d| d.text.as_str()))
                .chain(s.action_items.iter().map(|a| a.text.as_str()))
                .chain(s.open_questions.iter().map(|o| o.text.as_str()));
            if hay.into_iter().any(|t| t.to_lowercase().contains(q)) {
                return true;
            }
        }
        let edits: BTreeMap<String, String> = read_json_opt(&dir.join(TRANSCRIPT_EDITS)).ok().flatten().unwrap_or_default();
        let lines: Vec<TranscriptLine> = read_json_opt(&dir.join(TRANSCRIPT)).ok().flatten().unwrap_or_default();
        lines.iter().any(|l| edits.get(&l.id).unwrap_or(&l.text).to_lowercase().contains(q))
    }
}

pub(crate) fn list_item(f: &MeetingFile) -> MeetingListItem {
    MeetingListItem {
        id: f.id.clone(),
        title: f.title.clone(),
        app: f.app.clone(),
        app_label: f.app_label.clone(),
        started_at: f.started_at.clone(),
        duration_ms: f.duration_ms,
        status: f.status,
        progress: f.progress,
        eta: f.eta.clone().filter(|_| matches!(f.status, MeetingStatus::Transcribing | MeetingStatus::Summarizing)),
        folder_id: f.folder_id.clone(),
        tags: f.tags.clone(),
        people_count: f.people_count,
        pinned: f.pinned,
    }
}

fn broken_item(id: &str) -> MeetingListItem {
    MeetingListItem {
        id: id.to_string(),
        title: id.to_string(),
        app: "other".into(),
        app_label: "Unreadable".into(),
        started_at: String::new(),
        duration_ms: 0,
        status: MeetingStatus::Failed,
        progress: None,
        eta: None,
        folder_id: None,
        tags: vec![],
        people_count: 0,
        pinned: false,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tempfile::TempDir;

    pub fn storage() -> (TempDir, Storage) {
        let dir = tempfile::tempdir().unwrap();
        let s = Storage::new(dir.path().to_path_buf()).unwrap();
        (dir, s)
    }

    pub fn at(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    pub fn line(id: &str, at_ms: u64, text: &str) -> TranscriptLine {
        TranscriptLine { id: id.into(), at: at_ms, end: at_ms + 5000, speaker: "me".into(), text: text.into(), edited: false }
    }

    pub fn summary(recap: &str) -> Summary {
        Summary {
            recap: recap.into(),
            decisions: vec![SummaryItem { id: "d0".into(), text: "Ship onboarding first".into(), at: Some(21_000) }],
            action_items: vec![ActionItem { id: "a0".into(), text: "Send wireframes".into(), at: None, owner: Some("Me".into()), due: None, done: false }],
            open_questions: vec![],
            sections: vec![],
            highlights: vec![],
            template: None,
            sections_missing: false,
            engine: "Local AI".into(),
            edited: false,
        }
    }

    #[test]
    fn create_then_get() {
        let (dir, s) = storage();
        let f = s.create_meeting("zoom", "Zoom", at("2026-10-14T14:30:00-07:00")).unwrap();
        assert_eq!(f.status, MeetingStatus::Recording);
        assert_eq!(f.title, "Call, Zoom, Oct 14, 2:30 PM");
        assert!(f.title_is_placeholder);
        assert!(dir.path().join("meetings").join(&f.id).join("meeting.json").is_file());
        let m = s.get_meeting(&f.id).unwrap();
        assert_eq!(m.item.id, f.id);
        assert_eq!(m.item.status, MeetingStatus::Recording);
        assert!(m.transcript.is_empty());
        assert!(m.summary.is_none());
        assert!(!m.has_audio);
    }

    #[test]
    fn two_meetings_in_the_same_minute_get_different_ids() {
        let (_d, s) = storage();
        let a = s.create_meeting("zoom", "Zoom", at("2026-10-14T14:30:00-07:00")).unwrap();
        let b = s.create_meeting("zoom", "Zoom", at("2026-10-14T14:30:00-07:00")).unwrap();
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn get_unknown_or_unsafe_id_fails() {
        let (_d, s) = storage();
        assert!(matches!(s.get_meeting("nope"), Err(StorageError::NotFound(_))));
        assert!(matches!(s.get_meeting("../x"), Err(StorageError::InvalidId(_))));
    }

    #[test]
    fn list_is_newest_first() {
        let (_d, s) = storage();
        let a = s.create_meeting("zoom", "Zoom", at("2026-10-12T09:00:00-07:00")).unwrap();
        let b = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap();
        let c = s.create_meeting("zoom", "Zoom", at("2026-10-13T09:00:00-07:00")).unwrap();
        let ids: Vec<_> = s.list_meetings(&Filter::default()).unwrap().into_iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![b.id, c.id, a.id]);
    }

    #[test]
    fn list_filters_by_folder_tag_and_query() {
        let (_d, s) = storage();
        let a = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap();
        let b = s.create_meeting("slack", "Slack huddle", at("2026-10-13T09:00:00-07:00")).unwrap();
        s.update_meeting_file(&a.id, |f| {
            f.folder_id = Some("work".into());
            f.tags = vec!["roadmap".into()];
            f.title = "Q4 roadmap sync".into();
        })
        .unwrap();
        s.save_transcript(&b.id, &[line("l0", 0, "We should talk about the harbour project")]).unwrap();
        s.save_summary(&b.id, &summary("Weekly sync")).unwrap();

        let ids = |f: Filter| -> Vec<String> { s.list_meetings(&f).unwrap().into_iter().map(|i| i.id).collect() };
        assert_eq!(ids(Filter { folder_id: Some("work".into()), ..Default::default() }), vec![a.id.clone()]);
        assert_eq!(ids(Filter { tag: Some("roadmap".into()), ..Default::default() }), vec![a.id.clone()]);
        assert_eq!(ids(Filter { query: Some("ROADMAP".into()), ..Default::default() }), vec![a.id.clone()]);
        assert_eq!(ids(Filter { query: Some("harbour".into()), ..Default::default() }), vec![b.id.clone()]);
        assert_eq!(ids(Filter { query: Some("weekly sync".into()), ..Default::default() }), vec![b.id.clone()]);
        assert!(ids(Filter { query: Some("zzzz".into()), ..Default::default() }).is_empty());
    }

    #[test]
    fn get_merges_transcript_and_summary() {
        let (_d, s) = storage();
        let f = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap();
        s.save_transcript(&f.id, &[line("l0", 0, "hello")]).unwrap();
        s.save_summary(&f.id, &summary("recap")).unwrap();
        let m = s.get_meeting(&f.id).unwrap();
        assert_eq!(m.transcript[0].text, "hello");
        assert_eq!(m.summary.unwrap().recap, "recap");
    }

    #[test]
    fn corrupt_meeting_json_is_listed_as_failed_not_a_crash() {
        let (dir, s) = storage();
        let good = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap();
        let bad_dir = dir.path().join("meetings").join("2026-10-13_0900_bad1");
        fs::create_dir(&bad_dir).unwrap();
        fs::write(bad_dir.join("meeting.json"), "{ not json").unwrap();
        let items = s.list_meetings(&Filter::default()).unwrap();
        assert_eq!(items.len(), 2);
        let bad = items.iter().find(|i| i.id == "2026-10-13_0900_bad1").unwrap();
        assert_eq!(bad.status, MeetingStatus::Failed);
        assert_eq!(items.iter().find(|i| i.id == good.id).unwrap().status, MeetingStatus::Recording);
        assert!(s.get_meeting("2026-10-13_0900_bad1").is_err());
        assert_eq!(fs::read_to_string(bad_dir.join("meeting.json")).unwrap(), "{ not json");
    }

    #[test]
    fn newer_schema_is_listed_as_failed_and_left_untouched() {
        let (dir, s) = storage();
        let f = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap();
        let path = dir.path().join("meetings").join(&f.id).join("meeting.json");
        let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        v["schemaVersion"] = 99.into();
        fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();
        let before = fs::read(&path).unwrap();
        let items = s.list_meetings(&Filter::default()).unwrap();
        assert_eq!(items[0].status, MeetingStatus::Failed);
        assert!(matches!(s.get_meeting(&f.id), Err(StorageError::UnsupportedSchema { found: 99, .. })));
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn your_name_names_me_unless_the_meeting_renamed_it() {
        let (_d, mut s) = storage();
        let f = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap();
        s.save_summary(&f.id, &summary("recap")).unwrap();
        assert_eq!(s.get_meeting(&f.id).unwrap().speakers["me"], "Me");
        s.settings_set(serde_json::json!({ "yourName": " Miguel " })).unwrap();
        let m = s.get_meeting(&f.id).unwrap();
        assert_eq!(m.speakers["me"], "Miguel");
        assert_eq!(m.speakers["them:0"], "Them");
        // Older summaries said "Me" for the owner.
        assert_eq!(m.summary.unwrap().action_items[0].owner.as_deref(), Some("Miguel"));
        // meeting.json still says "Me", so a later change of name follows.
        assert_eq!(s.read_meeting_file(&f.id).unwrap().speakers["me"], "Me");
        s.update_meeting_file(&f.id, |m| { m.speakers.insert("me".into(), "Miggy".into()); }).unwrap();
        assert_eq!(s.get_meeting(&f.id).unwrap().speakers["me"], "Miggy");
    }

    #[test]
    fn audio_presence_and_expiry_are_derived() {
        let (dir, mut s) = storage();
        s.set_policy(Policy::from_settings("7d", "forever"));
        let f = s.create_meeting("zoom", "Zoom", at("2026-10-14T14:30:00-07:00")).unwrap();
        assert!(!s.get_meeting(&f.id).unwrap().has_audio);
        let d = dir.path().join("meetings").join(&f.id);
        fs::write(d.join("me.wav"), b"x").unwrap();
        let m = s.get_meeting(&f.id).unwrap();
        assert!(m.has_audio);
        assert_eq!(m.audio_kept_until.as_deref(), Some("2026-10-21T14:30:00-07:00"));
        s.set_policy(Policy::from_settings("forever", "forever"));
        assert!(s.get_meeting(&f.id).unwrap().audio_kept_until.is_none());
    }
}
