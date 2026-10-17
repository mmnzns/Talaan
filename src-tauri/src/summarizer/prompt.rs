//! Talaan's one summary prompt and output schema, the same for every engine (no user prompt editing).

use crate::storage::model::{Mark, Meeting, TranscriptLine};

/// JSON schema every engine's reply must match. Times are "mm:ss" (or "h:mm:ss") from the transcript,
/// empty when unknown; owner and due are empty when the call didn't say.
pub const SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "title": { "type": "string" },
    "recap": { "type": "string" },
    "decisions": { "type": "array", "maxItems": 10, "items": { "type": "object", "properties": { "text": { "type": "string" }, "time": { "type": "string" } }, "required": ["text", "time"], "additionalProperties": false } },
    "action_items": { "type": "array", "maxItems": 15, "items": { "type": "object", "properties": { "text": { "type": "string" }, "owner": { "type": "string" }, "due": { "type": "string" }, "time": { "type": "string" } }, "required": ["text", "owner", "due", "time"], "additionalProperties": false } },
    "open_questions": { "type": "array", "maxItems": 8, "items": { "type": "object", "properties": { "text": { "type": "string" }, "time": { "type": "string" } }, "required": ["text", "time"], "additionalProperties": false } }
  },
  "required": ["title", "recap", "decisions", "action_items", "open_questions"],
  "additionalProperties": false
}"#;

const RULES: &str = "You write meeting summaries. You get the transcript of a call, recorded on the user's own computer: \"{me}\" is the user, everyone else is on the other end. Reply with JSON only, matching the schema.

- title: 2 to 6 words naming what the call was about. No date, no app name.
- recap: 3 to 5 plain sentences: what the call was about and where it landed.
- decisions: what was agreed or settled. One sentence each, with the time of the line where it was decided.
- action_items: concrete tasks someone committed to or was asked to do. owner: the person's name as written in the transcript (\"{me}\" for the user), empty if nobody. due: the deadline as said (\"Friday\", \"Oct 17\"), empty if none. time: the line where it came up.
- open_questions: questions raised and not answered, or topics left unresolved.
- Times are the [mm:ss] or [h:mm:ss] stamps of transcript lines. Copy them exactly; empty if unsure.
- Use only what is in the transcript. Never invent names, dates, numbers or facts. Empty lists are fine.
- Moments the user marked during the call matter most: make sure they are covered.";

/// The instructions, naming the user the way the transcript does ("Me", or their name once they set one).
fn system(meeting: &Meeting, language: &str) -> String {
    format!("{}\n- Write everything in {}.", RULES.replace("{me}", &speaker_name(meeting, "me")), language_name(language))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    pub system: String,
    pub user: String,
}

/// "12:34" or "1:02:03" (hours only when needed), the same format the prompt shows.
pub fn stamp(ms: u64) -> String {
    let s = ms / 1000;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m:02}:{s:02}") }
}

pub fn speaker_name(meeting: &Meeting, speaker: &str) -> String {
    if let Some(name) = meeting.speakers.get(speaker).filter(|n| !n.trim().is_empty()) {
        return name.clone();
    }
    match speaker {
        "me" => "Me".into(),
        "them:0" => "Them".into(),
        s => s.strip_prefix("them:").map(|n| format!("Speaker {n}")).unwrap_or_else(|| s.to_string()),
    }
}

pub fn render_lines(meeting: &Meeting, lines: &[TranscriptLine]) -> String {
    lines.iter().map(|l| format!("[{}] {}: {}", stamp(l.at), speaker_name(meeting, &l.speaker), l.text.trim())).collect::<Vec<_>>().join("\n")
}

fn render_marks(marks: &[Mark]) -> String {
    marks.iter().map(|m| format!("[{}] {}", stamp(m.at), m.note.as_deref().filter(|n| !n.trim().is_empty()).unwrap_or("(marked)"))).collect::<Vec<_>>().join("\n")
}

