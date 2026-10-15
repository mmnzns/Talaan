//! Ranking passages for a question: keywords (BM25) and meaning (cosine over embeddings), fused by rank.

use crate::transcriber::echo::words;
use std::collections::HashMap;

/// Words too common to say anything about what a passage is about.
const STOP: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "of", "to", "in", "on", "at", "for", "with", "about", "from", "by", "as", "is", "are", "was", "were",
    "be", "been", "it", "it's", "this", "that", "these", "those", "i", "you", "we", "they", "he", "she", "me", "my", "our", "your", "their", "do",
    "did", "does", "what", "which", "who", "when", "where", "why", "how", "so", "if", "then", "there", "here", "have", "has", "had", "will",
    "would", "can", "could", "should", "just", "like", "yeah", "okay", "ok", "um", "uh", "not", "no", "yes", "all", "any", "some", "get", "got",
];

pub fn terms(text: &str) -> Vec<String> {
    words(text).into_iter().filter(|w| !STOP.contains(&w.as_str())).collect()
}

/// BM25 scores of every document for the query (0 = no shared term).
pub fn bm25(query: &[String], docs: &[Vec<String>]) -> Vec<f32> {
    const K1: f32 = 1.2;
    const B: f32 = 0.75;
    let n = docs.len() as f32;
    if docs.is_empty() || query.is_empty() {
        return vec![0.0; docs.len()];
    }
    let avg = docs.iter().map(|d| d.len()).sum::<usize>() as f32 / n;
    let mut df: HashMap<&str, f32> = HashMap::new();
    for d in docs {
        let mut seen: Vec<&str> = d.iter().map(String::as_str).collect();
        seen.sort_unstable();
        seen.dedup();
        for t in seen {
            *df.entry(t).or_default() += 1.0;
        }
    }
    let mut unique: Vec<&String> = query.iter().collect();
    unique.sort();
    unique.dedup();
    docs.iter()
        .map(|d| {
            let len = d.len() as f32;
            unique
                .iter()
                .map(|q| {
                    let tf = d.iter().filter(|t| t == q).count() as f32;
                    if tf == 0.0 {
                        return 0.0;
                    }
                    let df = df.get(q.as_str()).copied().unwrap_or(0.0);
                    let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
                    idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * len / avg.max(1.0)))
                })
                .sum()
        })
        .collect()
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    // Vectors are stored normalized.
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Below this similarity a passage isn't considered related by meaning at all.
const MIN_COSINE: f32 = 0.3;
/// Reciprocal rank fusion constant (the usual 60).
const RRF_K: f32 = 60.0;

/// Indices of the best passages, best first. `vectors[i]` is passage i's embedding when indexed.
pub fn rank(keyword: &[f32], query_vec: Option<&[f32]>, vectors: &[Option<&[f32]>], limit: usize) -> Vec<usize> {
    let mut fused: HashMap<usize, f32> = HashMap::new();
    let mut by_kw: Vec<usize> = (0..keyword.len()).filter(|&i| keyword[i] > 0.0).collect();
    by_kw.sort_by(|&a, &b| keyword[b].total_cmp(&keyword[a]));
    for (r, i) in by_kw.into_iter().enumerate() {
        *fused.entry(i).or_default() += 1.0 / (RRF_K + r as f32 + 1.0);
    }
    if let Some(q) = query_vec {
        let mut by_vec: Vec<(usize, f32)> = vectors.iter().enumerate().filter_map(|(i, v)| v.map(|v| (i, cosine(q, v)))).filter(|(_, s)| *s >= MIN_COSINE).collect();
        by_vec.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (r, (i, _)) in by_vec.into_iter().take(limit * 4).enumerate() {
            *fused.entry(i).or_default() += 1.0 / (RRF_K + r as f32 + 1.0);
        }
    }
    let mut out: Vec<(usize, f32)> = fused.into_iter().collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    out.into_iter().take(limit).map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docs(texts: &[&str]) -> Vec<Vec<String>> {
        texts.iter().map(|t| terms(t)).collect()
    }

    #[test]
    fn terms_drop_filler_and_case() {
        assert_eq!(terms("So what did we decide about Pricing?"), vec!["decide", "pricing"]);
    }

    #[test]
    fn bm25_prefers_rarer_and_repeated_terms() {
        let d = docs(&["pricing pricing moved to Q1", "the onboarding plan", "pricing review with legal", "onboarding onboarding ships first"]);
        let s = bm25(&terms("pricing legal"), &d);
        assert!(s[2] > s[0] && s[0] > 0.0, "{s:?}");
        assert_eq!(s[1], 0.0);
    }

    #[test]
    fn rank_fuses_keywords_and_meaning() {
        // Passage 2 shares no word with the question but means the same; passage 0 matches by keyword only.
        let kw = vec![1.0, 0.0, 0.0];
        let q = [1.0, 0.0];
        let v0 = [0.0, 1.0];
        let v2 = [0.95, 0.31];
        let both = rank(&kw, Some(&q), &[Some(&v0), None, Some(&v2)], 5);
        assert_eq!(both.len(), 2);
        assert!(both.contains(&0) && both.contains(&2));
        // Without vectors, keywords alone.
        assert_eq!(rank(&kw, None, &[None, None, None], 5), vec![0]);
    }

    #[test]
    fn a_passage_found_both_ways_wins() {
        let kw = vec![2.0, 1.0];
        let q = [1.0, 0.0];
        let r = rank(&kw, Some(&q), &[Some(&[0.5, 0.86]), Some(&[1.0, 0.0])], 5);
        // 0: kw rank 1 + vec rank 2; 1: kw rank 2 + vec rank 1. Tie on score: lower index first.
        assert_eq!(r, vec![0, 1]);
        let r = rank(&[0.0, 1.0], Some(&q), &[Some(&[0.5, 0.86]), Some(&[1.0, 0.0])], 5);
        assert_eq!(r[0], 1);
    }
}
