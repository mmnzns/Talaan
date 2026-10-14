//! Long calls: when a transcript doesn't fit the engine's context, it is split on speaker turns into parts
//! that do, each part is summarized, and a last call merges the part summaries.

use crate::storage::model::TranscriptLine;

/// Rough token count for budgeting: about 3.5 characters per token for English and other Latin scripts,
/// plus a few tokens per line for the "[mm:ss] Name:" prefix. Errs on the high side.
pub fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() as f64 / 3.5).ceil() as usize
}

pub fn line_tokens(l: &TranscriptLine) -> usize {
    estimate_tokens(&l.text) + 8
}

/// Splits lines into consecutive parts of at most `budget` tokens. Splits only between lines, and prefers a
/// change of speaker (a new turn) over cutting someone's run of lines in two. A single line longer than the
/// budget becomes its own part (the engine call will report if it truly can't fit).
pub fn split(lines: &[TranscriptLine], budget: usize) -> Vec<&[TranscriptLine]> {
    let mut parts = vec![];
    let mut start = 0;
    while start < lines.len() {
        let mut used = 0;
        let mut end = start;
        while end < lines.len() && (end == start || used + line_tokens(&lines[end]) <= budget) {
            used += line_tokens(&lines[end]);
            end += 1;
        }
        if end < lines.len() {
            // Back off to the last speaker change in the second half of the part, if there is one.
            let min = start + (end - start) / 2;
            if let Some(turn) = (min.max(start + 1)..end).rev().find(|&i| lines[i].speaker != lines[i - 1].speaker) {
                end = turn;
            }
        }
        parts.push(&lines[start..end]);
        start = end;
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summarizer::prompt::tests::line;

    fn lines(n: usize, words: usize) -> Vec<TranscriptLine> {
        (0..n).map(|i| line(&format!("l{i}"), i as u64 * 10_000, if (i / 3) % 2 == 0 { "me" } else { "them:0" }, &"word ".repeat(words))).collect()
    }

    #[test]
    fn a_short_call_is_one_part() {
        let l = lines(10, 20);
        let parts = split(&l, 10_000);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].len(), 10);
    }

    #[test]
    fn a_long_call_is_split_into_parts_that_fit_and_cover_everything() {
        let l = lines(300, 40); // ~66 tokens a line
        let budget = 2_000;
        let parts = split(&l, budget);
        assert!(parts.len() >= 10);
        assert!(parts.iter().all(|p| p.iter().map(line_tokens).sum::<usize>() <= budget));
        let ids: Vec<&str> = parts.iter().flat_map(|p| p.iter().map(|l| l.id.as_str())).collect();
        assert_eq!(ids, l.iter().map(|l| l.id.as_str()).collect::<Vec<_>>());
    }

    #[test]
    fn parts_end_on_a_change_of_speaker() {
        let l = lines(300, 40);
        for p in split(&l, 2_000).iter().take_while(|p| p.last().unwrap().id != l.last().unwrap().id) {
            let last = p.last().unwrap();
            let next_index: usize = last.id[1..].parse::<usize>().unwrap() + 1;
            assert_ne!(last.speaker, l[next_index].speaker, "part ends mid-turn at {}", last.id);
        }
    }

    #[test]
    fn one_huge_line_still_makes_progress() {
        let mut l = lines(3, 10);
        l[1].text = "word ".repeat(5_000);
        let parts = split(&l, 500);
        assert_eq!(parts.iter().map(|p| p.len()).sum::<usize>(), 3);
    }
}
