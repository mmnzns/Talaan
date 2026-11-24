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
    /// The attendees' email addresses, when the calendar has them (Email this summary and Draft follow-up send to them).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emails: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mark {
    pub id: String,
    pub at: u64,
    /// A mark with text is one of the user's notes; without text it is a marked moment (a summary highlight).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// A note written after the call, in the meeting view: it has no moment, so `at` means nothing.
    #[serde(default, skip_serializing_if = "is_false")]
    pub later: bool,
}

impl Mark {
    /// The note's text, when this mark is a note.
    pub fn note_text(&self) -> Option<&str> {
        self.note.as_deref().map(str::trim).filter(|n| !n.is_empty())
    }
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
    /// One sentence on why the meeting happened (the detailed layout); none for older summaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// 3 to 6 short bold-labeled points (the detailed layout).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub takeaways: Vec<Takeaway>,
    /// One entry per main topic, each with bullets (the detailed layout); empty for older summaries and meeting types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<SummaryTopic>,
    pub decisions: Vec<SummaryItem>,
    pub action_items: Vec<ActionItem>,
    pub open_questions: Vec<SummaryItem>,
    /// The meeting type's own parts ("Strengths", "Blockers"); empty for General.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<SummarySection>,
    /// One per marked moment: its time and what was said around it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<SummaryItem>,
    /// The meeting type it was written as (`summarizer::templates`); none for summaries from before meeting types.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// The language it was written in (a `summarizer::prompt` language code); none for summaries from before the
    /// language switch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// The meeting type's sections couldn't be written this time (the reply was cut off and the plain summary was
    /// asked for instead); the summary card offers to try again.
    #[serde(default, skip_serializing_if = "is_false")]
    pub sections_missing: bool,
    pub engine: String,
    /// Computed on read (true when `summary.edited.json` exists). Never trust it from disk.
    #[serde(default)]
    pub edited: bool,
}

impl Summary {
    /// Whether this is the detailed layout (purpose, takeaways, topics); older summaries and the other meeting types are not.
    pub fn is_detailed(&self) -> bool {
        self.purpose.as_deref().is_some_and(|p| !p.trim().is_empty()) || !self.takeaways.is_empty() || !self.topics.is_empty()
    }

    /// The action items grouped by owner: the user's group first (`you`, their name as Your name, or `you_label` when
    /// it is empty; "Me" and the name are the same person), then the other owners in the order they first appear (the
    /// same person written in another case counts once), items without an owner last under `None`. Items keep their
    /// order within a group.
    pub fn next_steps_by_owner(&self, you: Option<&str>, you_label: &str) -> Vec<(Option<String>, Vec<&ActionItem>)> {
        let you = you.map(str::trim).filter(|n| !n.is_empty() && *n != "Me");
        let you_label = you.unwrap_or(you_label);
        let mut groups: Vec<(Option<String>, Vec<&ActionItem>)> = vec![];
        for a in &self.action_items {
            let owner = a.owner.as_deref().map(str::trim).filter(|o| !o.is_empty());
            let owner = owner.map(|o| if o.eq_ignore_ascii_case("me") || you.is_some_and(|y| y.to_lowercase() == o.to_lowercase()) { you_label } else { o });
            match groups.iter_mut().find(|(g, _)| g.as_deref().map(str::to_lowercase) == owner.map(str::to_lowercase)) {
                Some((_, items)) => items.push(a),
                None => groups.push((owner.map(str::to_string), vec![a])),
            }
        }
        // The user first, nobody last (stable: the other owners keep the order they came in).
        groups.sort_by_key(|(o, _)| match o {
            None => 2,
            Some(o) if o == you_label => 0,
            Some(_) => 1,
        });
        groups
    }
}

