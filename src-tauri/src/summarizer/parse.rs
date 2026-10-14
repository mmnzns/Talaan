//! Reading a model's reply. Models wrap JSON in prose or code fences, get times slightly wrong and repeat
//! themselves; this turns whatever came back into a clean `Summary` or says exactly why it can't.

use crate::storage::model::{ActionItem, Summary, SummaryItem, TranscriptLine};
use serde::{Deserialize, Serialize};

/// The reply as the schema describes it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Draft {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub recap: String,
    #[serde(default)]
    pub decisions: Vec<Timed>,
    #[serde(default)]
    pub action_items: Vec<Action>,
    #[serde(default)]
    pub open_questions: Vec<Timed>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Timed {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub time: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Action {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub due: String,
    #[serde(default)]
    pub time: String,
}

/// The JSON object in a reply: code fences and text around it are ignored.
pub fn extract_json(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end > start).then(|| &text[start..=end])
}

pub fn parse_draft(text: &str) -> Result<Draft, String> {
    let json = extract_json(text).ok_or("the reply had no JSON object")?;
    let draft: Draft = serde_json::from_str(json).map_err(|e| format!("the reply wasn't valid JSON ({e})"))?;
    if draft.recap.trim().is_empty() {
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
    Summary { recap: clean(&draft.recap), decisions, action_items, open_questions, engine: engine.into(), edited: false }
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

    #[test]
    fn titles_are_used_only_when_they_name_something() {
        assert_eq!(usable_title(" \"Q4 roadmap sync.\" ").as_deref(), Some("Q4 roadmap sync"));
        assert_eq!(usable_title("Meeting"), None);
        assert_eq!(usable_title("This was a long call where we talked about many different things today"), None);
    }
}
