//! The plain-text copies written beside the JSON: `transcript.md` and `summary.md`.

use super::model::{Mark, Summary, TranscriptLine};
use std::collections::BTreeMap;

/// 754000 -> "12:34"; 3_725_000 -> "1:02:05"
pub fn format_time(ms: u64) -> String {
    let total = ms / 1000;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn speaker_name<'a>(speakers: &'a BTreeMap<String, String>, key: &'a str) -> &'a str {
    speakers.get(key).map(String::as_str).unwrap_or(if key == "me" { "Me" } else { "Them" })
}

pub fn transcript_md(title: &str, lines: &[TranscriptLine], speakers: &BTreeMap<String, String>) -> String {
    let mut out = format!("# {title}\n\n");
    for l in lines {
        out.push_str(&format!("**{} {}:** {}\n\n", format_time(l.at), speaker_name(speakers, &l.speaker), l.text));
    }
    out
}

/// The user's notes as a "Your notes" list (empty when there are none).
fn notes_list(marks: &[Mark]) -> String {
    let notes: Vec<&Mark> = marks.iter().filter(|m| m.note_text().is_some()).collect();
    let mut out = String::new();
    if !notes.is_empty() {
        out.push_str("\n## Your notes\n\n");
        for n in notes {
            let when = if n.later { String::new() } else { format!(" ({})", format_time(n.at)) };
            out.push_str(&format!("- {}{when}\n", n.note_text().unwrap_or_default()));
        }
    }
    out
}

/// `summary.md` before there is a summary: the user's notes alone, or None when there are none either.
pub fn notes_md(title: &str, marks: &[Mark]) -> Option<String> {
    let list = notes_list(marks);
    (!list.is_empty()).then(|| format!("# {title}\n{list}"))
}

/// `marks`: the meeting's marks; the ones with text are the user's notes, listed under "Your notes".
pub fn summary_md(title: &str, s: &Summary, marks: &[Mark]) -> String {
    let at = |t: Option<u64>| t.map(|t| format!(" ({})", format_time(t))).unwrap_or_default();
    let mut out = format!("# {title}\n\n{}\n", s.recap);
    out.push_str(&notes_list(marks));
    if !s.decisions.is_empty() {
        out.push_str("\n## Decisions\n\n");
        for d in &s.decisions {
            out.push_str(&format!("- {}{}\n", d.text, at(d.at)));
        }
    }
    if !s.action_items.is_empty() {
        out.push_str("\n## Action items\n\n");
        for a in &s.action_items {
            let owner = a.owner.as_ref().map(|o| format!(" — {o}")).unwrap_or_default();
            let due = a.due.as_ref().map(|d| format!(", due {d}")).unwrap_or_default();
            out.push_str(&format!("- [{}] {}{owner}{due}{}\n", if a.done { "x" } else { " " }, a.text, at(a.at)));
        }
    }
    if !s.open_questions.is_empty() {
        out.push_str("\n## Open questions\n\n");
        for q in &s.open_questions {
            out.push_str(&format!("- {}\n", q.text));
        }
    }
    for sec in &s.sections {
        out.push_str(&format!("\n## {}\n\n", sec.title));
        for i in &sec.items {
            out.push_str(&format!("- {}{}\n", i.text, at(i.at)));
        }
    }
    if !s.highlights.is_empty() {
        out.push_str("\n## Highlights\n\n");
        for h in &s.highlights {
            out.push_str(&format!("- {}{}\n", h.text, at(h.at)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::model::{ActionItem, SummaryItem};

    #[test]
    fn times_format_like_the_ui() {
        assert_eq!(format_time(0), "00:00");
        assert_eq!(format_time(754_000), "12:34");
        assert_eq!(format_time(3_725_000), "1:02:05");
    }

    #[test]
    fn transcript_uses_speaker_names() {
        let speakers = BTreeMap::from([("them:1".to_string(), "Dana Reyes".to_string())]);
        let lines = vec![
            TranscriptLine { id: "a".into(), at: 6000, end: 9000, speaker: "me".into(), text: "Hi".into(), edited: false },
            TranscriptLine { id: "b".into(), at: 12000, end: 15000, speaker: "them:1".into(), text: "Hello".into(), edited: false },
        ];
        let md = transcript_md("Sync", &lines, &speakers);
        assert!(md.starts_with("# Sync\n"));
        assert!(md.contains("**00:06 Me:** Hi"));
        assert!(md.contains("**00:12 Dana Reyes:** Hello"));
    }

    #[test]
    fn summary_lists_sections_and_skips_empty_ones() {
        let s = Summary {
            recap: "Recap text.".into(),
            decisions: vec![SummaryItem { id: "d".into(), text: "Ship it".into(), at: Some(21_000) }],
            action_items: vec![ActionItem { id: "a".into(), text: "Send wireframes".into(), at: None, owner: Some("Me".into()), due: Some("Fri".into()), done: true }],
            open_questions: vec![],
            sections: vec![],
            highlights: vec![],
            template: None,
            engine: "Local AI".into(),
            edited: false,
        };
        let md = summary_md("Sync", &s, &[]);
        assert!(md.contains("Recap text."));
        assert!(!md.contains("Your notes") && !md.contains("Highlights"));
        assert!(md.contains("- Ship it (00:21)"));
        assert!(md.contains("- [x] Send wireframes — Me, due Fri"));
        assert!(!md.contains("Open questions"));
    }

    #[test]
    fn notes_sections_and_highlights_are_listed() {
        use crate::storage::model::{SummaryItem, SummarySection};
        let s = Summary {
            recap: "R.".into(),
            decisions: vec![],
            action_items: vec![],
            open_questions: vec![],
            sections: vec![SummarySection { id: "s0".into(), title: "Blockers".into(), items: vec![SummaryItem { id: "s0i0".into(), text: "Dana: waiting on legal".into(), at: Some(65_000) }] }],
            highlights: vec![SummaryItem { id: "h0".into(), text: "Budget set at 50k.".into(), at: Some(120_000) }],
            template: Some("standup".into()),
            engine: "x".into(),
            edited: false,
        };
        let marks = vec![
            Mark { id: "k0".into(), at: 5_000, note: None, later: false },
            Mark { id: "k1".into(), at: 61_000, note: Some("ask about legal".into()), later: false },
            Mark { id: "n0".into(), at: 0, note: Some("send the deck".into()), later: true },
        ];
        let md = summary_md("Stand-up", &s, &marks);
        assert!(md.contains("## Your notes\n\n- ask about legal (01:01)\n- send the deck\n"));
        assert!(md.contains("## Blockers\n\n- Dana: waiting on legal (01:05)"));
        assert!(md.contains("## Highlights\n\n- Budget set at 50k. (02:00)"));
        // Before there is a summary, the notes alone.
        assert_eq!(notes_md("Stand-up", &marks).unwrap(), "# Stand-up\n\n## Your notes\n\n- ask about legal (01:01)\n- send the deck\n");
        assert_eq!(notes_md("Stand-up", &marks[..1]), None);
    }
}
