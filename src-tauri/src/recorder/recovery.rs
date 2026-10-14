//! Recovering a recording that was cut off by a crash: fix the WAV headers and the duration.

use super::wav::{repair_wav, wav_duration_ms};
use crate::storage::model::MeetingStatus;
use crate::storage::paths::{ME_WAV, THEM_WAV};
use crate::storage::{Storage, StorageError};

/// Repairs both tracks, sets the duration from the longer one and hands the meeting to the transcriber.
pub fn recover(storage: &Storage, id: &str) -> Result<u64, StorageError> {
    let dir = storage.dir(id)?;
    let mut longest = 0;
    for name in [ME_WAV, THEM_WAV] {
        let path = dir.join(name);
        if path.exists() && repair_wav(&path).is_ok() {
            longest = longest.max(wav_duration_ms(&path).unwrap_or(0));
        }
    }
    storage.update_meeting_file(id, |f| {
        f.duration_ms = f.duration_ms.max(longest);
        f.status = MeetingStatus::Transcribing;
        f.progress = Some(0.0);
        f.error = None;
    })?;
    Ok(longest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::timeline::SampleSink;
    use crate::recorder::wav::WavSink;
    use std::io::Write;

    #[test]
    fn a_crashed_recording_comes_back_with_its_audio() {
        let d = tempfile::tempdir().unwrap();
        let storage = Storage::new(d.path().to_path_buf()).unwrap();
        let f = storage.create_meeting("zoom", "Zoom", chrono::DateTime::parse_from_rfc3339("2026-10-14T14:30:00-07:00").unwrap()).unwrap();
        let dir = storage.dir(&f.id).unwrap();
        for name in [ME_WAV, THEM_WAV] {
            let mut s = WavSink::create(&dir.join(name)).unwrap();
            s.write(&vec![3i16; 32_000]).unwrap();
            s.flush().unwrap();
            std::mem::forget(s); // the crash
            let mut raw = std::fs::OpenOptions::new().append(true).open(dir.join(name)).unwrap();
            raw.write_all(&vec![0u8; 32_000]).unwrap(); // 1 s that never made it into the header
        }
        storage.scan_for_interrupted().unwrap();
        assert_eq!(recover(&storage, &f.id).unwrap(), 3_000);
        let m = storage.get_meeting(&f.id).unwrap();
        assert_eq!(m.item.status, MeetingStatus::Transcribing);
        assert_eq!(m.item.duration_ms, 3_000);
        assert!(m.has_audio);
    }

    #[test]
    fn a_meeting_without_audio_still_goes_to_the_transcriber() {
        let d = tempfile::tempdir().unwrap();
        let storage = Storage::new(d.path().to_path_buf()).unwrap();
        let f = storage.create_meeting("zoom", "Zoom", chrono::DateTime::parse_from_rfc3339("2026-10-14T14:30:00-07:00").unwrap()).unwrap();
        assert_eq!(recover(&storage, &f.id).unwrap(), 0);
        assert_eq!(storage.get_meeting(&f.id).unwrap().item.status, MeetingStatus::Transcribing);
    }
}
