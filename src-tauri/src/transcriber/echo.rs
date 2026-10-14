//! Echo removal. Without headphones the mic hears the other people through the speakers, so their words
//! show up on the Me track too, at about the same time. A Me sentence is dropped when most of its words were
//! also said on the Them track within a couple of seconds. Real speech that overlaps (both talking) uses
//! different words and stays. A Me sentence that is only partly echo stays: losing real words is worse.

use super::merge::{Timed, Track};

/// How far apart in time an echo and its source may be.
const WINDOW_MS: u64 = 2_000;
/// Share of the Me sentence's words that must appear on the Them track.
const MIN_SHARE: f64 = 0.6;
/// Two words count as the same when they are this similar (ASR spells echoes slightly differently).
const WORD_SIMILARITY: f64 = 0.8;
/// Sentences this short ("yeah", "okay") are common on both sides for real, so they need a tighter match.
const SHORT_WORDS: usize = 2;
const SHORT_WINDOW_MS: u64 = 700;

pub fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|w| w.trim_matches('\'').to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

fn same_word(a: &str, b: &str) -> bool {
    a == b || (a.len() >= 4 && b.len() >= 4 && strsim::normalized_levenshtein(a, b) >= WORD_SIMILARITY)
}

/// Returns the pieces without echoed Me sentences, and how many were removed.
pub fn remove_echo(pieces: Vec<Timed>) -> (Vec<Timed>, usize) {
    let them: Vec<(u64, u64, Vec<String>)> = pieces.iter().filter(|p| p.track == Track::Them).map(|p| (p.start_ms, p.end_ms, words(&p.text))).collect();
    let mut removed = 0;
    let kept = pieces
        .into_iter()
        .filter(|p| {
            if p.track != Track::Me {
                return true;
            }
            let mine = words(&p.text);
            if mine.is_empty() {
                return true;
            }
            let window = if mine.len() <= SHORT_WORDS { SHORT_WINDOW_MS } else { WINDOW_MS };
            let nearby: Vec<&String> = them
                .iter()
                .filter(|(s, e, _)| *s <= p.end_ms + window && p.start_ms <= e + window)
                .flat_map(|(_, _, w)| w.iter())
                .collect();
            if nearby.is_empty() {
                return true;
            }
            let matched = mine.iter().filter(|w| nearby.iter().any(|n| same_word(w, n))).count();
            let share = matched as f64 / mine.len() as f64;
            let echo = if mine.len() <= SHORT_WORDS { share >= 1.0 } else { share >= MIN_SHARE };
            if echo {
                removed += 1;
            }
            !echo
        })
        .collect();
    (kept, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(track: Track, s: f32, e: f32, text: &str) -> Timed {
        Timed { track, start_ms: (s * 1000.0) as u64, end_ms: (e * 1000.0) as u64, text: text.into() }
    }

    fn kept_me(pieces: Vec<Timed>) -> Vec<String> {
        remove_echo(pieces).0.into_iter().filter(|p| p.track == Track::Me).map(|p| p.text).collect()
    }

    #[test]
    fn an_exact_echo_is_removed() {
        let (out, n) = remove_echo(vec![
            t(Track::Them, 10.0, 14.0, "We should ship the onboarding first."),
            t(Track::Me, 10.1, 14.2, "We should ship the onboarding first."),
        ]);
        assert_eq!(n, 1);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].track, Track::Them);
    }

    #[test]
    fn an_echo_heard_differently_is_still_removed() {
        // The mic hears the speakers worse, so the engine gets some words wrong.
        assert!(kept_me(vec![
            t(Track::Them, 36.0, 45.0, "So we're testing the app, it's saying that we should record about ten minutes of the interview."),
            t(Track::Me, 36.3, 45.5, "So we testing the app saying that we should record about ten minute of uh the in the view."),
        ])
        .is_empty());
    }

    #[test]
    fn real_speech_at_the_same_time_stays() {
        assert_eq!(
            kept_me(vec![t(Track::Them, 5.0, 9.0, "The budget review moved to Thursday."), t(Track::Me, 6.0, 8.0, "Sorry, can I jump in quickly?")]),
            vec!["Sorry, can I jump in quickly?"]
        );
    }

    #[test]
    fn the_same_words_much_later_are_not_echo() {
        // Repeating what someone said ("so ship onboarding first") is real speech.
        assert_eq!(
            kept_me(vec![t(Track::Them, 5.0, 8.0, "We should ship onboarding first."), t(Track::Me, 20.0, 23.0, "We should ship onboarding first.")]),
            vec!["We should ship onboarding first."]
        );
    }

    #[test]
    fn a_reply_that_reuses_some_words_stays() {
        assert_eq!(
            kept_me(vec![
                t(Track::Them, 5.0, 8.0, "Can you send the wireframes by Friday?"),
                t(Track::Me, 8.5, 11.0, "Yes, I'll send them Thursday night actually."),
            ]),
            vec!["Yes, I'll send them Thursday night actually."]
        );
    }

    #[test]
    fn short_replies_need_a_tight_match() {
        // "Yeah" said a second after theirs is a reply; "Yeah" at the same moment is the echo.
        assert_eq!(kept_me(vec![t(Track::Them, 5.0, 5.5, "Yeah."), t(Track::Me, 6.5, 7.0, "Yeah.")]), vec!["Yeah."]);
        assert!(kept_me(vec![t(Track::Them, 5.0, 5.5, "Yeah."), t(Track::Me, 5.05, 5.6, "Yeah.")]).is_empty());
    }

    #[test]
    fn them_lines_are_never_touched() {
        let (out, n) = remove_echo(vec![t(Track::Them, 1.0, 2.0, "Hello."), t(Track::Them, 1.0, 2.0, "Hello.")]);
        assert_eq!((out.len(), n), (2, 0));
    }

    #[test]
    fn words_ignore_case_and_punctuation() {
        assert_eq!(words("It's 10, OK? Ten!"), vec!["it's", "10", "ok", "ten"]);
    }
}
