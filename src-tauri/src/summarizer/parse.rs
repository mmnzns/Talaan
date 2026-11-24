//! Reading a model's reply. Models wrap JSON in prose or code fences, get times slightly wrong and repeat
//! themselves; this turns whatever came back into a clean `Summary` or says exactly why it can't.

use super::prompt::{around, moments, speaker_name};
use super::templates::Template;
use crate::storage::model::{ActionItem, Meeting, Summary, SummaryBullet, SummaryItem, SummarySection, SummaryTopic, Takeaway, TranscriptLine};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

/// The reply as the schema describes it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Draft {
    #[serde(default, deserialize_with = "null_string")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "null_string")]
    pub recap: String,
    /// The detailed layout (General meetings): why the call happened, the points to remember and one entry per topic.
    /// Read leniently like `sections`, and absent from a reply in the older shape.
    #[serde(default, skip_serializing_if = "String::is_empty", deserialize_with = "null_string")]
    pub purpose: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "lenient")]
    pub takeaways: Vec<DraftTakeaway>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "lenient")]
    pub topics: Vec<DraftTopic>,
    #[serde(default, deserialize_with = "null_vec")]
    pub decisions: Vec<Timed>,
    #[serde(default, deserialize_with = "null_vec")]
    pub action_items: Vec<Action>,
    #[serde(default, deserialize_with = "null_vec")]
    pub open_questions: Vec<Timed>,
    /// The meeting type's own parts (only asked for when it has some). Read leniently: a malformed entry is dropped
    /// rather than costing the whole summary.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "lenient")]
    pub sections: Vec<DraftSection>,
    /// One per marked moment (only asked for when there are some). Lenient like `sections`.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "lenient")]
    pub highlights: Vec<Timed>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DraftTakeaway {
    #[serde(default, deserialize_with = "null_string")]
    pub label: String,
    #[serde(default, deserialize_with = "null_string")]
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DraftTopic {
    #[serde(default, deserialize_with = "null_string")]
    pub title: String,
    #[serde(default, deserialize_with = "lenient_bullets")]
    pub bullets: Vec<DraftBullet>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct DraftBullet {
    pub text: String,
    pub sub_bullets: Vec<String>,
}

/// Bullets as the schema asks (`{text, sub_bullets}`), or as a small model sometimes writes them (a plain string, or
/// an object with `sub_bullets` missing or not a list). Entries that are neither are skipped.
fn lenient_bullets<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<DraftBullet>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    let serde_json::Value::Array(items) = v else { return Ok(vec![]) };
    Ok(items
        .into_iter()
        .filter_map(|x| match x {
            serde_json::Value::String(text) => Some(DraftBullet { text, sub_bullets: vec![] }),
            serde_json::Value::Object(o) => {
                let text = o.get("text").and_then(|t| t.as_str())?.to_string();
                let sub_bullets = match o.get("sub_bullets").or_else(|| o.get("subBullets")) {
                    Some(serde_json::Value::Array(a)) => a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect(),
                    _ => vec![],
                };
                Some(DraftBullet { text, sub_bullets })
            }
            _ => None,
        })
        .collect())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DraftSection {
    #[serde(default, deserialize_with = "null_string")]
    pub title: String,
    #[serde(default, deserialize_with = "null_vec")]
    pub items: Vec<Timed>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Timed {
    #[serde(default, deserialize_with = "null_string")]
    pub text: String,
    #[serde(default, deserialize_with = "null_string")]
    pub time: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Action {
    #[serde(default, deserialize_with = "null_string")]
    pub text: String,
    #[serde(default, deserialize_with = "null_string")]
    pub owner: String,
    #[serde(default, deserialize_with = "null_string")]
    pub due: String,
    #[serde(default, deserialize_with = "null_string")]
    pub time: String,
}

/// A text field a model wrote as `null` reads as missing (an empty string), like a field it left out.
fn null_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}

/// A list a model wrote as `null` reads as missing (empty).
fn null_vec<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Vec<T>, D::Error> {
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}

/// A list where each entry that doesn't fit is skipped, and anything that isn't a list reads as empty.
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Vec<T>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(match v {
        serde_json::Value::Array(items) => items.into_iter().filter_map(|x| serde_json::from_value(x).ok()).collect(),
        _ => vec![],
    })
}

/// The JSON object in a reply: code fences and text around it are ignored.
pub fn extract_json(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end > start).then(|| &text[start..=end])
}

/// The reply without `<think>…</think>` blocks, which reasoning models put before (or around) the answer.
/// Braces inside them would otherwise be taken for the JSON. An unclosed block runs to the end of the reply.
pub fn strip_reasoning(text: &str) -> String {
    const OPEN: &str = "<think>";
    const CLOSE: &str = "</think>";
    // ASCII lowercasing keeps byte offsets, so positions found in `lower` cut `text` in the same places.
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(start) = lower[at..].find(OPEN).map(|i| at + i) {
        out.push_str(&text[at..start]);
        match lower[start..].find(CLOSE) {
            Some(end) => at = start + end + CLOSE.len(),
            None => return out,
        }
    }
    out.push_str(&text[at..]);
    out
}

pub fn parse_draft(text: &str) -> Result<Draft, String> {
    let text = strip_reasoning(text);
    let json = extract_json(&text).ok_or("the reply had no JSON object")?;
    let draft: Draft = serde_json::from_str(json).map_err(|e| format!("the reply wasn't valid JSON ({e})"))?;
    if draft.recap.trim().is_empty() && draft.purpose.trim().is_empty() {
        return Err("the reply had no recap".into());
    }
    Ok(draft)
}

/// "12:34", "1:02:03" or "[12:34]" to milliseconds.
pub fn parse_time(t: &str) -> Option<u64> {
    let t = t.trim().trim_start_matches('[').trim_end_matches(']').trim();
    let parts: Vec<u64> = t.split(':').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    let s = match parts.as_slice() {
        [m, s] if *s < 60 => m * 60 + s,
        [h, m, s] if *m < 60 && *s < 60 => h * 3600 + m * 60 + s,
        _ => return None,
    };
    Some(s * 1000)
}

/// The start of the transcript line a time points at: the line containing it, or the nearest line start
/// within 30 s. A time that matches nothing is dropped rather than shown pointing at the wrong moment.
pub fn snap(ms: u64, lines: &[TranscriptLine]) -> Option<u64> {
    if let Some(l) = lines.iter().find(|l| l.at <= ms + 999 && ms <= l.end) {
        return Some(l.at);
    }
    lines.iter().map(|l| l.at).min_by_key(|at| at.abs_diff(ms)).filter(|at| at.abs_diff(ms) <= 30_000)
}

fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Turns a draft into the stored summary: ids, snapped times, no empty or repeated items.
pub fn to_summary(draft: &Draft, lines: &[TranscriptLine], engine: &str) -> Summary {
    let time = |t: &str| parse_time(t).and_then(|ms| snap(ms, lines));
    let mut seen = std::collections::HashSet::new();
    let mut fresh = |kind: &str, text: &str| !text.is_empty() && seen.insert(format!("{kind}:{}", text.to_lowercase()));
    let decisions = draft
        .decisions
        .iter()
        .map(|d| (clean(&d.text), &d.time))
        .filter(|(t, _)| fresh("d", t))
        .enumerate()
        .map(|(i, (text, t))| SummaryItem { id: format!("d{i}"), text, at: time(t) })
        .collect();
    let action_items = draft
        .action_items
        .iter()
        .map(|a| (clean(&a.text), a))
        .filter(|(t, _)| fresh("a", t))
        .enumerate()
        .map(|(i, (text, a))| ActionItem {
            id: format!("a{i}"),
            text,
            at: time(&a.time),
            owner: Some(clean(&a.owner)).filter(|o| !o.is_empty()),
            due: Some(clean(&a.due)).filter(|d| !d.is_empty()),
            done: false,
        })
        .collect();
    let open_questions = draft
        .open_questions
        .iter()
        .map(|q| (clean(&q.text), &q.time))
        .filter(|(t, _)| fresh("q", t))
        .enumerate()
        .map(|(i, (text, t))| SummaryItem { id: format!("q{i}"), text, at: time(t) })
        .collect();
    let purpose = Some(clean(&draft.purpose)).filter(|p| !p.is_empty());
    let takeaways = to_takeaways(draft);
    let topics = to_topics(draft);
    // The recap (what notifications, Ask, the follow-up email and list previews read) is the model's own when it wrote
    // one, else the purpose and the takeaways as plain sentences.
    let recap = if clean(&draft.recap).is_empty() { recap_from(purpose.as_deref(), &takeaways) } else { clean(&draft.recap) };
    Summary { recap, purpose, takeaways, topics, decisions, action_items, open_questions, sections: vec![], highlights: vec![], template: None, language: None, sections_missing: false, engine: engine.into(), edited: false }
}

/// The purpose, then each takeaway as "Label: text", as plain sentences.
pub fn recap_from(purpose: Option<&str>, takeaways: &[Takeaway]) -> String {
    let mut parts: Vec<String> = purpose.into_iter().map(|p| p.to_string()).collect();
    for t in takeaways {
        let label = t.label.trim().trim_end_matches(':');
        parts.push(if label.is_empty() { t.text.clone() } else { format!("{label}: {}", t.text) });
    }
    parts.iter().map(|p| if p.ends_with(['.', '!', '?', '。']) { p.clone() } else { format!("{p}.") }).collect::<Vec<_>>().join(" ")
}

/// Takeaways with a text, labels cleaned, repeats dropped.
fn to_takeaways(draft: &Draft) -> Vec<Takeaway> {
    let mut out: Vec<Takeaway> = vec![];
    for t in &draft.takeaways {
        let (label, text) = (clean(&t.label).trim_end_matches(':').trim().to_string(), clean(&t.text));
        if !text.is_empty() && !out.iter().any(|o| o.text.eq_ignore_ascii_case(&text)) {
            out.push(Takeaway { label, text });
        }
    }
    out
}

/// Topics with a title and at least one bullet; empty and repeated bullets dropped; a topic named twice (a merge that
/// didn't merge) is one topic.
fn to_topics(draft: &Draft) -> Vec<SummaryTopic> {
    let mut out: Vec<SummaryTopic> = vec![];
    for t in &draft.topics {
        let title = clean(&t.title);
        let mut bullets: Vec<SummaryBullet> = vec![];
        for b in &t.bullets {
            let text = clean(&b.text);
            if text.is_empty() || bullets.iter().any(|x| x.text.eq_ignore_ascii_case(&text)) {
                continue;
            }
            let mut subs: Vec<String> = vec![];
            for sub in b.sub_bullets.iter().map(|x| clean(x)).filter(|x| !x.is_empty()) {
                if !subs.iter().any(|x| x.eq_ignore_ascii_case(&sub)) {
                    subs.push(sub);
                }
            }
            bullets.push(SummaryBullet { text, sub_bullets: subs });
        }
        if title.is_empty() || bullets.is_empty() {
            continue;
        }
        match out.iter_mut().find(|o| o.title.to_lowercase() == title.to_lowercase()) {
            Some(o) => {
                for b in bullets {
                    if !o.bullets.iter().any(|x| x.text.eq_ignore_ascii_case(&b.text)) {
                        o.bullets.push(b);
                    }
                }
            }
            None => out.push(SummaryTopic { id: String::new(), title, bullets }),
        }
    }
    for (i, t) in out.iter_mut().enumerate() {
        t.id = format!("t{i}");
    }
    out
}

/// The meeting type's sections: titles in the type's own words, items with snapped times, empty ones dropped.
pub fn to_sections(draft: &Draft, lines: &[TranscriptLine], template: &Template, language: &str) -> Vec<SummarySection> {
    if template.is_general() {
        return vec![];
    }
    let mut out: Vec<SummarySection> = vec![];
    for s in &draft.sections {
        let title = template.canonical_title(&clean(&s.title), language);
        let mut items: Vec<SummaryItem> = vec![];
        for it in &s.items {
            let text = clean(&it.text);
            if !text.is_empty() && !items.iter().any(|x| x.text.eq_ignore_ascii_case(&text)) {
                items.push(SummaryItem { id: String::new(), text, at: parse_time(&it.time).and_then(|ms| snap(ms, lines)) });
            }
        }
        if title.is_empty() || items.is_empty() {
            continue;
        }
        // The same title twice (a merge that didn't merge): one section.
        match out.iter_mut().find(|o| o.title.to_lowercase() == title.to_lowercase()) {
            Some(o) => o.items.extend(items.into_iter().filter(|i| !o.items.iter().any(|x| x.text.eq_ignore_ascii_case(&i.text))).collect::<Vec<_>>()),
            None => out.push(SummarySection { id: String::new(), title, items }),
        }
    }
    for (i, s) in out.iter_mut().enumerate() {
        s.id = format!("s{i}");
        for (j, it) in s.items.iter_mut().enumerate() {
            it.id = format!("s{i}i{j}");
        }
    }
    out
}

/// What a highlight says when the model left a marked moment out: the line said at that moment.
fn said_at(meeting: &Meeting, at: u64) -> Option<String> {
    let near = around(&meeting.transcript, at);
    let l = near.iter().find(|l| l.at <= at + 999 && at <= l.end).or_else(|| near.iter().min_by_key(|l| l.at.abs_diff(at)))?;
    let text: String = clean(&l.text).chars().take(200).collect();
    Some(format!("{}: {text}", speaker_name(meeting, &l.speaker)))
}

/// One highlight per marked moment, in time order: the model's words for it (the closest highlight within 30 s), or
/// the line said then when the model skipped it. Times point at the marked moment's line.
pub fn to_highlights(draft: &Draft, meeting: &Meeting) -> Vec<SummaryItem> {
    let mut used = vec![false; draft.highlights.len()];
    let mut marks = moments(meeting);
    marks.sort_by_key(|m| m.at);
    let mut out = vec![];
    for m in marks {
        let best = draft
            .highlights
            .iter()
            .enumerate()
            .filter(|(i, h)| !used[*i] && !clean(&h.text).is_empty())
            .filter_map(|(i, h)| parse_time(&h.time).map(|t| (i, t.abs_diff(m.at))))
            .filter(|(_, d)| *d <= 30_000)
            .min_by_key(|(_, d)| *d);
        let text = match best {
            Some((i, _)) => {
                used[i] = true;
                Some(clean(&draft.highlights[i].text))
            }
            None => said_at(meeting, m.at),
        };
        if let Some(text) = text {
            let at = snap(m.at, &meeting.transcript).unwrap_or(m.at);
            out.push(SummaryItem { id: format!("h{}", out.len()), text, at: Some(at) });
        }
    }
    out
}

/// A title worth using: short, not a sentence, not just "Meeting".
pub fn usable_title(title: &str) -> Option<String> {
    let t = clean(title).trim_matches(|c: char| c == '"' || c == '.' || c == '\'').to_string();
    let words = t.split_whitespace().count();
    let generic = ["meeting", "call", "meeting summary", "summary", "discussion"].contains(&t.to_lowercase().as_str());
    (!t.is_empty() && words <= 8 && t.len() <= 60 && !generic).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summarizer::prompt::tests::line;

    fn lines() -> Vec<TranscriptLine> {
        vec![line("l0", 0, "me", "Hi."), line("l1", 60_000, "them:0", "Pricing moves to Q1."), line("l2", 125_000, "me", "I'll send wireframes Friday.")]
    }

    #[test]
    fn json_is_found_inside_fences_and_prose() {
        let reply = "Sure! Here it is:\n```json\n{\"title\":\"Q4 sync\",\"recap\":\"We met.\",\"decisions\":[],\"action_items\":[],\"open_questions\":[]}\n```\nHope that helps.";
        let d = parse_draft(reply).unwrap();
        assert_eq!(d.title, "Q4 sync");
    }

    #[test]
    fn reasoning_blocks_are_ignored() {
        let reply = "<think>The user wants {json}. Maybe {\"recap\": \"wrong\"}?</think>\n{\"title\":\"Q4 sync\",\"recap\":\"We met.\"}";
        assert_eq!(parse_draft(reply).unwrap().recap, "We met.");
        // Cut off mid-thought: there is no answer to find.
        assert_eq!(parse_draft("<think>Let me write {\"recap\": \"x\"} and"), Err("the reply had no JSON object".into()));
        // Two blocks, any case, and text that only mentions the tag.
        let reply = "<THINK>{a}</THINK>{\"recap\":\"One.\"}<think>{b}</think>";
        assert_eq!(parse_draft(reply).unwrap().recap, "One.");
        assert_eq!(strip_reasoning("no tags {\"recap\":\"x\"}"), "no tags {\"recap\":\"x\"}");
    }

    #[test]
    fn a_malformed_optional_part_never_costs_the_recap() {
        let d = parse_draft(r#"{"recap":"We met.","sections":"none","highlights":[{"time":"01:00","text":"Kept."},{"time":5},"junk"]}"#).unwrap();
        assert_eq!(d.recap, "We met.");
        assert!(d.sections.is_empty());
        assert_eq!(d.highlights, vec![Timed { text: "Kept.".into(), time: "01:00".into() }]);
        let d = parse_draft(r#"{"recap":"R.","sections":[{"title":"Today","items":"x"},{"title":"Blockers","items":[]}]}"#).unwrap();
        assert_eq!(d.sections.len(), 1, "the entry whose items aren't a list is dropped");
    }

    #[test]
    fn null_reads_like_missing_for_the_text_and_list_fields() {
        let d = parse_draft(r#"{"title":null,"recap":null,"purpose":"Plan the pilot.","takeaways":[{"label":null,"text":"Two teams."}],"topics":[{"title":null,"bullets":null}],"decisions":null,"action_items":[{"text":"Send it.","owner":null,"due":null,"time":null}],"open_questions":null,"sections":null,"highlights":null}"#).unwrap();
        assert_eq!((d.title.as_str(), d.recap.as_str(), d.purpose.as_str()), ("", "", "Plan the pilot."));
        assert_eq!(d.takeaways, vec![DraftTakeaway { label: "".into(), text: "Two teams.".into() }]);
        assert!(d.decisions.is_empty() && d.open_questions.is_empty() && d.sections.is_empty() && d.highlights.is_empty());
        assert_eq!(d.action_items, vec![Action { text: "Send it.".into(), ..Default::default() }]);
        // A null recap with nothing else is still no summary.
        assert!(parse_draft(r#"{"recap":null}"#).is_err());
    }

    #[test]
    fn bad_replies_say_why() {
        assert_eq!(parse_draft("I can't help with that."), Err("the reply had no JSON object".into()));
        assert!(parse_draft("{\"recap\": \"cut off").unwrap_err().contains("no JSON object") || parse_draft("{\"recap\": \"cut off").is_err());
        assert!(parse_draft("{\"recap\": 3}").unwrap_err().contains("valid JSON"));
        assert_eq!(parse_draft("{\"recap\": \"  \", \"title\": \"x\"}"), Err("the reply had no recap".into()));
        // Missing lists are fine: an empty list means nothing to report.
        assert!(parse_draft("{\"recap\": \"We met.\"}").is_ok());
    }

    #[test]
    fn times_parse_in_every_shape_models_use() {
        assert_eq!(parse_time("01:05"), Some(65_000));
        assert_eq!(parse_time("[1:05]"), Some(65_000));
        assert_eq!(parse_time("1:02:03"), Some(3_723_000));
        assert_eq!(parse_time(""), None);
        assert_eq!(parse_time("1:75"), None);
        assert_eq!(parse_time("soon"), None);
    }

    #[test]
    fn times_snap_to_a_real_line_or_are_dropped() {
        let l = lines();
        assert_eq!(snap(61_000, &l), Some(60_000)); // inside a line
        assert_eq!(snap(70_000, &l), Some(60_000)); // near a line start
        assert_eq!(snap(400_000, &l), None); // nowhere near anything
    }

    #[test]
    fn a_draft_becomes_a_clean_summary() {
        let d = Draft {
            title: "Q4 roadmap".into(),
            recap: "  We   planned Q4. ".into(),
            decisions: vec![Timed { text: "Pricing moves to Q1.".into(), time: "01:00".into() }, Timed { text: "pricing moves to q1.".into(), time: "01:02".into() }, Timed { text: " ".into(), time: "".into() }],
            action_items: vec![Action { text: "Send wireframes".into(), owner: "Me".into(), due: "Friday".into(), time: "02:05".into() }, Action { text: "Book a room".into(), owner: "".into(), due: "".into(), time: "9:99".into() }],
            open_questions: vec![Timed { text: "Who owns analytics?".into(), time: "".into() }],
            ..Default::default()
        };
        let s = to_summary(&d, &lines(), "Local AI · Qwen3.5 4B");
        assert_eq!(s.recap, "We planned Q4.");
        assert_eq!(s.decisions.len(), 1);
        assert_eq!(s.decisions[0].at, Some(60_000));
        assert_eq!(s.action_items[0].owner.as_deref(), Some("Me"));
        assert_eq!(s.action_items[0].due.as_deref(), Some("Friday"));
        assert_eq!(s.action_items[0].at, Some(125_000));
        assert_eq!((s.action_items[1].owner.clone(), s.action_items[1].at), (None, None));
        assert_eq!(s.open_questions[0].id, "q0");
        assert_eq!(s.engine, "Local AI · Qwen3.5 4B");
    }

    fn timed(text: &str, time: &str) -> Timed {
        Timed { text: text.into(), time: time.into() }
    }

    #[test]
    fn sections_keep_the_types_titles_and_drop_empty_ones() {
        let t = crate::summarizer::templates::find("interview");
        let d = Draft {
            sections: vec![
                DraftSection { title: "Strengths".into(), items: vec![timed("Led a migration.", "01:00"), timed("led a migration.", "01:01")] },
                DraftSection { title: "Concerns".into(), items: vec![] },
                DraftSection { title: "Next steps:".into(), items: vec![timed("Second round Friday.", "")] },
                DraftSection { title: "strengths".into(), items: vec![timed("Clear speaker.", "02:05")] },
            ],
            ..Default::default()
        };
        let s = to_sections(&d, &lines(), t, "fr");
        assert_eq!(s.iter().map(|x| x.title.as_str()).collect::<Vec<_>>(), vec!["Points forts", "Prochaines étapes"]);
        assert_eq!(s[0].items.iter().map(|i| (i.id.as_str(), i.text.as_str(), i.at)).collect::<Vec<_>>(), vec![("s0i0", "Led a migration.", Some(60_000)), ("s0i1", "Clear speaker.", Some(125_000))]);
        assert_eq!(s[1].id, "s1");
        // General has no sections, whatever the model sent.
        assert!(to_sections(&d, &lines(), crate::summarizer::templates::find("general"), "en").is_empty());
    }

    #[test]
    fn every_marked_moment_gets_a_highlight() {
        use crate::summarizer::prompt::tests::{mark, meeting};
        let mut m = meeting(lines());
        m.marks = vec![mark("k1", 126_000, None), mark("k0", 61_000, None), mark("n0", 62_000, Some("a note, not a highlight"))];
        let d = Draft { highlights: vec![timed("Pricing moved to the first quarter.", "01:01"), timed("Far away.", "9:00")], ..Default::default() };
        let h = to_highlights(&d, &m);
        assert_eq!(h.len(), 2);
        assert_eq!((h[0].id.as_str(), h[0].text.as_str(), h[0].at), ("h0", "Pricing moved to the first quarter.", Some(60_000)));
        // The model skipped this one: the line said then stands in.
        assert_eq!((h[1].text.as_str(), h[1].at), ("Me: I'll send wireframes Friday.", Some(125_000)));
        m.marks.clear();
        assert!(to_highlights(&d, &m).is_empty());
    }

    #[test]
    fn titles_are_used_only_when_they_name_something() {
        assert_eq!(usable_title(" \"Q4 roadmap sync.\" ").as_deref(), Some("Q4 roadmap sync"));
        assert_eq!(usable_title("Meeting"), None);
        assert_eq!(usable_title("This was a long call where we talked about many different things today"), None);
    }

    #[test]
    fn the_detailed_fields_are_read_and_cleaned() {
        let reply = r#"{"title":"Pilot","purpose":" To plan  the pilot. ","takeaways":[{"label":"Scope:","text":"Two teams."},{"label":"Scope","text":"two teams."},{"label":"","text":"No label here."},{"label":"Empty","text":" "}],
            "topics":[{"title":"Plan","bullets":[{"text":"Two teams.","sub_bullets":["A","a"," ","B"]},{"text":"two teams.","sub_bullets":[]},{"text":"","sub_bullets":[]}]},
                      {"title":"plan","bullets":[{"text":"Review in six weeks.","sub_bullets":[]}]},{"title":"Empty topic","bullets":[]},{"title":"","bullets":[{"text":"x","sub_bullets":[]}]}],
            "decisions":[],"action_items":[],"open_questions":[]}"#;
        let d = parse_draft(reply).unwrap();
        let s = to_summary(&d, &lines(), "Test");
        assert_eq!(s.purpose.as_deref(), Some("To plan the pilot."));
        assert_eq!(s.takeaways.iter().map(|t| (t.label.as_str(), t.text.as_str())).collect::<Vec<_>>(), vec![("Scope", "Two teams."), ("", "No label here.")]);
        // The same topic named twice is one topic; empty and repeated bullets and sub-bullets are dropped.
        assert_eq!(s.topics.len(), 1);
        assert_eq!((s.topics[0].id.as_str(), s.topics[0].title.as_str()), ("t0", "Plan"));
        assert_eq!(s.topics[0].bullets.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(), vec!["Two teams.", "Review in six weeks."]);
        assert_eq!(s.topics[0].bullets[0].sub_bullets, vec!["A", "B"]);
        assert_eq!(s.recap, "To plan the pilot. Scope: Two teams. No label here.");
    }

    #[test]
    fn a_purpose_alone_is_enough_and_a_reply_with_neither_is_not() {
        assert!(parse_draft(r#"{"title":"T","purpose":"To meet.","decisions":[],"action_items":[],"open_questions":[]}"#).is_ok());
        assert_eq!(parse_draft(r#"{"title":"T","recap":"","purpose":" ","topics":[]}"#), Err("the reply had no recap".into()));
    }

    #[test]
    fn small_models_bullets_written_as_strings_or_with_odd_sub_bullets_are_still_read() {
        let reply = r#"{"title":"T","purpose":"P.","topics":[{"title":"Plan","bullets":["Plain bullet.",{"text":"Object bullet.","sub_bullets":"not a list"},{"text":"With subs.","subBullets":["One"]},7]}],"takeaways":"oops"}"#;
        let d = parse_draft(reply).unwrap();
        let s = to_summary(&d, &lines(), "Test");
        assert_eq!(s.topics[0].bullets.len(), 3);
        assert_eq!(s.topics[0].bullets[2].sub_bullets, vec!["One"]);
        assert!(s.takeaways.is_empty());
    }

    #[test]
    fn an_older_reply_keeps_its_recap_and_has_no_detailed_fields() {
        let d = parse_draft(r#"{"title":"T","recap":"We met.","decisions":[],"action_items":[],"open_questions":[]}"#).unwrap();
        let s = to_summary(&d, &lines(), "Test");
        assert_eq!(s.recap, "We met.");
        assert!(s.purpose.is_none() && s.takeaways.is_empty() && s.topics.is_empty() && !s.is_detailed());
        // A draft that goes into a merge doesn't carry empty detail fields.
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("purpose") && !json.contains("topics") && !json.contains("takeaways"));
    }

    #[test]
    fn the_recap_is_the_purpose_then_the_takeaways_as_sentences() {
        let k = |l: &str, t: &str| Takeaway { label: l.into(), text: t.into() };
        assert_eq!(recap_from(Some("To plan."), &[k("Scope", "Two teams."), k("Budget:", "It fits!")]), "To plan. Scope: Two teams. Budget: It fits!");
        assert_eq!(recap_from(None, &[k("", "Only this")]), "Only this.");
        assert_eq!(recap_from(None, &[]), "");
    }
}