/// English name of a language code for the instruction ("es" → "Spanish"); unknown codes pass through.
pub fn language_name(code: &str) -> String {
    let name = match code {
        "en" => "English",
        "es" => "Spanish",
        "tl" | "fil" => "Filipino",
        "fr" => "French",
        "de" => "German",
        "pt" => "Portuguese",
        "it" => "Italian",
        "nl" => "Dutch",
        "ja" => "Japanese",
        "zh" => "Chinese",
        "ko" => "Korean",
        _ => return code.to_string(),
    };
    name.into()
}

/// The prompt for one part of a call. `part` is (index from 1, count) when the call is split.
pub fn summary_prompt(meeting: &Meeting, lines: &[TranscriptLine], language: &str, part: Option<(usize, usize)>) -> Prompt {
    let mut user = String::new();
    user.push_str(&format!("Call recorded {}. Length {}.\n", meeting.item.started_at.get(..10).unwrap_or(""), stamp(meeting.item.duration_ms)));
    if meeting.item.app == "import" {
        // One track, so everyone (the user included) is labeled Them.
        user.push_str("This is an imported recording on a single track: every line says Them, including the person who made it. \"Them\" is not a name: use a person's name as owner only when it is said, otherwise leave owner empty, and don't call anyone \"the speaker\".\n");
    }
    if let Some(ev) = &meeting.calendar_event {
        user.push_str(&format!("Calendar event: {}.", ev.title.trim()));
        if !ev.attendees.is_empty() {
            user.push_str(&format!(" Invited: {}. Use these names for owners when the transcript makes clear who it is.", ev.attendees.join(", ")));
        }
        user.push('\n');
    }
    if let (Some((i, n)), Some(first), Some(last)) = (part, lines.first(), lines.last()) {
        user.push_str(&format!("This is part {i} of {n} of a long call, from {} to {}. Summarize only this part.\n", stamp(first.at), stamp(last.end)));
    }
    let marks: Vec<Mark> = meeting.marks.iter().filter(|m| lines.first().is_none_or(|f| m.at + 2_000 >= f.at) && lines.last().is_none_or(|l| m.at <= l.end + 2_000)).cloned().collect();
    if !marks.is_empty() {
        user.push_str("\nMoments the user marked during the call:\n");
        user.push_str(&render_marks(&marks));
        user.push('\n');
    }
    user.push_str("\nTranscript:\n");
    user.push_str(&render_lines(meeting, lines));
    Prompt { system: system(meeting, language), user }
}