impl ActionItem {
    /// The task for putting a due date or a time after it: without the trailing period or space the summary AI often
    /// ends it with ("Confirm the estimate." would read "estimate., due Thursday"). Untouched when nothing follows.
    pub fn text_before_suffix(&self) -> &str {
        let text = self.text.trim_end();
        if self.due.as_deref().is_some_and(|d| !d.trim().is_empty()) || self.at.is_some() {
            let cut = text.trim_end_matches(|c: char| c == '.' || c.is_whitespace());
            if !cut.is_empty() {
                return cut;
            }
        }
        text
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Takeaway {
    pub label: String,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SummaryTopic {
    pub id: String,
    pub title: String,
    pub bullets: Vec<SummaryBullet>,
}

/// A topic's bullet, with sub-bullets one level down.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SummaryBullet {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_bullets: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SummarySection {
    pub id: String,
    pub title: String,
    pub items: Vec<SummaryItem>,
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

/// Why a meeting's copy is still to be written.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingCopy {
    /// "waiting" (asked for, not written yet), "missing" (the folder wasn't there) or "failed" (writing failed).
    pub kind: String,
    /// For "failed": what went wrong (English, from the system).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A meeting's copy in the auto-save folder as Talaan last wrote it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportedCopy {
    pub path: String,
    /// SHA-256 of what was written, lowercase hex.
    pub sha256: String,
    pub size: u64,
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
    /// RFC 3339: when an imported file came into Talaan. `started_at` is when it was recorded, so retention ages the
    /// meeting from the later of the two. Absent on recorded meetings and on files from before this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_at: Option<String>,
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
    /// When the current step (transcribing or summarizing) should be done (`crate::eta`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eta: Option<Eta>,
    /// Why processing failed (status `failed`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ProcessingError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcription: Option<TranscriptionInfo>,
    /// The recording stopped saving part way (a full disk or a failed write); the meeting page says so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording_issue: Option<RecordingIssue>,
    /// Why there is no summary (meeting is `ready`, transcript-only): no engine set up, or the AI failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_error: Option<ProcessingError>,
    /// Keep this meeting transcript-only until the user asks for a summary (set from the recording controls).
    #[serde(default, skip_serializing_if = "is_false")]
    pub transcript_only: bool,
    /// The meeting type to summarize it as (`summarizer::templates`); none = the Settings default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// The language to write this meeting's summaries in (a `summary_languages` code), picked on the meeting; none = the
    /// Settings default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_language: Option<String>,
    /// "Transcribe again" with this speech model; cleared once the new transcript is saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retranscribe_model: Option<String>,
    /// Why the last "Transcribe again" didn't work; the previous transcript and summary were put back. Shown until
    /// dismissed or the next run starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retranscribe_error: Option<ProcessingError>,
    /// How many times a worker started on this meeting without ending the attempt cleanly (a crash or a kill leaves it
    /// above 0). A meeting that keeps crashing is stopped at launch instead of being retried for ever.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub attempts: u32,
    /// The copy written by "Also save each meeting to a folder" (`autosave.rs`): the only file Talaan ever replaces in
    /// that folder, and only while it still matches this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported_copy: Option<ExportedCopy>,
    /// A copy that is due but not written yet (`autosave.rs`): retried at launch, after the next copy that works, and
    /// with Try again in Check setup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autosave_pending: Option<PendingCopy>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// What went wrong while recording, for the meeting page.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecordingIssue {
    /// `write_failed`: the disk was full or couldn't be written to.
    pub kind: String,
    /// Where the audio stops, in ms into the recording.
    pub at_ms: u64,
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

/// When the step that's running should be done.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Eta {
    /// RFC 3339; the screens count down to it.
    pub finish_at: String,
    /// The work runs on this computer's processor (slow enough to say so).
    #[serde(default)]
    pub on_cpu: bool,
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
    /// The setting asked for the graphics card and the model could not use it, so the processor did the work.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gpu_fell_back: bool,
    /// The graphics card's name, when the model ran on one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
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
    /// While transcribing or summarizing: when that step should be done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eta: Option<Eta>,
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
    /// Why the last "Transcribe again" didn't work (the previous transcript and summary are back).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retranscribe_error: Option<ProcessingError>,
    /// How the transcript was made (the speech model), once there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcription: Option<TranscriptionInfo>,
    /// The recording stopped saving part way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording_issue: Option<RecordingIssue>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub transcript_only: bool,
    /// The meeting type chosen for this meeting, when one was (otherwise the Settings default applies).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// The summary language chosen for this meeting, when one was (otherwise the Settings default applies).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_language: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../fixtures/meeting.sample.json");

    #[test]
    fn a_summary_from_before_the_language_switch_still_loads_and_stays_without_one() {
        let old = r#"{"recap":"R.","decisions":[],"actionItems":[],"openQuestions":[],"engine":"x"}"#;
        let s: Summary = serde_json::from_str(old).unwrap();
        assert_eq!(s.language, None);
        assert!(!serde_json::to_string(&s).unwrap().contains("language"));
        let new: Summary = serde_json::from_str(r#"{"recap":"R.","decisions":[],"actionItems":[],"openQuestions":[],"engine":"x","language":"fil"}"#).unwrap();
        assert_eq!(new.language.as_deref(), Some("fil"));
        assert!(serde_json::to_string(&new).unwrap().contains(r#""language":"fil""#));
    }

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

    #[test]
    fn the_detailed_fields_are_optional_and_round_trip_in_camel_case() {
        let new = Summary {
            recap: "R.".into(),
            purpose: Some("P.".into()),
            takeaways: vec![Takeaway { label: "L".into(), text: "T.".into() }],
            topics: vec![SummaryTopic { id: "t0".into(), title: "Topic".into(), bullets: vec![SummaryBullet { text: "B.".into(), sub_bullets: vec!["S".into()] }, SummaryBullet { text: "C.".into(), sub_bullets: vec![] }] }],
            decisions: vec![],
            action_items: vec![],
            open_questions: vec![],
            sections: vec![],
            highlights: vec![],
            template: None,
            language: None,
            sections_missing: false,
            engine: "x".into(),
            edited: false,
        };
        let json = serde_json::to_string(&new).unwrap();
        assert!(json.contains("\"subBullets\":[\"S\"]") && json.contains("\"purpose\":\"P.\""));
        // A bullet without sub-bullets writes none.
        assert!(json.contains("{\"text\":\"C.\"}"));
        assert_eq!(serde_json::from_str::<Summary>(&json).unwrap(), new);
        // A summary from before the detailed layout has none of them.
        let old: Summary = serde_json::from_str(r#"{"recap":"R.","decisions":[],"actionItems":[],"openQuestions":[],"engine":"x"}"#).unwrap();
        assert!(old.purpose.is_none() && old.takeaways.is_empty() && old.topics.is_empty() && !old.is_detailed());
        assert!(!serde_json::to_string(&old).unwrap().contains("topics"));
        assert!(new.is_detailed());
    }

    fn shape<'a>(groups: &'a [(Option<String>, Vec<&ActionItem>)]) -> Vec<(Option<&'a str>, Vec<&'a str>)> {
        groups.iter().map(|(o, items)| (o.as_deref(), items.iter().map(|a| a.id.as_str()).collect())).collect()
    }

    fn summary_with(owners: &[(&str, Option<&str>)]) -> Summary {
        let mut s: Summary = serde_json::from_str(r#"{"recap":"R.","decisions":[],"actionItems":[],"openQuestions":[],"engine":"x"}"#).unwrap();
        s.action_items = owners.iter().map(|(id, owner)| ActionItem { id: (*id).into(), text: (*id).into(), at: None, owner: owner.map(String::from), due: None, done: false }).collect();
        s
    }

    #[test]
    fn next_steps_put_the_user_first_then_owners_in_order_of_first_appearance_with_nobody_last() {
        let mut s = summary_with(&[("a", None), ("b", Some("Dana")), ("c", Some("Me")), ("d", Some(" dana ")), ("e", Some("")), ("f", Some("Me"))]);
        // Your name empty: the group is "You".
        assert_eq!(shape(&s.next_steps_by_owner(None, "You")), vec![(Some("You"), vec!["c", "f"]), (Some("Dana"), vec!["b", "d"]), (None, vec!["a", "e"])]);
        s.action_items.clear();
        assert!(s.next_steps_by_owner(None, "You").is_empty());
    }

    #[test]
    fn next_steps_name_the_users_group_after_your_name_whether_the_summary_wrote_me_or_the_name() {
        let s = summary_with(&[("a", Some("Rosa")), ("b", Some("miguel")), ("c", None), ("d", Some("Me")), ("e", Some("Dana"))]);
        assert_eq!(shape(&s.next_steps_by_owner(Some(" Miguel "), "You")), vec![(Some("Miguel"), vec!["b", "d"]), (Some("Rosa"), vec!["a"]), (Some("Dana"), vec!["e"]), (None, vec!["c"])]);
        // The stored default "Me" is not a name.
        assert_eq!(shape(&s.next_steps_by_owner(Some("Me"), "You"))[0].0, Some("You"));
    }

    #[test]
    fn a_task_loses_its_trailing_period_only_when_something_goes_after_it() {
        let item = |text: &str, due: Option<&str>, at: Option<u64>| ActionItem { id: "a".into(), text: text.into(), at, owner: None, due: due.map(String::from), done: false };
        assert_eq!(item("Confirm the estimate. ", Some("Thursday"), None).text_before_suffix(), "Confirm the estimate");
        assert_eq!(item("Send it.", None, Some(5)).text_before_suffix(), "Send it");
        assert_eq!(item("Send it.", None, None).text_before_suffix(), "Send it.");
        assert_eq!(item("Send it.", Some("  "), None).text_before_suffix(), "Send it.");
        assert_eq!(item("...", Some("Fri"), None).text_before_suffix(), "...");
    }
}
