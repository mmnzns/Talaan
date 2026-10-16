//! Types shared with the frontend (`src/data/types.ts`). Field names are camelCase on the wire.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MeetingStatus {
    Recording,
    Transcribing,
    Summarizing,
    Ready,
    Failed,
    NeedsRecovery,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub title: String,
    pub attendees: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mark {
    pub id: String,
    pub at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PauseGap {
    pub at: u64,
    pub duration_ms: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SummaryItem {
    pub id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActionItem {
    pub id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    pub done: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub recap: String,
    pub decisions: Vec<SummaryItem>,
    pub action_items: Vec<ActionItem>,
    pub open_questions: Vec<SummaryItem>,
    pub engine: String,
    /// Computed on read (true when `summary.edited.json` exists). Never trust it from disk.
    #[serde(default)]
    pub edited: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptLine {
    pub id: String,
    pub at: u64,
    pub end: u64,
    pub speaker: String,
    pub text: String,
    /// Computed on read (true when an edit exists for this line).
    #[serde(default)]
    pub edited: bool,
}

/// What lives in `meeting.json`. Unknown fields are kept so newer builds' data survives older builds.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MeetingFile {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub title_is_placeholder: bool,
    pub app: String,
    pub app_label: String,
    /// RFC 3339 with the local offset.
    pub started_at: String,
    #[serde(default)]
    pub duration_ms: u64,
    pub language: String,
    pub status: MeetingStatus,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub people_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_event: Option<CalendarEvent>,
    #[serde(default)]
    pub speakers: BTreeMap<String, String>,
    #[serde(default)]
    pub marks: Vec<Mark>,
    #[serde(default)]
    pub pauses: Vec<PauseGap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(default)]
    pub audio_deleted: bool,
    /// 0..1 while transcribing or summarizing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// Why processing failed (status `failed`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ProcessingError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcription: Option<TranscriptionInfo>,
    /// Why there is no summary (meeting is `ready`, transcript-only): no engine set up, or the AI failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_error: Option<ProcessingError>,
    /// Keep this meeting transcript-only until the user asks for a summary (set from the recording controls).
    #[serde(default, skip_serializing_if = "is_false")]
    pub transcript_only: bool,
    /// "Transcribe again" with this speech model; cleared once the new transcript is saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retranscribe_model: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingError {
    /// `no_model` (re-queued when a model finishes downloading) or `failed`.
    pub code: String,
    pub message: String,
    /// For debugging, not shown: what the engine reported when it failed (the local AI's exit status and error
    /// lines). Never transcript text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// How the transcript was made, for the record and for comparing engines.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionInfo {
    pub model: String,
    pub echo_lines_removed: usize,
    pub speech_ms: u64,
    pub took_ms: u64,
    pub finished_at: String,
}

/// A list row (matches `MeetingListItem`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MeetingListItem {
    pub id: String,
    pub title: String,
    pub app: String,
    pub app_label: String,
    pub started_at: String,
    pub duration_ms: u64,
    pub status: MeetingStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_id: Option<String>,
    pub tags: Vec<String>,
    pub people_count: u32,
    pub pinned: bool,
}

/// A full meeting as the frontend sees it (matches `Meeting`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Meeting {
    #[serde(flatten)]
    pub item: MeetingListItem,
    pub language: String,
    pub has_audio: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_kept_until: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_event: Option<CalendarEvent>,
    pub speakers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<Summary>,
    pub transcript: Vec<TranscriptLine>,
    pub marks: Vec<Mark>,
    pub pauses: Vec<PauseGap>,
    /// Why processing failed (status `failed`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ProcessingError>,
    /// Why there is no summary yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_error: Option<ProcessingError>,
    /// How the transcript was made (the speech model), once there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcription: Option<TranscriptionInfo>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub transcript_only: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../fixtures/meeting.sample.json");

    #[test]
    fn sample_meeting_round_trips_unchanged() {
        let original: Value = serde_json::from_str(FIXTURE).unwrap();
        let meeting: Meeting = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(serde_json::to_value(&meeting).unwrap(), original);
    }

    #[test]
    fn meeting_file_keeps_unknown_fields() {
        let json = r#"{"schemaVersion":1,"id":"x","title":"t","app":"zoom","appLabel":"Zoom",
            "startedAt":"2026-10-14T14:30:00-07:00","language":"en","status":"ready","futureField":{"a":1}}"#;
        let file: MeetingFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.extra["futureField"]["a"], 1);
        let back = serde_json::to_value(&file).unwrap();
        assert_eq!(back["futureField"]["a"], 1);
    }

    #[test]
    fn status_uses_snake_case_names() {
        assert_eq!(serde_json::to_string(&MeetingStatus::NeedsRecovery).unwrap(), "\"needs_recovery\"");
    }
}
