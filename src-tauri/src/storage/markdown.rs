//! The plain-text copies written beside the JSON: `transcript.md` and `summary.md`.

use super::model::{Summary, TranscriptLine};
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

pub fn summary_md(title: &str, s: &Summary) -> String {
    let at = |t: Option<u64>| t.map(|t| format!(" ({})", format_time(t))).unwrap_or_default();
    let mut out = format!("# {title}\n\n{}\n", s.recap);
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
            engine: "Local AI".into(),
            edited: false,
        };
        let md = summary_md("Sync", &s);
        assert!(md.contains("Recap text."));
        assert!(md.contains("- Ship it (00:21)"));
        assert!(md.contains("- [x] Send wireframes — Me, due Fri"));
        assert!(!md.contains("Open questions"));
    }
}
