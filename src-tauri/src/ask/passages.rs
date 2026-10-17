//! Cutting a meeting into searchable passages.

use crate::storage::model::Meeting;
use crate::summarizer::prompt::speaker_name;

/// A transcript window closes once it has this much text or spans this long.
const WINDOW_CHARS: usize = 700;
const WINDOW_MS: u64 = 90_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Passage {
    pub meeting_id: String,
    /// Where it starts in the recording; None for the recap.
    pub at: Option<u64>,
    /// "Speaker: text" lines, or one summary item.
    pub text: String,
    /// Transcript passages: each line's start, so a citation can point at the line it came from.
    pub lines: Vec<(u64, String)>,
}

pub fn passages(m: &Meeting) -> Vec<Passage> {
    let id = &m.item.id;
    let mut out = vec![];
    let mut cur: Vec<(u64, String)> = vec![];
    let mut chars = 0;
    let flush = |cur: &mut Vec<(u64, String)>, out: &mut Vec<Passage>| {
        if cur.is_empty() {
            return;
        }
        out.push(Passage {
            meeting_id: id.clone(),
            at: Some(cur[0].0),
            text: cur.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n"),
            lines: cur.clone(),
        });
        // Overlap by one line so an answer split across a boundary is still found whole.
        let last = cur.pop().expect("not empty");
        cur.clear();
        cur.push(last);
    };
    for l in &m.transcript {
        let text = l.text.trim();
        if text.is_empty() {
            continue;
        }
        let line = format!("{}: {}", speaker_name(m, &l.speaker), text);
        let too_long = cur.first().is_some_and(|(start, _)| l.at.saturating_sub(*start) > WINDOW_MS);
        if (chars >= WINDOW_CHARS || too_long) && cur.len() > 1 {
            flush(&mut cur, &mut out);
            chars = cur.iter().map(|(_, t)| t.len()).sum();
        }
        chars += line.len();
        cur.push((l.at, line));
    }
    // The last window, unless it's only the overlap line already in the previous one.
    if cur.len() > 1 || out.is_empty() {
        let last = cur.clone();
        if !last.is_empty() {
            out.push(Passage { meeting_id: id.clone(), at: Some(last[0].0), text: last.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n"), lines: last });
        }
    }

    if let Some(s) = &m.summary {
        let item = |at: Option<u64>, text: String| Passage { meeting_id: id.clone(), at, text, lines: vec![] };
        if !s.recap.trim().is_empty() {
            out.push(item(None, format!("Summary: {}", s.recap.trim())));
        }
        for d in &s.decisions {
            out.push(item(d.at, format!("Decision: {}", d.text.trim())));
        }
        for a in &s.action_items {
            let mut who = vec![];
            if let Some(o) = a.owner.as_deref().filter(|o| !o.trim().is_empty()) {
                who.push(o.trim().to_string());
            }
            if let Some(d) = a.due.as_deref().filter(|d| !d.trim().is_empty()) {
                who.push(format!("due {}", d.trim()));
            }
            let who = if who.is_empty() { String::new() } else { format!(" ({})", who.join(", ")) };
            out.push(item(a.at, format!("Action item{who}: {}", a.text.trim())));
        }
        for q in &s.open_questions {
            out.push(item(q.at, format!("Open question: {}", q.text.trim())));
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::storage::model::{ActionItem, Summary, SummaryItem, TranscriptLine};

    pub fn meeting(id: &str, lines: &[(u64, &str, &str)]) -> Meeting {
        let mut m: Meeting = serde_json::from_str(include_str!("../../../fixtures/meeting.sample.json")).unwrap();
        m.item.id = id.into();
        m.speakers.clear();
        m.summary = None;
        m.transcript = lines
            .iter()
            .enumerate()
            .map(|(i, (at, who, text))| TranscriptLine { id: format!("l{i}"), at: *at, end: at + 1000, speaker: (*who).into(), text: (*text).into(), edited: false })
            .collect();
        m
    }

    #[test]
    fn a_short_call_is_one_passage_with_speaker_names() {
        let p = passages(&meeting("a", &[(0, "me", "Hi."), (2000, "them:1", "Hello there.")]));
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].text, "Me: Hi.\nSpeaker 1: Hello there.");
        assert_eq!(p[0].at, Some(0));
        assert_eq!(p[0].lines.len(), 2);
    }

    #[test]
    fn long_calls_are_windowed_with_one_line_of_overlap() {
        let lines: Vec<(u64, &str, &str)> = (0..40).map(|i| (i * 10_000, "them:0", "We talked about the onboarding plan for a while longer.")).collect();
        let p = passages(&meeting("a", &lines));
        assert!(p.len() > 3, "{}", p.len());
        for w in p.windows(2) {
            assert_eq!(w[0].lines.last(), w[1].lines.first(), "overlap");
        }
        assert!(p.iter().all(|x| x.lines.last().unwrap().0 - x.lines[0].0 <= WINDOW_MS + 10_000));
        // Every line is in some passage.
        let covered: std::collections::BTreeSet<u64> = p.iter().flat_map(|x| x.lines.iter().map(|l| l.0)).collect();
        assert_eq!(covered.len(), 40);
    }

    #[test]
    fn summary_items_become_their_own_passages() {
        let mut m = meeting("a", &[(0, "me", "Hi.")]);
        m.summary = Some(Summary {
            recap: "We planned the launch.".into(),
            decisions: vec![SummaryItem { id: "d0".into(), text: "Launch on the 20th".into(), at: Some(36_000) }],
            action_items: vec![ActionItem { id: "a0".into(), text: "Email legal".into(), at: Some(24_000), owner: Some("Daniel".into()), due: Some("Monday".into()), done: false }],
            open_questions: vec![],
            sections: vec![],
            highlights: vec![],
            template: None,
            engine: "x".into(),
            edited: false,
        });
        let texts: Vec<_> = passages(&m).into_iter().map(|p| (p.at, p.text)).collect();
        assert!(texts.contains(&(None, "Summary: We planned the launch.".into())));
        assert!(texts.contains(&(Some(36_000), "Decision: Launch on the 20th".into())));
        assert!(texts.contains(&(Some(24_000), "Action item (Daniel, due Monday): Email legal".into())));
    }

    #[test]
    fn an_empty_meeting_has_no_passages() {
        assert!(passages(&meeting("a", &[])).is_empty());
    }
}
