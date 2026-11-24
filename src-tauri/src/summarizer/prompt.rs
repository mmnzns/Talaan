//! Talaan's summary prompt and output schema, the same for every engine. What shapes it: the meeting type
//! (`templates`), the user's standing instructions (Settings › Summary AI), the notes they typed and the moments
//! they marked. A General meeting with none of those gets exactly the original fixed prompt.

use super::templates::{self, Template};
use crate::storage::model::{Mark, Meeting, TranscriptLine};
use serde_json::Value;

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

const TIMED_ITEM: &str = r#"{ "type": "object", "properties": { "text": { "type": "string" }, "time": { "type": "string" } }, "required": ["text", "time"], "additionalProperties": false }"#;

/// How much the detailed layout may hold: the full version, or the compact one for a small context (an 8 GB
/// computer's 4096 tokens) that has to fit the reply with room to spare.
struct Limits {
    takeaways: usize,
    topics: usize,
    bullets: usize,
    sub_bullets: usize,
    decisions: usize,
    actions: usize,
    questions: usize,
}

const FULL: Limits = Limits { takeaways: 6, topics: 8, bullets: 8, sub_bullets: 4, decisions: 10, actions: 15, questions: 8 };
const COMPACT: Limits = Limits { takeaways: 3, topics: 3, bullets: 3, sub_bullets: 2, decisions: 4, actions: 6, questions: 3 };

