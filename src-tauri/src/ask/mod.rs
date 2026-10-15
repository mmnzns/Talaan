//! Ask: questions answered from the user's own meetings, with citations to the exact moment.
//!
//! Each meeting is cut into passages (transcript windows and summary items). A question is matched against
//! them two ways: keywords (BM25, always available) and meaning (on-device embeddings, once the search model
//! is downloaded), fused by rank. The best passages go to the user's summary engine with one fixed prompt; its
//! reply names the passages it used, which become clickable citations.

pub mod answer;
pub mod index;
pub mod passages;
pub mod search;

/// Qwen3-Embedding is instruction-aware: questions get a task line, passages are embedded as they are.
pub fn query_text(question: &str) -> String {
    format!("Instruct: Given a question about the user's meetings, retrieve the meeting passages that answer it\nQuery:{}", question.trim())
}

#[cfg(test)]
mod tests {
    /// Real search model through the helper. Build the helper first (`cargo build -p talaan-llm`), then
    /// `cargo test embed_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn embed_smoke() {
        let model = dirs::data_dir().unwrap().join("Talaan/models/search/Qwen3-Embedding-0.6B-Q8_0.gguf");
        let exe = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join(crate::summarizer::local::helper_file());
        let mut ai = crate::summarizer::local::LocalAi::new(&exe);
        let passages = [
            "Speaker 1: The numbers are final as of this morning. Daniel, can you chase legal for the copy?",
            "Me: I'll own the announcement email. Draft by next Wednesday.",
            "Speaker 2: The landing page is done. We still need the pricing table before we can publish.",
            "Speaker 1: Ang deadline ng proposal ay sa Biyernes, kailangan nating tapusin ang budget.",
        ];
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let t = std::time::Instant::now();
        let docs = ai.embed(&model, passages.iter().map(|s| s.to_string()).collect(), &cancel).unwrap();
        eprintln!("{} passages, {} dims, {:.2}s", docs.len(), docs[0].len(), t.elapsed().as_secs_f32());
        for (q, want) in [("Who is writing the launch email?", 1), ("What's left before the website goes live?", 2), ("Who talks to the lawyers?", 0), ("When is the proposal due?", 3)] {
            let v = ai.embed(&model, vec![super::query_text(q)], &cancel).unwrap().remove(0);
            let scores: Vec<f32> = docs.iter().map(|d| super::search::cosine(&v, d)).collect();
            let best = (0..scores.len()).max_by(|&a, &b| scores[a].total_cmp(&scores[b])).unwrap();
            eprintln!("{q:<45} -> {best} {scores:.2?}");
            assert_eq!(best, want, "{q}");
        }
    }
}
