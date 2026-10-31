//! Talaan's summary prompt and output schema, the same for every engine. What shapes it: the meeting type
//! (`templates`), the user's standing instructions (Settings › Summary AI), the notes they typed and the moments
//! they marked. A General meeting with none of those gets exactly the original fixed prompt.

use super::templates::{self, Template};
use crate::storage::model::{Mark, Meeting, TranscriptLine};
use serde_json::{json, Value};

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

/// The schema for one meeting: the base one, plus `sections` for a meeting type that has them and `highlights`
/// (at most one per marked moment) when the user marked some. Every property stays required (OpenAI's strict mode
/// needs that).
pub fn schema(sections: bool, highlights: usize) -> String {
    if !sections && highlights == 0 {
        return SCHEMA.to_string();
    }
    let mut v: Value = serde_json::from_str(SCHEMA).expect("schema is valid JSON");
    let timed = json!({ "type": "object", "properties": { "text": { "type": "string" }, "time": { "type": "string" } }, "required": ["text", "time"], "additionalProperties": false });
    let mut required = vec![];
    if sections {
        v["properties"]["sections"] = json!({ "type": "array", "maxItems": 6, "items": { "type": "object", "properties": { "title": { "type": "string" }, "items": { "type": "array", "maxItems": 12, "items": timed } }, "required": ["title", "items"], "additionalProperties": false } });
        required.push("sections");
    }
    if highlights > 0 {
        v["properties"]["highlights"] = json!({ "type": "array", "maxItems": highlights.min(30), "items": { "type": "object", "properties": { "time": { "type": "string" }, "text": { "type": "string" } }, "required": ["time", "text"], "additionalProperties": false } });
        required.push("highlights");
    }
    if let Some(r) = v["required"].as_array_mut() {
        r.extend(required.into_iter().map(Value::from));
    }
    v.to_string()
}

const RULES: &str = "You write meeting summaries. You get the transcript of a call, recorded on the user's own computer: \"{me}\" is the user, everyone else is on the other end. Reply with JSON only, matching the schema.

- title: 2 to 6 words naming what the call was about. No date, no app name.
- recap: 3 to 5 plain sentences: what the call was about and where it landed.
- decisions: what was agreed or settled. One sentence each, with the time of the line where it was decided.
- action_items: concrete tasks someone committed to or was asked to do. owner: the person's name as written in the transcript (\"{me}\" for the user), empty if nobody. due: the deadline as said (\"Friday\", \"Oct 17\"), empty if none. time: the line where it came up.
- open_questions: questions raised and not answered, or topics left unresolved.
- Times are the [mm:ss] or [h:mm:ss] stamps of transcript lines. Copy them exactly; empty if unsure.
- Use only what is in the transcript. Never invent names, dates, numbers or facts. Empty lists are fine.
- Moments the user marked during the call matter most: make sure they are covered.";

/// How this summary is written: the meeting type and the user's standing instructions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Options {
    /// A `templates` id; unknown or empty means General.
    pub template: String,
    /// Settings › Summary AI › Custom instructions ("Always mention budget numbers").
    pub instructions: String,
}

/// Custom instructions longer than this are cut (the box in Settings stops there too).
pub const MAX_INSTRUCTIONS: usize = 1_000;

impl Options {
    pub fn template(&self) -> &'static Template {
        templates::find(&self.template)
    }

    fn instructions(&self) -> String {
        self.instructions.trim().chars().take(MAX_INSTRUCTIONS).collect()
    }
}

/// The user's notes: marks with text, typed during the call (timed) or added afterwards in the meeting view.
pub fn notes(meeting: &Meeting) -> Vec<&Mark> {
    meeting.marks.iter().filter(|m| m.note_text().is_some()).collect()
}

/// Moments the user marked without writing anything: each becomes a highlight.
pub fn moments(meeting: &Meeting) -> Vec<&Mark> {
    meeting.marks.iter().filter(|m| m.note_text().is_none() && !m.later).collect()
}

/// Half the window of transcript shown around a marked moment.
pub const AROUND_MS: u64 = 20_000;

