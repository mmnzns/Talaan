//! Messages between Talaan and its local AI helper (`talaan-llm`). One JSON object per line on stdin
//! (requests) and stdout (events). The helper runs one request at a time.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Answers with `Event::Ready`. Used to check the helper started.
    Ping,
    /// Generates one reply. The model stays loaded for the next request with the same path and context size.
    Complete(Completion),
    /// Turns texts into embedding vectors (L2-normalized), for Ask's search index.
    Embed(Embedding),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Embedding {
    pub model_path: String,
    pub texts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Completion {
    pub model_path: String,
    /// Context size in tokens (prompt + reply).
    pub n_ctx: u32,
    pub system: String,
    pub user: String,
    /// When set, the reply is constrained to JSON matching this schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<String>,
    pub max_tokens: u32,
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Ready,
    /// Reading the prompt (`done` of `total` tokens), then writing (`done` tokens written, `total` 0).
    Progress { phase: Phase, done: u32, total: u32 },
    Done(Reply),
    /// One vector per input text, in order.
    Embedded { vectors: Vec<Vec<f32>> },
    Error { message: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Loading,
    Reading,
    Writing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Reply {
    pub text: String,
    pub prompt_tokens: u32,
    pub output_tokens: u32,
    pub seconds: f32,
    /// True when the reply stopped at `max_tokens` instead of finishing.
    pub truncated: bool,
}

pub fn encode<T: Serialize>(msg: &T) -> String {
    serde_json::to_string(msg).expect("protocol messages always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_events_round_trip_as_single_lines() {
        let req = Request::Complete(Completion {
            model_path: "/m.gguf".into(),
            n_ctx: 8192,
            system: "s".into(),
            user: "line one\nline two".into(),
            json_schema: Some("{}".into()),
            max_tokens: 100,
            temperature: 0.2,
        });
        let line = encode(&req);
        assert!(!line.contains('\n'));
        assert!(line.contains("\"op\":\"complete\""));
        assert_eq!(serde_json::from_str::<Request>(&line).unwrap(), req);
        let ev = Event::Progress { phase: Phase::Reading, done: 5, total: 10 };
        assert_eq!(serde_json::from_str::<Event>(&encode(&ev)).unwrap(), ev);
        assert_eq!(encode(&Request::Ping), r#"{"op":"ping"}"#);
    }
}
