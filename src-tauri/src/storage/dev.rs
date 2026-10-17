//! Developer helpers: load sample meetings into a real library, and wipe it again. Used by the dev-only commands.

use super::io::write_json_atomic;
use super::model::*;
use super::paths::*;
use super::{Result, Storage};
use std::collections::BTreeMap;
use std::fs;

/// One second of 16 kHz mono silence, so a seeded meeting really has audio files.
fn silent_wav() -> Vec<u8> {
    let samples = 16_000u32;
    let data_len = samples * 2;
    let mut v = Vec::with_capacity(44 + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // PCM
    v.extend_from_slice(&1u16.to_le_bytes()); // mono
    v.extend_from_slice(&16_000u32.to_le_bytes());
    v.extend_from_slice(&32_000u32.to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    v.resize(44 + data_len as usize, 0);
    v
}

impl Storage {
    /// Writes (or overwrites) a meeting from its frontend shape. Meetings that were mid-processing become ready.
    pub fn dev_seed_meeting(&self, m: &Meeting) -> Result<()> {
        let dir = meeting_dir(&self.root, &m.item.id);
        fs::create_dir_all(&dir)?;
        let status = match m.item.status {
            MeetingStatus::Transcribing | MeetingStatus::Summarizing => MeetingStatus::Ready,
            s => s,
        };
        let file = MeetingFile {
            schema_version: SCHEMA_VERSION,
            id: m.item.id.clone(),
            title: m.item.title.clone(),
            title_is_placeholder: false,
            app: m.item.app.clone(),
            app_label: m.item.app_label.clone(),
            started_at: m.item.started_at.clone(),
            duration_ms: m.item.duration_ms,
            language: m.language.clone(),
            status,
            pinned: m.item.pinned,
            folder_id: m.item.folder_id.clone(),
            tags: m.item.tags.clone(),
            people_count: m.item.people_count,
            calendar_event: m.calendar_event.clone(),
            speakers: m.speakers.clone(),
            marks: m.marks.clone(),
            pauses: m.pauses.clone(),
            engine: m.summary.as_ref().map(|s| s.engine.clone()),
            audio_deleted: !m.has_audio,
            progress: m.item.progress,
            eta: m.item.eta.clone(),
            error: m.error.clone(),
            transcription: m.transcription.clone(),
            summary_error: None,
            transcript_only: m.transcript_only,
            retranscribe_model: None,
            exported_copy: None,
            extra: Default::default(),
        };
        write_json_atomic(&dir.join(META), &file)?;

        if m.has_audio {
            let wav = silent_wav();
            fs::write(dir.join(ME_WAV), &wav)?;
            fs::write(dir.join(THEM_WAV), &wav)?;
        }
        if !m.transcript.is_empty() {
            self.save_transcript(&m.item.id, &m.transcript)?;
        }
        if let Some(summary) = &m.summary {
            self.save_summary(&m.item.id, summary)?;
            self.update_summary(&m.item.id, summary)?; // persists the done flags
        }
        self.regenerate_markdown(&m.item.id)
    }

    pub fn dev_set_folders(&self, folders: &[(String, String)]) -> Result<()> {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct F<'a> {
            schema_version: u32,
            folders: Vec<BTreeMap<&'a str, &'a str>>,
        }
        let folders = folders.iter().map(|(id, name)| BTreeMap::from([("id", id.as_str()), ("name", name.as_str())])).collect();
        write_json_atomic(&self.root.join(LIBRARY), &F { schema_version: 1, folders })
    }

    /// Removes every meeting and folder but keeps the settings.
    pub fn dev_wipe_library(&self) -> Result<()> {
        let dir = meetings_dir(&self.root);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;
        super::io::remove_if_exists(&self.root.join(LIBRARY))
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::tests::storage;
    use super::*;

    fn sample() -> Meeting {
        serde_json::from_str(include_str!("../../../fixtures/meeting.sample.json")).unwrap()
    }

    #[test]
    fn a_seeded_meeting_reads_back_like_the_fixture() {
        let (d, s) = storage();
        let m = sample();
        s.dev_seed_meeting(&m).unwrap();
        let back = s.get_meeting(&m.item.id).unwrap();
        assert_eq!(back.item.title, "Q4 roadmap sync");
        assert_eq!(back.transcript.len(), 2);
        assert_eq!(back.marks, m.marks);
        assert_eq!(back.pauses, m.pauses);
        assert_eq!(back.speakers, m.speakers);
        assert!(back.has_audio);
        let sum = back.summary.unwrap();
        assert!(sum.action_items[1].done);
        assert!(!sum.edited);
        assert!(d.path().join("meetings").join(&m.item.id).join("summary.md").exists());
    }

    #[test]
    fn seeding_twice_overwrites_instead_of_failing() {
        let (_d, s) = storage();
        let m = sample();
        s.dev_seed_meeting(&m).unwrap();
        s.dev_seed_meeting(&m).unwrap();
        assert_eq!(s.list_meetings(&Default::default()).unwrap().len(), 1);
    }

    #[test]
    fn folders_and_wipe() {
        let (_d, s) = storage();
        s.dev_set_folders(&[("work".into(), "Work".into())]).unwrap();
        s.dev_seed_meeting(&sample()).unwrap();
        assert_eq!(s.list_folders().unwrap()[0].count, 1);
        s.dev_wipe_library().unwrap();
        assert!(s.list_meetings(&Default::default()).unwrap().is_empty());
        assert!(s.list_folders().unwrap().is_empty());
    }

    #[test]
    fn the_silent_wav_is_a_valid_header() {
        let w = silent_wav();
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..12], b"WAVE");
        assert_eq!(w.len(), 44 + 32_000);
    }
}
