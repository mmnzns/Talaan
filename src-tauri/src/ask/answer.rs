//! The Ask prompt, its output schema, and turning the reply into an answer with citations.

use super::passages::Passage;
use crate::summarizer::prompt::{stamp, Prompt};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const SCHEMA: &str = r#"{"type":"object","properties":{"answer":{"type":"string"},"points":{"type":"array","maxItems":6,"items":{"type":"object","properties":{"text":{"type":"string"},"source":{"type":"integer"}},"required":["text","source"]}}},"required":["answer","points"]}"#;

const SYSTEM: &str = "You answer questions about the user's own meetings, using only the numbered excerpts provided. \
\"Me\" in the excerpts is the user; address them as \"you\".\n\
Reply with JSON: \"answer\" is a direct answer in one to three sentences; \"points\" are up to six supporting facts, \
each with the number of the excerpt it comes from in \"source\".\n\
Rules:\n\
- Use only what the excerpts say. Never guess or add outside knowledge.\n\
- If the excerpts don't answer the question, reply with an empty \"answer\" and no points.\n\
- Keep names, numbers and dates exactly as written.\n\
- Write in the language of the question.";

/// What a meeting is called in the prompt and in citations.
#[derive(Debug, Clone)]
pub struct MeetingRef {
    pub id: String,
    pub title: String,
    /// "Oct 14"
    pub date: String,
}

pub fn prompt(question: &str, excerpts: &[(&Passage, &MeetingRef)]) -> Prompt {
    let mut user = String::from("Excerpts:\n\n");
    for (n, (p, m)) in excerpts.iter().enumerate() {
        let when = p.at.map(|a| format!(" · {}", stamp(a))).unwrap_or_default();
        user.push_str(&format!("[{}] {} · {}{}\n{}\n\n", n + 1, m.title, m.date, when, p.text));
    }
    user.push_str(&format!("Question: {}", question.trim()));
    Prompt { system: SYSTEM.into(), user }
}

#[derive(Deserialize)]
struct Draft {
    #[serde(default)]
    answer: String,
    #[serde(default)]
    points: Vec<DraftPoint>,
}

#[derive(Deserialize)]
struct DraftPoint {
    #[serde(default)]
    text: String,
    #[serde(default)]
    source: i64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    pub meeting_id: String,
    pub meeting_title: String,
    pub date: String,
    pub at: u64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Point {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citation: Option<Citation>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AskAnswer {
    pub text: String,
    pub points: Vec<Point>,
    pub from_meetings: usize,
    pub total_meetings: usize,
}

impl AskAnswer {
    pub fn nothing(total_meetings: usize) -> Self {
        AskAnswer { text: String::new(), points: vec![], from_meetings: 0, total_meetings }
    }
}

/// The line inside a transcript passage that best matches the point, so the citation lands on it.
fn best_line(p: &Passage, point: &str) -> Option<u64> {
    let want: BTreeSet<String> = super::search::terms(point).into_iter().collect();
    let score = |t: &str| super::search::terms(t).into_iter().filter(|w| want.contains(w)).collect::<BTreeSet<_>>().len();
    let best = p.lines.iter().map(|(at, t)| (score(t), *at)).max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))?;
    Some(if best.0 > 0 { best.1 } else { p.lines[0].0 })
}

pub fn parse(reply: &str, excerpts: &[(&Passage, &MeetingRef)], total_meetings: usize) -> Result<AskAnswer, String> {
    let json = crate::summarizer::parse::extract_json(reply).ok_or("the reply had no JSON object")?;
    let d: Draft = serde_json::from_str(json).map_err(|e| format!("the reply wasn't valid JSON ({e})"))?;
    let mut cited = BTreeSet::new();
    let points: Vec<Point> = d
        .points
        .into_iter()
        .filter(|p| !p.text.trim().is_empty())
        .map(|p| {
            let source = usize::try_from(p.source).ok().filter(|s| (1..=excerpts.len()).contains(s)).map(|s| excerpts[s - 1]);
            let citation = source.and_then(|(passage, m)| {
                let at = if passage.lines.is_empty() { passage.at? } else { best_line(passage, &p.text)? };
                cited.insert(m.id.clone());
                Some(Citation { meeting_id: m.id.clone(), meeting_title: m.title.clone(), date: m.date.clone(), at })
            });
            Point { text: p.text.trim().to_string(), citation }
        })
        .collect();
    Ok(AskAnswer { text: d.answer.trim().to_string(), points, from_meetings: cited.len(), total_meetings })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Vec<Passage>, Vec<MeetingRef>) {
        let p = vec![
            Passage {
                meeting_id: "a".into(),
                at: Some(10_000),
                text: "Me: Hi.\nSpeaker 1: Pricing moves to Q1.".into(),
                lines: vec![(10_000, "Me: Hi.".into()), (14_000, "Speaker 1: Pricing moves to Q1.".into())],
            },
            Passage { meeting_id: "b".into(), at: None, text: "Summary: Launch planning.".into(), lines: vec![] },
            Passage { meeting_id: "b".into(), at: Some(36_000), text: "Decision: Fallback date is the 20th".into(), lines: vec![] },
        ];
        let m = vec![MeetingRef { id: "a".into(), title: "Roadmap".into(), date: "Oct 14".into() }, MeetingRef { id: "b".into(), title: "Launch".into(), date: "Oct 12".into() }];
        (p, m)
    }

    #[test]
    fn the_prompt_numbers_excerpts_with_meeting_and_time() {
        let (p, m) = fixture();
        let ex = vec![(&p[0], &m[0]), (&p[1], &m[1])];
        let pr = prompt("  What about pricing? ", &ex);
        assert!(pr.user.contains("[1] Roadmap · Oct 14 · 00:10\nMe: Hi."));
        assert!(pr.user.contains("[2] Launch · Oct 12\nSummary: Launch planning."));
        assert!(pr.user.ends_with("Question: What about pricing?"));
    }

    #[test]
    fn points_cite_the_matching_line_and_count_meetings() {
        let (p, m) = fixture();
        let ex = vec![(&p[0], &m[0]), (&p[1], &m[1]), (&p[2], &m[1])];
        let reply = r#"{"answer":"Pricing moved to Q1.","points":[{"text":"Pricing moves to Q1","source":1},{"text":"Fallback is the 20th","source":3},{"text":"Made up","source":9},{"text":"Recap only","source":2}]}"#;
        let a = parse(reply, &ex, 7).unwrap();
        assert_eq!(a.text, "Pricing moved to Q1.");
        assert_eq!(a.points[0].citation.as_ref().unwrap().at, 14_000);
        assert_eq!(a.points[1].citation.as_ref().unwrap().at, 36_000);
        assert_eq!(a.points[1].citation.as_ref().unwrap().meeting_title, "Launch");
        // A source that doesn't exist, or the recap (no time), keeps the point without a citation.
        assert!(a.points[2].citation.is_none());
        assert!(a.points[3].citation.is_none());
        assert_eq!((a.from_meetings, a.total_meetings), (2, 7));
    }

    #[test]
    fn an_empty_answer_is_nothing_found() {
        let (p, m) = fixture();
        let a = parse(r#"{"answer":"","points":[]}"#, &[(&p[0], &m[0])], 3).unwrap();
        assert_eq!(a, AskAnswer::nothing(3));
        assert!(parse("no json here", &[], 1).is_err());
    }
}