/// Combines the summaries of a long call's parts into one.
pub fn merge_prompt(meeting: &Meeting, parts_json: &[String], language: &str) -> Prompt {
    let mut user = format!(
        "These are summaries of the {} consecutive parts of one call (length {}), in order. Combine them into one summary of the whole call: one recap for the whole call, a title for the whole call, decisions and action items merged (keep the earliest time when two say the same thing), open questions that a later part answered removed.\n",
        parts_json.len(),
        stamp(meeting.item.duration_ms)
    );
    for (i, p) in parts_json.iter().enumerate() {
        user.push_str(&format!("\nPart {}:\n{}\n", i + 1, p));
    }
    Prompt { system: system(meeting, language), user }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::storage::model::{MeetingListItem, MeetingStatus};
    use std::collections::BTreeMap;

    pub fn line(id: &str, at: u64, speaker: &str, text: &str) -> TranscriptLine {
        TranscriptLine { id: id.into(), at, end: at + 4_000, speaker: speaker.into(), text: text.into(), edited: false }
    }

    pub fn meeting(lines: Vec<TranscriptLine>) -> Meeting {
        Meeting {
            item: MeetingListItem {
                id: "m".into(),
                title: "Call, Zoom, Oct 14, 2:30 PM".into(),
                app: "zoom".into(),
                app_label: "Zoom".into(),
                started_at: "2026-10-14T14:30:00-07:00".into(),
                duration_ms: 1_800_000,
                status: MeetingStatus::Summarizing,
                progress: None,
                eta: None,
                folder_id: None,
                tags: vec![],
                people_count: 0,
                pinned: false,
            },
            language: "en".into(),
            has_audio: true,
            audio_kept_until: None,
            calendar_event: None,
            speakers: BTreeMap::from([("them:1".to_string(), "Dana Reyes".to_string())]),
            summary: None,
            transcript: lines,
            marks: vec![Mark { id: "k0".into(), at: 65_000, note: Some("pricing decision".into()) }],
            pauses: vec![],
            error: None,
            summary_error: None,
            transcription: None,
            transcript_only: false,
        }
    }

    #[test]
    fn calendar_attendees_are_offered_as_owner_names() {
        let mut m = meeting(vec![line("l0", 0, "them:0", "I'll send it.")]);
        m.calendar_event = Some(crate::storage::model::CalendarEvent { title: "Q4 sync".into(), attendees: vec!["Priya Shah".into(), "Dana Reyes".into()], emails: vec![] });
        let p = summary_prompt(&m, &m.transcript, "en", None);
        assert!(p.user.contains("Calendar event: Q4 sync. Invited: Priya Shah, Dana Reyes."));
        m.calendar_event = None;
        assert!(!summary_prompt(&m, &m.transcript, "en", None).user.contains("Calendar event"));
    }

    #[test]
    fn stamps_match_the_transcript_format() {
        assert_eq!(stamp(0), "00:00");
        assert_eq!(stamp(65_400), "01:05");
        assert_eq!(stamp(3_723_000), "1:02:03");
    }

    #[test]
    fn the_prompt_has_names_marks_times_and_language() {
        let lines = vec![line("l0", 60_000, "them:1", "Pricing moves to Q1."), line("l1", 66_000, "me", "Agreed.")];
        let m = meeting(lines.clone());
        let p = summary_prompt(&m, &lines, "es", None);
        assert!(p.user.contains("[01:00] Dana Reyes: Pricing moves to Q1."));
        assert!(p.user.contains("[01:06] Me: Agreed."));
        assert!(p.user.contains("Moments the user marked during the call:\n[01:05] pricing decision"));
        assert!(p.system.contains("Write everything in Spanish."));
        assert!(!p.user.contains("part"));
    }

    #[test]
    fn the_user_is_named_the_way_the_transcript_names_them() {
        let lines = vec![line("l0", 0, "me", "I'll send the deck.")];
        let mut m = meeting(lines.clone());
        let p = summary_prompt(&m, &lines, "en", None);
        assert!(p.system.contains("\"Me\" is the user") && p.system.contains("(\"Me\" for the user)"));
        m.speakers.insert("me".into(), "Miguel".into());
        let p = summary_prompt(&m, &lines, "en", None);
        assert!(p.user.contains("[00:00] Miguel: I'll send the deck."));
        assert!(p.system.contains("\"Miguel\" is the user") && p.system.contains("(\"Miguel\" for the user)"));
        assert!(!p.system.contains("{me}"));
        assert!(merge_prompt(&m, &["{}".into()], "en").system.contains("\"Miguel\" is the user"));
    }

    #[test]
    fn a_part_says_which_part_and_keeps_only_its_marks() {
        let lines = vec![line("l5", 600_000, "me", "Next topic.")];
        let p = summary_prompt(&meeting(lines.clone()), &lines, "en", Some((2, 3)));
        assert!(p.user.contains("This is part 2 of 3 of a long call, from 10:00 to 10:04."));
        assert!(!p.user.contains("pricing decision"));
    }

    #[test]
    fn imported_recordings_explain_the_single_track() {
        let lines = vec![line("l0", 0, "them:0", "I will send the update Monday.")];
        let mut m = meeting(lines.clone());
        m.item.app = "import".into();
        assert!(summary_prompt(&m, &lines, "en", None).user.contains("\"Them\" is not a name"));
        m.item.app = "zoom".into();
        assert!(!summary_prompt(&m, &lines, "en", None).user.contains("single track"));
    }

    #[test]
    fn unnamed_speakers_get_readable_names() {
        let m = meeting(vec![]);
        assert_eq!(speaker_name(&m, "them:0"), "Them");
        assert_eq!(speaker_name(&m, "them:2"), "Speaker 2");
        assert_eq!(speaker_name(&m, "them:1"), "Dana Reyes");
    }

    #[test]
    fn the_schema_is_valid_json() {
        let v: serde_json::Value = serde_json::from_str(SCHEMA).unwrap();
        assert_eq!(v["required"].as_array().unwrap().len(), 5);
    }
}
