//! Turning the two tracks' timed pieces into one transcript: echo removal, then interleaving by time and
//! joining a speaker's consecutive sentences into readable lines.

use crate::storage::model::TranscriptLine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Track {
    Me,
    Them,
}

/// One timed sentence on the recording clock.
#[derive(Debug, Clone, PartialEq)]
pub struct Timed {
    pub track: Track,
    /// Who on the Them track (1, 2, 3 from diarization); 0 = not split.
    pub speaker: u8,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// Same speaker, gap under this: one line.
const JOIN_GAP_MS: u64 = 1_200;
/// A line never grows past this, so timestamps stay useful for jumping around.
const MAX_LINE_MS: u64 = 40_000;

pub fn merge(mut pieces: Vec<Timed>) -> Vec<TranscriptLine> {
    pieces.sort_by_key(|p| (p.start_ms, p.track == Track::Them));
    let mut lines: Vec<Timed> = vec![];
    for p in pieces {
        if let Some(last) = lines.last_mut() {
            let same = last.track == p.track && last.speaker == p.speaker;
            let close = p.start_ms <= last.end_ms + JOIN_GAP_MS;
            let short = p.end_ms.saturating_sub(last.start_ms) <= MAX_LINE_MS;
            if same && close && short {
                last.end_ms = last.end_ms.max(p.end_ms);
                last.text = format!("{} {}", last.text.trim_end(), p.text.trim_start());
                continue;
            }
        }
        lines.push(p);
    }
    lines
        .into_iter()
        .enumerate()
        .map(|(i, l)| TranscriptLine {
            id: format!("l{i}"),
            at: l.start_ms,
            end: l.end_ms.max(l.start_ms),
            speaker: match l.track {
                Track::Me => "me".into(),
                Track::Them => format!("them:{}", l.speaker),
            },
            text: l.text,
            edited: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn t(track: Track, s: f32, e: f32, text: &str) -> Timed {
        Timed { track, speaker: 0, start_ms: (s * 1000.0) as u64, end_ms: (e * 1000.0) as u64, text: text.into() }
    }

    #[test]
    fn interleaves_by_time_and_joins_a_speakers_sentences() {
        let lines = merge(vec![
            t(Track::Them, 5.0, 7.0, "How was the trip?"),
            t(Track::Me, 0.0, 2.0, "Hi."),
            t(Track::Me, 2.5, 4.0, "Can you hear me?"),
            t(Track::Me, 8.0, 9.0, "Good, thanks."),
            t(Track::Them, 7.5, 8.5, "Mine was long."),
        ]);
        let got: Vec<_> = lines.iter().map(|l| (l.speaker.as_str(), l.at, l.text.as_str())).collect();
        assert_eq!(
            got,
            vec![
                ("me", 0, "Hi. Can you hear me?"),
                ("them:0", 5000, "How was the trip? Mine was long."),
                ("me", 8000, "Good, thanks."),
            ]
        );
        assert_eq!(lines.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(), vec!["l0", "l1", "l2"]);
        assert_eq!(lines[1].end, 8500);
    }

    #[test]
    fn different_people_on_the_them_track_get_their_own_lines() {
        let mut a = t(Track::Them, 0.0, 2.0, "Over to you.");
        a.speaker = 1;
        let mut b = t(Track::Them, 2.3, 4.0, "Thanks.");
        b.speaker = 2;
        let lines = merge(vec![a, b]);
        assert_eq!(lines.iter().map(|l| l.speaker.as_str()).collect::<Vec<_>>(), vec!["them:1", "them:2"]);
    }

    #[test]
    fn a_pause_starts_a_new_line() {
        let lines = merge(vec![t(Track::Me, 0.0, 2.0, "One."), t(Track::Me, 3.5, 4.0, "Two.")]);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn long_monologues_are_broken_up() {
        let pieces = (0..10).map(|i| t(Track::Them, i as f32 * 6.0, i as f32 * 6.0 + 5.5, "Sentence.")).collect();
        let lines = merge(pieces);
        assert!(lines.len() >= 2);
        assert!(lines.iter().all(|l| l.end - l.at <= MAX_LINE_MS));
    }
}