/// The schema for one meeting. `detailed` (the General layout) swaps the recap for a purpose, takeaways and topics
/// (the recap is then written from them); `compact` holds those lists to what a small context can carry. `sections`
/// adds a meeting type's parts and `highlights` one per marked moment (at most that many). Properties are listed in
/// the order the reply should write them, and every one stays required (OpenAI's strict mode needs that).
pub fn schema(detailed: bool, compact: bool, sections: bool, highlights: usize) -> String {
    if !detailed && !sections && highlights == 0 {
        return SCHEMA.to_string();
    }
    let lim = if compact && detailed { COMPACT } else { FULL };
    let text = r#"{ "type": "string" }"#.to_string();
    let mut props: Vec<(&str, String)> = vec![("title", text.clone())];
    if detailed {
        props.push(("purpose", text.clone()));
        props.push((
            "takeaways",
            format!(
                r#"{{ "type": "array", "maxItems": {}, "items": {{ "type": "object", "properties": {{ "label": {{ "type": "string" }}, "text": {{ "type": "string" }} }}, "required": ["label", "text"], "additionalProperties": false }} }}"#,
                lim.takeaways
            ),
        ));
        props.push((
            "topics",
            format!(
                r#"{{ "type": "array", "maxItems": {}, "items": {{ "type": "object", "properties": {{ "title": {{ "type": "string" }}, "bullets": {{ "type": "array", "maxItems": {}, "items": {{ "type": "object", "properties": {{ "text": {{ "type": "string" }}, "sub_bullets": {{ "type": "array", "maxItems": {}, "items": {{ "type": "string" }} }} }}, "required": ["text", "sub_bullets"], "additionalProperties": false }} }} }}, "required": ["title", "bullets"], "additionalProperties": false }} }}"#,
                lim.topics, lim.bullets, lim.sub_bullets
            ),
        ));
    } else {
        props.push(("recap", text));
    }
    let (decisions, actions, questions) = (lim.decisions, lim.actions, lim.questions);
    props.push(("decisions", format!(r#"{{ "type": "array", "maxItems": {decisions}, "items": {TIMED_ITEM} }}"#)));
    props.push((
        "action_items",
        format!(
            r#"{{ "type": "array", "maxItems": {actions}, "items": {{ "type": "object", "properties": {{ "text": {{ "type": "string" }}, "owner": {{ "type": "string" }}, "due": {{ "type": "string" }}, "time": {{ "type": "string" }} }}, "required": ["text", "owner", "due", "time"], "additionalProperties": false }} }}"#
        ),
    ));
    props.push(("open_questions", format!(r#"{{ "type": "array", "maxItems": {questions}, "items": {TIMED_ITEM} }}"#)));
    if sections {
        props.push((
            "sections",
            format!(
                r#"{{ "type": "array", "maxItems": 6, "items": {{ "type": "object", "properties": {{ "title": {{ "type": "string" }}, "items": {{ "type": "array", "maxItems": 12, "items": {TIMED_ITEM} }} }}, "required": ["title", "items"], "additionalProperties": false }} }}"#
            ),
        ));
    }
    if highlights > 0 {
        props.push((
            "highlights",
            format!(
                r#"{{ "type": "array", "maxItems": {}, "items": {{ "type": "object", "properties": {{ "time": {{ "type": "string" }}, "text": {{ "type": "string" }} }}, "required": ["time", "text"], "additionalProperties": false }} }}"#,
                highlights.min(30)
            ),
        ));
    }
    let body = props.iter().map(|(k, v)| format!("\"{k}\": {v}")).collect::<Vec<_>>().join(", ");
    let required = props.iter().map(|(k, _)| format!("\"{k}\"")).collect::<Vec<_>>().join(", ");
    format!(r#"{{ "type": "object", "properties": {{ {body} }}, "required": [{required}], "additionalProperties": false }}"#)
}

const RULES: &str = "You write meeting summaries. You get the transcript of a call, recorded on the user's own computer: \"{me}\" is the user, everyone else is on the other end. Reply with JSON only, matching the schema.

- title: 2 to 6 words naming what the call was about. No date, no app name.
- recap: 3 to 5 plain sentences: what the call was about and where it landed.
- decisions: what was agreed or settled. One sentence each, with the time of the line where it was decided.
- action_items: concrete tasks someone committed to or was asked to do. owner: the person's name (\"{owner_me}\" for the user), empty if nobody. due: the deadline the call gave (\"Friday\", \"Oct 17\"), written in the summary's language like the rest, empty if none. time: the line where it came up.
- open_questions: questions raised and not answered, or topics left unresolved.
- Times are the [mm:ss] or [h:mm:ss] stamps of transcript lines. Copy them exactly; empty if unsure.
- Use only what is in the transcript. Never invent names, dates, numbers or facts. Empty lists are fine.
- Moments the user marked during the call matter most: make sure they are covered.";

/// The General meeting's rules: a detailed summary in the order Purpose, Key takeaways, Topics, Decisions, Next steps,
/// Open questions. Talaan writes the recap from the purpose and the takeaways, so the reply has none.
const DETAILED_RULES: &str = "You write detailed meeting summaries. You get the transcript of a call, recorded on the user's own computer: \"{me}\" is the user, everyone else is on the other end. Reply with JSON only, matching the schema.

- title: 2 to 6 words naming what the call was about. No date, no app name.
- purpose: one sentence on why the meeting happened.
- takeaways: the points worth remembering, {takeaways}. label: 2 to 4 words naming the point (\"Lending automation\"). text: one sentence on what was said or settled.
- topics: one entry for each main subject discussed, in the order it came up. title: a short name for the subject. bullets: the detail, as concrete facts: numbers, dates, names of systems and products, the reason behind a choice, what was ruled out. A bullet may have sub_bullets (one level, never deeper) for its parts, steps or examples; otherwise leave sub_bullets empty. {topic_size}
- decisions: what was agreed or settled. One sentence each, with the time of the line where it was decided.
- action_items: the next steps: concrete tasks someone committed to or was asked to do. text: the task, as a full sentence. owner: the person's name (\"{owner_me}\" for the user), empty if nobody. due: the deadline the call gave (\"Friday\", \"Oct 17\"), written in the summary's language like the rest, empty if none. time: the line where it came up.
- open_questions: questions raised and not answered, or topics left unresolved.
- Times are the [mm:ss] or [h:mm:ss] stamps of transcript lines. Copy them exactly; empty if unsure.
- Use only what is in the transcript. Never invent names, dates, numbers or facts. Empty lists are fine.
- The length follows the call: a short call has one topic and one or two takeaways. Never pad, and never say the same thing in two places.
- Moments the user marked during the call matter most: make sure they are covered.";

/// How people are named, added to every meeting type's rules.
const NAMES_RULE: &str = "- Names: use real names. The transcript labels people with a name the user set, or with \"Them\" / \"Speaker 1\" / \"Speaker 2\" (voices told apart automatically, not names). Work out who is who from the conversation itself (people addressing each other, introductions such as \"this is Kevin\") and from the invited names when the call has them, then write the real name everywhere, in the text and as owner. A name the user set in the transcript always wins. {me_rule} When a person's name never comes up and is not known, use a neutral role if the context makes it clear (\"the recruiter\", \"the client\"), otherwise keep the transcript label (\"Speaker 1\"). Never invent a name.";

const ME_NAMED: &str = "The user is \"{me}\": write that name for them, never \"Me\".";
const ME_UNNAMED: &str = "The user is labeled \"Me\" in the transcript, but \"Me\" is not a name: never write it. Write to the user in the second person (\"You will send the plan\", \"You asked about pricing\") and use \"You\" (in the summary's language) as owner.";

/// How this summary is written: the meeting type and the user's standing instructions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Options {
    /// A `templates` id; unknown or empty means General.
    pub template: String,
    /// Settings › Summary AI › Custom instructions ("Always mention budget numbers").
    pub instructions: String,
    /// Ask for the compact detailed layout (fewer topics and bullets): set for a small context.
    pub compact: bool,
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

/// The name the user goes by in this meeting (Your name, or one they gave themselves), none when the transcript
/// still says "Me".
pub fn user_name(meeting: &Meeting) -> Option<String> {
    meeting.speakers.get("me").map(|n| n.trim()).filter(|n| !n.is_empty() && *n != "Me").map(str::to_string)
}

/// Whether this meeting gets the detailed layout: the General type does; the others keep their own sections.
pub fn is_detailed(opts: &Options) -> bool {
    opts.template().is_general()
}

/// The instructions, naming the user the way the transcript does ("Me", or their name once they set one).
fn system(meeting: &Meeting, language: &str, opts: &Options) -> String {
    let name = user_name(meeting);
    let me = speaker_name(meeting, "me");
    let detailed = is_detailed(opts);
    let mut s = if detailed { DETAILED_RULES } else { RULES }.replace("{me}", &me).replace("{owner_me}", name.as_deref().unwrap_or("You"));
    if detailed {
        let (takeaways, size) = if opts.compact {
            ("at most 3", "Keep it compact: at most 3 topics with at most 3 short bullets each, and at most 2 sub_bullets for a bullet.")
        } else {
            ("3 to 6", "A short bullet is better than none, but a topic with one thin bullet should be folded into another topic.")
        };
        s = s.replace("{takeaways}", takeaways).replace("{topic_size}", size);
    }
    let me_rule = match &name {
        Some(n) => ME_NAMED.replace("{me}", n),
        None => ME_UNNAMED.to_string(),
    };
    s.push('\n');
    s.push_str(&NAMES_RULE.replace("{me_rule}", &me_rule));
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
        let places = if detailed { "purpose, takeaways, topics, decisions, action items, open questions" } else { "recap, decisions, action items, open questions" };
        s.push_str(&format!("\n- The user's own notes are the backbone of the summary: keep every point they wrote, in their wording, put each where it belongs ({places}), and fill in the details from the transcript. When a note and the transcript disagree, follow the transcript."));
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

/// The labels the transcript uses, in order of first speaking: "Miguel (the user), Speaker 1, Dana Reyes". A speaker
/// the user named is marked as named (it wins over anything the conversation suggests); Them and Speaker N are not names.
fn roster(meeting: &Meeting, lines: &[TranscriptLine]) -> String {
    let mut seen: Vec<&str> = vec![];
    for l in lines.iter().filter(|l| !l.text.trim().is_empty()) {
        if !seen.contains(&l.speaker.as_str()) {
            seen.push(&l.speaker);
        }
    }
    seen.iter()
        .map(|sp| {
            let label = speaker_name(meeting, sp);
            let named = meeting.speakers.get(*sp).is_some_and(|n| !n.trim().is_empty() && !(*sp == "me" && n.trim() == "Me"));
            match (*sp == "me", named) {
                (true, true) => format!("{label} (the user)"),
                (true, false) => format!("{label} (the user, not a name)"),
                (false, true) => format!("{label} (name set by the user)"),
                (false, false) => format!("{label} (a label, not a name)"),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
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
    if meeting.item.app != "import" {
        let roster = roster(meeting, lines);
        if !roster.is_empty() {
            user.push_str(&format!("Speakers in this transcript: {roster}.\n"));
        }
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
    let names = "Use the same person's name everywhere: once a part reveals who someone is, use the name in every part.";
    let mut user = if is_detailed(opts) {
        let size = if opts.compact { "at most 3 takeaways and at most 3 topics" } else { "3 to 6 takeaways and the main topics" };
        format!(
            "These are summaries of the {} consecutive parts of one call (length {}), in order. Combine them into one summary of the whole call: a title and one purpose for the whole call, {size}, topics about the same subject joined into one (keep their facts, drop repeats), decisions and action items merged (keep the earliest time when two say the same thing), open questions that a later part answered removed. {names}\n",
            parts_json.len(),
            stamp(meeting.item.duration_ms)
        )
    } else {
        format!(
            "These are summaries of the {} consecutive parts of one call (length {}), in order. Combine them into one summary of the whole call: one recap for the whole call, a title for the whole call, decisions and action items merged (keep the earliest time when two say the same thing), open questions that a later part answered removed. {names}\n",
            parts_json.len(),
            stamp(meeting.item.duration_ms)
        )
    };
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
    use serde_json::json;
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
        // Due dates too: the model used to keep them as said, in English.
        assert!(p.system.contains("due: the deadline the call gave (\"Friday\", \"Oct 17\"), written in the summary's language"));
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
            recording_issue: None,
            transcript_only: false,
            template: None,
            summary_language: None,
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
    fn a_general_meeting_gets_the_detailed_layout_and_the_other_types_keep_their_own() {
        let lines = vec![line("l0", 60_000, "them:1", "Pricing moves to Q1."), line("l1", 66_000, "me", "Agreed.")];
        let mut m = meeting(lines.clone());
        m.marks.clear();
        let p = summary_prompt(&m, &lines, "en", None, &general());
        for part in ["- purpose: one sentence on why the meeting happened.", "- takeaways: the points worth remembering, 3 to 6.", "- topics: one entry for each main subject", "sub_bullets (one level, never deeper)", "- action_items: the next steps"] {
            assert!(p.system.contains(part), "{part}");
        }
        // The recap is written by Talaan, so the reply doesn't ask for one.
        assert!(!p.system.contains("- recap:"));
        assert!(!p.user.contains("notes") && !p.user.contains("marked"));
        // An unknown type is General too.
        let odd = Options { template: "from_the_future".into(), instructions: "  ".into(), ..Default::default() };
        assert_eq!(summary_prompt(&m, &lines, "en", None, &odd), p);
        // A meeting type keeps its recap and sections.
        let standup = Options { template: "standup".into(), ..Default::default() };
        let s = summary_prompt(&m, &lines, "en", None, &standup).system;
        assert!(s.contains("- recap: 3 to 5 plain sentences") && s.contains("- sections: exactly these 3") && !s.contains("- topics:"));
        assert_eq!(schema(false, false, false, 0), SCHEMA);
    }

    #[test]
    fn the_compact_layout_asks_for_fewer_topics_and_bullets() {
        let lines = vec![line("l0", 0, "me", "Hi.")];
        let m = meeting(lines.clone());
        let compact = Options { compact: true, ..Default::default() };
        let p = summary_prompt(&m, &lines, "en", None, &compact).system;
        assert!(p.contains("takeaways: the points worth remembering, at most 3.") && p.contains("Keep it compact: at most 3 topics with at most 3 short bullets each"));
        assert!(!summary_prompt(&m, &lines, "en", None, &general()).system.contains("Keep it compact"));
        let full: Value = serde_json::from_str(&schema(true, false, false, 0)).unwrap();
        let small: Value = serde_json::from_str(&schema(true, true, false, 0)).unwrap();
        assert_eq!(full["properties"]["topics"]["maxItems"], 8);
        assert_eq!(small["properties"]["topics"]["maxItems"], 3);
        assert_eq!(small["properties"]["topics"]["items"]["properties"]["bullets"]["maxItems"], 3);
        assert!(small.to_string().len() < full.to_string().len());
        assert!(merge_prompt(&m, &["{}".into()], "en", &compact).user.contains("at most 3 takeaways and at most 3 topics"));
    }

    #[test]
    fn the_detailed_schema_lists_purpose_first_and_every_property_is_required() {
        let v: Value = serde_json::from_str(&schema(true, false, false, 0)).unwrap();
        let keys: Vec<&str> = v["properties"].as_object().unwrap().keys().map(|k| k.as_str()).collect();
        // Written in the order of the summary (the order the model fills them in).
        let text = schema(true, false, false, 0);
        let at = |k: &str| text.find(&format!("\"{k}\":")).unwrap();
        assert!(at("title") < at("purpose") && at("purpose") < at("takeaways") && at("takeaways") < at("topics") && at("topics") < at("decisions") && at("decisions") < at("action_items") && at("action_items") < at("open_questions"));
        assert!(!keys.contains(&"recap") && keys.contains(&"purpose") && keys.contains(&"topics"));
        assert_eq!(v["required"].as_array().unwrap().len(), keys.len());
        // With marked moments, still one schema with highlights last.
        let with: Value = serde_json::from_str(&schema(true, false, false, 2)).unwrap();
        assert_eq!(with["properties"]["highlights"]["maxItems"], 2);
        assert!(with["required"].as_array().unwrap().contains(&json!("highlights")));
    }

    /// OpenAI's strict mode and Gemini accept only a small set of keywords, and every object must close its properties
    /// and require all of them.
    #[test]
    fn every_schema_uses_only_keywords_the_cloud_engines_accept() {
        fn walk(v: &Value, at: &str) {
            let Some(o) = v.as_object() else { return };
            for k in o.keys() {
                assert!(["type", "properties", "required", "items", "maxItems", "additionalProperties"].contains(&k.as_str()), "{at}: unsupported keyword {k}");
            }
            if o.get("type") == Some(&json!("object")) {
                let props = o["properties"].as_object().unwrap();
                assert_eq!(o["additionalProperties"], json!(false), "{at}");
                let mut req: Vec<&str> = o["required"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
                let mut all: Vec<&str> = props.keys().map(|k| k.as_str()).collect();
                req.sort();
                all.sort();
                assert_eq!(req, all, "{at}: every property is required");
                for (k, child) in props {
                    walk(child, &format!("{at}.{k}"));
                }
            }
            if let Some(items) = o.get("items") {
                walk(items, &format!("{at}[]"));
            }
        }
        for detailed in [false, true] {
            for compact in [false, true] {
                for sections in [false, true] {
                    for highlights in [0, 3] {
                        let text = schema(detailed, compact, sections, highlights);
                        walk(&serde_json::from_str(&text).unwrap(), &format!("schema({detailed},{compact},{sections},{highlights})"));
                    }
                }
            }
        }
    }

    #[test]
    fn people_are_named_for_real_in_every_meeting_type() {
        let lines = vec![line("l0", 0, "them:2", "This is Kevin from finance."), line("l1", 5_000, "me", "Hi Kevin.")];
        let mut m = meeting(lines.clone());
        m.marks.clear();
        let general = summary_prompt(&m, &lines, "en", None, &general()).system;
        for part in ["Names: use real names.", "people addressing each other, introductions such as \"this is Kevin\"", "A name the user set in the transcript always wins.", "a neutral role if the context makes it clear (\"the recruiter\", \"the client\")", "keep the transcript label (\"Speaker 1\")", "Never invent a name."] {
            assert!(general.contains(part), "{part}");
        }
        let one = Options { template: "one_on_one".into(), ..Default::default() };
        let other = summary_prompt(&m, &lines, "en", None, &one).system;
        assert!(other.contains("Names: use real names.") && other.contains("Never invent a name."));
        // The transcript's labels are explained: a set name versus a label.
        let user = summary_prompt(&m, &lines, "en", None, &Options::default()).user;
        assert!(user.contains("Speakers in this transcript: Speaker 2 (a label, not a name), Me (the user, not a name)."), "{user}");
    }

    #[test]
    fn your_name_is_how_the_summary_writes_the_user_and_without_it_the_summary_speaks_to_you() {
        let lines = vec![line("l0", 0, "me", "I'll send the deck."), line("l1", 4_000, "them:1", "Thanks.")];
        let mut m = meeting(lines.clone());
        m.speakers.insert("me".into(), "Miguel".into());
        let named = summary_prompt(&m, &lines, "en", None, &general());
        assert!(named.system.contains("The user is \"Miguel\": write that name for them, never \"Me\"."));
        assert!(named.system.contains("(\"Miguel\" for the user)"));
        assert!(named.user.contains("Speakers in this transcript: Miguel (the user), Dana Reyes (name set by the user)."));
        assert!(!named.system.contains("second person"));
        // Your name empty: the transcript says Me, which is not a name.
        m.speakers.remove("me");
        let unnamed = summary_prompt(&m, &lines, "en", None, &general());
        assert!(unnamed.system.contains("\"Me\" is not a name: never write it."));
        assert!(unnamed.system.contains("second person (\"You will send the plan\""));
        assert!(unnamed.system.contains("(\"You\" for the user)"));
        // The same for a meeting type, and for the merge step.
        let standup = Options { template: "standup".into(), ..Default::default() };
        assert!(summary_prompt(&m, &lines, "en", None, &standup).system.contains("second person"));
        assert!(merge_prompt(&m, &["{}".into()], "en", &general()).system.contains("second person"));
        assert!(merge_prompt(&m, &["{}".into()], "en", &general()).user.contains("Use the same person's name everywhere"));
        m.speakers.insert("me".into(), "Me".into());
        assert_eq!(user_name(&m), None);
        m.speakers.insert("me".into(), "Miguel".into());
        assert_eq!(user_name(&m).as_deref(), Some("Miguel"));
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
        let s: Value = serde_json::from_str(&schema(false, false, false, 1)).unwrap();
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
        let standup = Options { template: "standup".into(), ..Default::default() };
        let p = summary_prompt(&m, &lines, "fr", None, &standup);
        assert!(p.system.contains("This call is a team stand-up"));
        assert!(p.system.contains("with these titles: \"Hier\", \"Aujourd’hui\", \"Blocages\""));
        assert!(p.system.contains("Start every item with the person's name"));
        let de = summary_prompt(&m, &lines, "de", None, &standup);
        assert!(de.system.contains("with these titles (translated into German): \"Yesterday\", \"Today\", \"Blockers\""));
        let s: Value = serde_json::from_str(&schema(false, false, true, 0)).unwrap();
        assert!(s["required"].as_array().unwrap().contains(&json!("sections")));
        assert!(merge_prompt(&m, &["{}".into()], "fr", &standup).user.contains("Merge sections that have the same title"));
    }

    #[test]
    fn custom_instructions_are_followed_without_inventing_facts() {
        let lines = vec![line("l0", 0, "me", "Hi.")];
        let m = meeting(lines.clone());
        let o = Options { template: "general".into(), instructions: "  Always mention budget numbers.  ".into(), ..Default::default() };
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
        assert!(p.system.contains("\"Me\" is the user") && p.system.contains("(\"You\" for the user)"));
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
        let both: serde_json::Value = serde_json::from_str(&schema(false, false, true, 3)).unwrap();
        assert_eq!(both["required"].as_array().unwrap().len(), 7);
        let detailed: serde_json::Value = serde_json::from_str(&schema(true, false, false, 0)).unwrap();
        assert_eq!(detailed["required"].as_array().unwrap().len(), 7);
    }
}