/// The lines said within 20 s of a moment, at most 8 (the closest), in order.
pub fn around(lines: &[TranscriptLine], at: u64) -> Vec<&TranscriptLine> {
    let mut near: Vec<&TranscriptLine> = lines.iter().filter(|l| l.end + AROUND_MS >= at && l.at <= at + AROUND_MS && !l.text.trim().is_empty()).collect();
    if near.len() > 8 {
        near.sort_by_key(|l| l.at.abs_diff(at));
        near.truncate(8);
        near.sort_by_key(|l| l.at);
    }
    near
}

/// The instructions, naming the user the way the transcript does ("Me", or their name once they set one).
fn system(meeting: &Meeting, language: &str, opts: &Options) -> String {
    let mut s = RULES.replace("{me}", &speaker_name(meeting, "me"));
    let t = opts.template();
    if !t.is_general() {
        let titles = t.titles(language).iter().map(|x| format!("\"{x}\"")).collect::<Vec<_>>().join(", ");
        let translate = if Template::titles_localized(language) { String::new() } else { format!(" (translated into {})", language_name(language)) };
        s.push_str(&format!("\n- This call is {}. Fit the recap, decisions, action items and open questions to that.", t.focus));
        s.push_str(&format!(
            "\n- sections: exactly these {}, in this order, with these titles{translate}: {titles}. Each item is one short sentence with the time of its line. {} A section with nothing to report has no items.",
            t.sections.len(),
            t.section_rule
        ));
    }
    if !moments(meeting).is_empty() {
        s.push_str("\n- highlights: one for each marked moment listed before the transcript, in order. time: the marked time exactly as given. text: one or two sentences on what was being said or decided at that moment.");
    }
    if !notes(meeting).is_empty() {
        s.push_str("\n- The user's own notes are the backbone of the summary: keep every point they wrote, in their wording, put each where it belongs (recap, decisions, action items, open questions), and fill in the details from the transcript. When a note and the transcript disagree, follow the transcript.");
    }
    let instructions = opts.instructions();
    if !instructions.is_empty() {
        s.push_str(&format!(
            "\n- The user asks this of every summary. Follow it where the transcript allows, but never invent names, dates, numbers or facts to satisfy it:\n\"\"\"\n{instructions}\n\"\"\""
        ));
    }
    s.push_str(&format!("\n- Write everything in {}.", language_name(language)));
    if !instructions.is_empty() {
        // Said again last, after text the user wrote, so it is the rule the model read most recently.
        s.push_str("\n- Whatever the instructions above say: use only what is in the transcript and the notes, and never invent names, dates, numbers or facts.");
    }
    s
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

fn render_notes(notes: &[&Mark]) -> String {
    notes
        .iter()
        .map(|m| {
            let when = if m.later { "(after the call)".to_string() } else { format!("[{}]", stamp(m.at)) };
            format!("{when} {}", m.note_text().unwrap_or_default())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Characters of transcript shown around marked moments in all, however many there are.
pub const AROUND_BUDGET: usize = 6_000;

/// The marked moments. In a one-part call the transcript is all there, so only the times are listed; in a part of
/// a long call each comes with what was said around it (it may straddle the part's edge), within `AROUND_BUDGET`
/// for all of them together.
fn render_moments(meeting: &Meeting, moments: &[&Mark], with_context: bool) -> String {
    let mut out = String::new();
    let each = AROUND_BUDGET / moments.len().max(1);
    // So many marks that each would get under a line: the times alone (the part has the transcript anyway).
    let with_context = with_context && each >= 80;
    for m in moments {
        if !with_context {
            out.push_str(&format!("[{}] marked\n", stamp(m.at)));
            continue;
        }
        out.push_str(&format!("[{}] marked. Said around it:\n", stamp(m.at)));
        let mut used = 0;
        let mut near = around(&meeting.transcript, m.at);
        near.sort_by_key(|l| l.at.abs_diff(m.at));
        let mut kept: Vec<(u64, String)> = vec![];
        for l in near {
            let row = format!("  [{}] {}: {}", stamp(l.at), speaker_name(meeting, &l.speaker), l.text.trim());
            let room = each.saturating_sub(used);
            if room < 40 {
                break;
            }
            let row: String = if row.chars().count() > room { row.chars().take(room - 1).collect::<String>() + "…" } else { row };
            used += row.chars().count();
            kept.push((l.at, row));
        }
        // Back in spoken order, by time (the text "59:59" sorts after "1:00:00").
        kept.sort_by_key(|(at, _)| *at);
        for (_, row) in kept {
            out.push_str(&row);
            out.push('\n');
        }
    }
    out
}

/// The summary languages Talaan offers (code, English name used in the prompt). One JSON file read by this side
/// and by the screens (`src/lib/summaryLanguages.ts`), so the two lists can't drift apart.
const LANGUAGES_JSON: &str = include_str!("../../../src/lib/summaryLanguages.json");

fn languages() -> &'static [(String, String)] {
    static LIST: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| {
        let rows: Vec<Value> = serde_json::from_str(LANGUAGES_JSON).expect("summaryLanguages.json is valid");
        rows.iter().filter_map(|r| Some((r["code"].as_str()?.to_string(), r["name"].as_str()?.to_string()))).collect()
    })
}

/// The codes of the summary languages, in the order the screens list them.
pub fn summary_language_codes() -> Vec<&'static str> {
    languages().iter().map(|(c, _)| c.as_str()).collect()
}

/// Whether `code` is one of the languages a summary can be written in (what the language switch accepts).
pub fn is_summary_language(code: &str) -> bool {
    languages().iter().any(|(c, _)| c == code)
}

/// English name of a language code for the instruction ("es" → "Spanish"). Codes from older settings or from the
/// speech model's detection ("tl", "zh") keep working; any other unknown code passes through.
pub fn language_name(code: &str) -> String {
    if let Some((_, name)) = languages().iter().find(|(c, _)| c == code) {
        return name.clone();
    }
    match code {
        "tl" => "Filipino".into(),
        "zh" => "Chinese".into(),
        _ => code.to_string(),
    }
}

/// The prompt for one part of a call. `part` is (index from 1, count) when the call is split.
pub fn summary_prompt(meeting: &Meeting, lines: &[TranscriptLine], language: &str, part: Option<(usize, usize)>, opts: &Options) -> Prompt {
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
    let in_range = |at: u64| lines.first().is_none_or(|f| at + 2_000 >= f.at) && lines.last().is_none_or(|l| at <= l.end + 2_000);
    let notes: Vec<&Mark> = notes(meeting).into_iter().filter(|m| m.later || in_range(m.at)).collect();
    if !notes.is_empty() {
        user.push_str("\nThe user's own notes, with the time each was typed (or added after the call):\n");
        user.push_str(&render_notes(&notes));
        user.push('\n');
    }
    let moments: Vec<&Mark> = moments(meeting).into_iter().filter(|m| in_range(m.at)).collect();
    if !moments.is_empty() {
        user.push_str(if part.is_some() { "\nMoments the user marked during the call:\n" } else { "\nMoments the user marked during the call (what was said 20 s either side is in the transcript below):\n" });
        user.push_str(&render_moments(meeting, &moments, part.is_some()));
    }
    user.push_str("\nTranscript:\n");
    user.push_str(&render_lines(meeting, lines));
    Prompt { system: system(meeting, language, opts), user }
}

/// Combines the summaries of a long call's parts into one.
pub fn merge_prompt(meeting: &Meeting, parts_json: &[String], language: &str, opts: &Options) -> Prompt {
    let mut user = format!(
        "These are summaries of the {} consecutive parts of one call (length {}), in order. Combine them into one summary of the whole call: one recap for the whole call, a title for the whole call, decisions and action items merged (keep the earliest time when two say the same thing), open questions that a later part answered removed.\n",
        parts_json.len(),
        stamp(meeting.item.duration_ms)
    );
    if !opts.template().is_general() {
        user.push_str("Merge sections that have the same title, keeping the titles and their order.\n");
    }
    let moments = moments(meeting);
    if !moments.is_empty() {
        let times = moments.iter().map(|m| format!("[{}]", stamp(m.at))).collect::<Vec<_>>().join(", ");
        user.push_str(&format!("Keep exactly one highlight for each marked moment: {times}.\n"));
    }
    let notes = notes(meeting);
    if !notes.is_empty() {
        user.push_str("\nThe user's own notes, the backbone of the combined summary:\n");
        user.push_str(&render_notes(&notes));
        user.push('\n');
    }
    for (i, p) in parts_json.iter().enumerate() {
        user.push_str(&format!("\nPart {}:\n{}\n", i + 1, p));
    }
    Prompt { system: system(meeting, language, opts), user }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::storage::model::{MeetingListItem, MeetingStatus};
    use std::collections::BTreeMap;

    #[test]
    fn every_summary_language_has_its_english_name() {
        assert_eq!(summary_language_codes().len(), 32);
        assert_eq!(language_name("en"), "English");
        assert_eq!(language_name("fil"), "Filipino");
        assert_eq!(language_name("sv"), "Swedish");
        assert_eq!(language_name("he"), "Hebrew");
        assert_eq!(language_name("ur"), "Urdu");
        for code in summary_language_codes() {
            assert!(is_summary_language(code));
            assert_ne!(language_name(code), code, "{code} has no name");
        }
    }

    #[test]
    fn chinese_names_the_script_and_old_codes_still_work() {
        assert_eq!(language_name("zh-Hans"), "Chinese (Simplified)");
        assert_eq!(language_name("zh-Hant"), "Chinese (Traditional)");
        // What the speech model detects, and what older settings hold.
        assert_eq!((language_name("tl").as_str(), language_name("zh").as_str()), ("Filipino", "Chinese"));
        assert!(!is_summary_language("tl") && !is_summary_language("zh") && !is_summary_language("klingon") && !is_summary_language(""));
        // Unknown codes pass through.
        assert_eq!(language_name("xx"), "xx");
    }

    #[test]
    fn the_prompt_asks_for_the_chosen_language() {
        let m = meeting(vec![line("l0", 0, "me", "Hello there.")]);
        let p = summary_prompt(&m, &m.transcript, "fil", None, &Options::default());
        assert!(p.system.contains("Write everything in Filipino."));
    }

    pub fn line(id: &str, at: u64, speaker: &str, text: &str) -> TranscriptLine {
        TranscriptLine { id: id.into(), at, end: at + 4_000, speaker: speaker.into(), text: text.into(), edited: false }
    }

    pub fn mark(id: &str, at: u64, note: Option<&str>) -> Mark {
        Mark { id: id.into(), at, note: note.map(String::from), later: false }
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
            marks: vec![mark("k0", 65_000, Some("pricing decision"))],
            pauses: vec![],
            error: None,
            summary_error: None,
            transcription: None,
            transcript_only: false,
            template: None,
        }
    }

    fn general() -> Options {
        Options::default()
    }

    #[test]
    fn calendar_attendees_are_offered_as_owner_names() {
        let mut m = meeting(vec![line("l0", 0, "them:0", "I'll send it.")]);
        m.calendar_event = Some(crate::storage::model::CalendarEvent { title: "Q4 sync".into(), attendees: vec!["Priya Shah".into(), "Dana Reyes".into()], emails: vec![] });
        let p = summary_prompt(&m, &m.transcript, "en", None, &general());
        assert!(p.user.contains("Calendar event: Q4 sync. Invited: Priya Shah, Dana Reyes."));
        m.calendar_event = None;
        assert!(!summary_prompt(&m, &m.transcript, "en", None, &general()).user.contains("Calendar event"));
    }

    #[test]
    fn stamps_match_the_transcript_format() {
        assert_eq!(stamp(0), "00:00");
        assert_eq!(stamp(65_400), "01:05");
        assert_eq!(stamp(3_723_000), "1:02:03");
    }

    #[test]
    fn the_prompt_has_names_notes_times_and_language() {
        let lines = vec![line("l0", 60_000, "them:1", "Pricing moves to Q1."), line("l1", 66_000, "me", "Agreed.")];
        let m = meeting(lines.clone());
        let p = summary_prompt(&m, &lines, "es", None, &general());
        assert!(p.user.contains("[01:00] Dana Reyes: Pricing moves to Q1."));
        assert!(p.user.contains("[01:06] Me: Agreed."));
        assert!(p.user.contains("The user's own notes, with the time each was typed (or added after the call):\n[01:05] pricing decision"));
        assert!(p.system.contains("backbone of the summary"));
        assert!(p.system.contains("Write everything in Spanish."));
        assert!(!p.user.contains("part"));
    }

    #[test]
    fn a_general_meeting_without_notes_or_marks_gets_the_original_prompt() {
        let lines = vec![line("l0", 60_000, "them:1", "Pricing moves to Q1."), line("l1", 66_000, "me", "Agreed.")];
        let mut m = meeting(lines.clone());
        m.marks.clear();
        let p = summary_prompt(&m, &lines, "en", None, &general());
        assert_eq!(p.system, format!("{}\n- Write everything in English.", RULES.replace("{me}", "Me")));
        assert!(!p.user.contains("notes") && !p.user.contains("marked"));
        assert_eq!(schema(false, 0), SCHEMA);
        // An unknown type is General too.
        let odd = Options { template: "from_the_future".into(), instructions: "  ".into() };
        assert_eq!(summary_prompt(&m, &lines, "en", None, &odd), p);
    }

    #[test]
    fn marked_moments_come_with_what_was_said_around_them() {
        let lines = vec![
            line("l0", 10_000, "me", "Long before."),
            line("l1", 50_000, "them:1", "So the budget is fifty thousand."),
            line("l2", 62_000, "me", "That works for us."),
            line("l3", 100_000, "me", "Much later."),
        ];
        let mut m = meeting(lines.clone());
        m.marks = vec![mark("k0", 60_000, None)];
        // A part of a long call shows what was said around the moment.
        let p = summary_prompt(&m, &lines, "en", Some((1, 2)), &general());
        assert!(p.user.contains("Moments the user marked during the call:\n[01:00] marked. Said around it:\n  [00:50] Dana Reyes: So the budget is fifty thousand.\n  [01:02] Me: That works for us.\n"));
        assert!(!p.user.contains("  [00:10]") && !p.user.contains("  [01:40]"));
        assert!(p.system.contains("highlights: one for each marked moment"));
        assert!(!p.system.contains("backbone"));
        // A one-part call has the whole transcript already: just the times.
        let one = summary_prompt(&m, &lines, "en", None, &general());
        assert!(one.user.contains("[01:00] marked\n"));
        assert!(!one.user.contains("Said around it"));
        let s: Value = serde_json::from_str(&schema(false, 1)).unwrap();
        assert!(s["required"].as_array().unwrap().contains(&json!("highlights")));
        assert_eq!(s["properties"]["highlights"]["maxItems"], 1);
        assert!(s["properties"].get("sections").is_none());
    }

    #[test]
    fn lines_around_a_moment_stay_in_spoken_order_across_the_hour() {
        let lines = vec![line("l0", 3_590_000, "me", "Just before the hour."), line("l1", 3_605_000, "them:1", "Just after it.")];
        let mut m = meeting(lines.clone());
        m.marks = vec![mark("k0", 3_600_000, None)];
        let p = summary_prompt(&m, &lines, "en", Some((2, 2)), &general());
        let before = p.user.find("  [59:50] Me: Just before the hour.").unwrap();
        let after = p.user.find("  [1:00:05] Dana Reyes: Just after it.").unwrap();
        assert!(before < after);
    }

    #[test]
    fn many_marks_keep_their_context_within_one_budget() {
        let lines: Vec<_> = (0..400).map(|i| line(&format!("l{i}"), i * 5_000, "me", &"A long sentence about the plan and the budget. ".repeat(6))).collect();
        let mut m = meeting(lines.clone());
        m.marks = (0..100).map(|i| mark(&format!("k{i}"), i * 20_000, None)).collect();
        let p = summary_prompt(&m, &[], "en", Some((1, 1)), &general());
        let context = p.user.len();
        assert!(context < AROUND_BUDGET + 100 * 40, "{context}");
        assert_eq!(p.user.matches("] marked
").count(), 100);
        // Fewer marks share the same budget, each with some of what was said.
        m.marks.truncate(20);
        let p = summary_prompt(&m, &[], "en", Some((1, 1)), &general());
        assert_eq!(p.user.matches(" marked. Said around it:").count(), 20);
        assert!(p.user.len() < AROUND_BUDGET + 20 * 60, "{}", p.user.len());
    }

    #[test]
    fn notes_added_after_the_call_go_to_every_part() {
        let lines = vec![line("l5", 600_000, "me", "Next topic.")];
        let mut m = meeting(lines.clone());
        m.marks.push(Mark { id: "n1".into(), at: 0, note: Some("Ask Dana about the budget".into()), later: true });
        let p = summary_prompt(&m, &lines, "en", Some((2, 3)), &general());
        assert!(p.user.contains("(after the call) Ask Dana about the budget"));
        assert!(!p.user.contains("pricing decision"));
        let merged = merge_prompt(&m, &["{}".into()], "en", &general());
        assert!(merged.user.contains("[01:05] pricing decision\n(after the call) Ask Dana about the budget"));
    }

    #[test]
    fn a_meeting_type_sets_the_focus_and_section_titles_in_the_summary_language() {
        let lines = vec![line("l0", 0, "me", "Yesterday I fixed the export.")];
        let m = meeting(lines.clone());
        let standup = Options { template: "standup".into(), instructions: String::new() };
        let p = summary_prompt(&m, &lines, "fr", None, &standup);
        assert!(p.system.contains("This call is a team stand-up"));
        assert!(p.system.contains("with these titles: \"Hier\", \"Aujourd’hui\", \"Blocages\""));
        assert!(p.system.contains("Start every item with the person's name"));
        let de = summary_prompt(&m, &lines, "de", None, &standup);
        assert!(de.system.contains("with these titles (translated into German): \"Yesterday\", \"Today\", \"Blockers\""));
        let s: Value = serde_json::from_str(&schema(true, 0)).unwrap();
        assert!(s["required"].as_array().unwrap().contains(&json!("sections")));
        assert!(merge_prompt(&m, &["{}".into()], "fr", &standup).user.contains("Merge sections that have the same title"));
    }

    #[test]
    fn custom_instructions_are_followed_without_inventing_facts() {
        let lines = vec![line("l0", 0, "me", "Hi.")];
        let m = meeting(lines.clone());
        let o = Options { template: "general".into(), instructions: "  Always mention budget numbers.  ".into() };
        let p = summary_prompt(&m, &lines, "en", None, &o);
        assert!(p.system.contains("never invent names, dates, numbers or facts to satisfy it:\n\"\"\"\nAlways mention budget numbers.\n\"\"\""));
        assert!(p.system.ends_with("never invent names, dates, numbers or facts."));
        assert!(p.system.contains("- Write everything in English.\n- Whatever the instructions above say"));
        let long = Options { instructions: "x".repeat(5_000), ..o };
        assert!(summary_prompt(&m, &lines, "en", None, &long).system.matches('x').count() <= MAX_INSTRUCTIONS + 10);
    }

    #[test]
    fn the_user_is_named_the_way_the_transcript_names_them() {
        let lines = vec![line("l0", 0, "me", "I'll send the deck.")];
        let mut m = meeting(lines.clone());
        let p = summary_prompt(&m, &lines, "en", None, &general());
        assert!(p.system.contains("\"Me\" is the user") && p.system.contains("(\"Me\" for the user)"));
        m.speakers.insert("me".into(), "Miguel".into());
        let p = summary_prompt(&m, &lines, "en", None, &general());
        assert!(p.user.contains("[00:00] Miguel: I'll send the deck."));
        assert!(p.system.contains("\"Miguel\" is the user") && p.system.contains("(\"Miguel\" for the user)"));
        assert!(!p.system.contains("{me}"));
        assert!(merge_prompt(&m, &["{}".into()], "en", &general()).system.contains("\"Miguel\" is the user"));
    }

    #[test]
    fn a_part_says_which_part_and_keeps_only_its_notes() {
        let lines = vec![line("l5", 600_000, "me", "Next topic.")];
        let p = summary_prompt(&meeting(lines.clone()), &lines, "en", Some((2, 3)), &general());
        assert!(p.user.contains("This is part 2 of 3 of a long call, from 10:00 to 10:04."));
        assert!(!p.user.contains("pricing decision"));
    }

    #[test]
    fn imported_recordings_explain_the_single_track() {
        let lines = vec![line("l0", 0, "them:0", "I will send the update Monday.")];
        let mut m = meeting(lines.clone());
        m.item.app = "import".into();
        assert!(summary_prompt(&m, &lines, "en", None, &general()).user.contains("\"Them\" is not a name"));
        m.item.app = "zoom".into();
        assert!(!summary_prompt(&m, &lines, "en", None, &general()).user.contains("single track"));
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
        let both: serde_json::Value = serde_json::from_str(&schema(true, 3)).unwrap();
        assert_eq!(both["required"].as_array().unwrap().len(), 7);
    }
}
