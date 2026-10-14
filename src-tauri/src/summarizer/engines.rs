//! The summary engines. Request bodies and reply parsing are pure functions (tested with recorded shapes);
//! the network call is one small function. Only transcript text is ever sent, never audio.

use super::local::LocalAi;
use super::prompt::Prompt;
use super::summarize::{EngineError, SummaryEngine};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use talaan_llm_protocol::Completion;

/// Context used with local models and local servers: room for ~45 minutes of talk per part on a laptop.
pub const LOCAL_CONTEXT: usize = 16_384;
/// Cloud models take far more; this keeps a single request (and its cost) reasonable.
pub const CLOUD_CONTEXT: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Claude,
    OpenAi,
    Gemini,
    /// Ollama, LM Studio, or any OpenAI-compatible server.
    Compatible,
}

impl Provider {
    pub fn parse(s: &str) -> Option<Provider> {
        Some(match s {
            "claude" => Provider::Claude,
            "openai" => Provider::OpenAi,
            "gemini" => Provider::Gemini,
            "compatible" => Provider::Compatible,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Provider::Claude => "Claude",
            Provider::OpenAi => "OpenAI",
            Provider::Gemini => "Gemini",
            Provider::Compatible => "OpenAI-compatible",
        }
    }

    /// Keychain account name.
    pub fn key_id(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::OpenAi => "openai",
            Provider::Gemini => "gemini",
            Provider::Compatible => "compatible",
        }
    }
}

// ---------- local model through the helper ----------

pub struct LocalEngine {
    pub ai: LocalAi,
    pub model_path: PathBuf,
    pub model_name: String,
}

impl SummaryEngine for LocalEngine {
    fn complete(&mut self, p: &Prompt, schema: &str, max_tokens: u32, cancel: &AtomicBool) -> Result<String, EngineError> {
        let c = Completion {
            model_path: self.model_path.to_string_lossy().into(),
            n_ctx: LOCAL_CONTEXT as u32,
            system: p.system.clone(),
            user: p.user.clone(),
            json_schema: Some(schema.into()),
            max_tokens,
            temperature: 0.2,
        };
        match self.ai.complete(&c, cancel, &mut |_, _, _| {}) {
            Ok(r) => Ok(r.text),
            Err(e) if e == "cancelled" => Err(EngineError::Cancelled),
            Err(e) => Err(EngineError::Failed(e)),
        }
    }
    fn context_tokens(&self) -> usize {
        LOCAL_CONTEXT
    }
    fn label(&self) -> String {
        format!("Local AI · {}", self.model_name)
    }
}

// ---------- HTTP engines ----------

pub struct HttpEngine {
    pub provider: Provider,
    /// API root: `https://api.openai.com/v1`, `http://localhost:11434/v1`, ...
    pub base: String,
    pub key: Option<String>,
    pub model: String,
    pub context: usize,
    /// "Claude", "Ollama · llama3.2" ...
    pub label: String,
}

pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

/// Gemini's schema dialect has no `additionalProperties` / `maxItems` on all versions: strip them.
fn gemini_schema(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(m.iter().filter(|(k, _)| k.as_str() != "additionalProperties").map(|(k, v)| (k.clone(), gemini_schema(v))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(gemini_schema).collect()),
        other => other.clone(),
    }
}

impl HttpEngine {
    pub fn request(&self, p: &Prompt, schema: &str, max_tokens: u32) -> HttpRequest {
        let schema: Value = serde_json::from_str(schema).expect("schema is valid JSON");
        let key = self.key.clone().unwrap_or_default();
        match self.provider {
            Provider::Claude => HttpRequest {
                url: format!("{}/messages", self.base),
                headers: vec![("x-api-key".into(), key), ("anthropic-version".into(), "2023-06-01".into())],
                body: json!({
                    "model": self.model,
                    "max_tokens": max_tokens,
                    "temperature": 0.2,
                    "system": format!("{}\n\nThe JSON schema:\n{}", p.system, schema),
                    "messages": [{ "role": "user", "content": p.user }],
                }),
            },
            Provider::Gemini => HttpRequest {
                url: format!("{}/models/{}:generateContent", self.base, self.model),
                headers: vec![("x-goog-api-key".into(), key)],
                body: json!({
                    "systemInstruction": { "parts": [{ "text": p.system }] },
                    "contents": [{ "role": "user", "parts": [{ "text": p.user }] }],
                    "generationConfig": { "temperature": 0.2, "maxOutputTokens": max_tokens, "responseMimeType": "application/json", "responseSchema": gemini_schema(&schema) },
                }),
            },
            Provider::OpenAi | Provider::Compatible => {
                let mut headers = vec![];
                if !key.is_empty() {
                    headers.push(("Authorization".into(), format!("Bearer {key}")));
                }
                HttpRequest {
                    url: format!("{}/chat/completions", self.base),
                    headers,
                    body: json!({
                        "model": self.model,
                        "temperature": 0.2,
                        "max_tokens": max_tokens,
                        "messages": [{ "role": "system", "content": p.system }, { "role": "user", "content": p.user }],
                        "response_format": { "type": "json_schema", "json_schema": { "name": "meeting_summary", "strict": true, "schema": schema } },
                    }),
                }
            }
        }
    }

    /// The reply text from a provider's response body.
    pub fn reply_text(&self, body: &Value) -> Result<String, EngineError> {
        let text = match self.provider {
            Provider::Claude => body["content"].as_array().and_then(|c| c.iter().find_map(|b| b["text"].as_str())),
            Provider::Gemini => body["candidates"][0]["content"]["parts"][0]["text"].as_str(),
            Provider::OpenAi | Provider::Compatible => body["choices"][0]["message"]["content"].as_str(),
        };
        text.map(str::to_string).ok_or_else(|| EngineError::Failed(format!("{} sent an empty reply", self.provider.name())))
    }
}

/// Turns an HTTP error into words: the provider's own message when it gives one.
pub fn http_error(provider: Provider, status: u16, body: &Value) -> EngineError {
    let detail = body["error"]["message"].as_str().or_else(|| body["error"].as_str()).or_else(|| body["message"].as_str()).unwrap_or("");
    match status {
        401 | 403 => EngineError::Unavailable(format!("{} didn't accept the API key. Check it in Settings › Summary AI.", provider.name())),
        404 => EngineError::Unavailable(format!("{} doesn't have that model. Pick another in Settings › Summary AI. {detail}", provider.name()).trim().to_string()),
        429 => EngineError::Failed(format!("{} is rate limiting or out of credit. Try again later. {detail}", provider.name()).trim().to_string()),
        _ => EngineError::Failed(format!("{} returned an error ({status}). {detail}", provider.name()).trim().to_string()),
    }
}

fn client(timeout: Duration) -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("Talaan/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .build()
        .expect("http client")
}

fn send(provider: Provider, method: reqwest::Method, url: &str, headers: &[(String, String)], body: Option<&Value>, timeout: Duration) -> Result<Value, EngineError> {
    let mut req = client(timeout).request(method, url);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    if let Some(b) = body {
        req = req.json(b);
    }
    let resp = req.send().map_err(|e| {
        if e.is_connect() {
            EngineError::Unavailable(format!("Couldn't reach {} at {url}. Is it running, and is this Mac online?", provider.name()))
        } else if e.is_timeout() {
            EngineError::Failed(format!("{} took too long to answer.", provider.name()))
        } else {
            EngineError::Failed(format!("Couldn't reach {}: {e}", provider.name()))
        }
    })?;
    let status = resp.status().as_u16();
    let body: Value = resp.json().unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        return Err(http_error(provider, status, &body));
    }
    Ok(body)
}

impl SummaryEngine for HttpEngine {
    fn complete(&mut self, p: &Prompt, schema: &str, max_tokens: u32, _cancel: &AtomicBool) -> Result<String, EngineError> {
        let r = self.request(p, schema, max_tokens);
        let body = send(self.provider, reqwest::Method::POST, &r.url, &r.headers, Some(&r.body), Duration::from_secs(600))?;
        self.reply_text(&body)
    }
    fn context_tokens(&self) -> usize {
        self.context
    }
    fn label(&self) -> String {
        self.label.clone()
    }
}

// ---------- model lists (Settings: Test button, local server model picker) ----------

pub fn auth_headers(provider: Provider, key: &str) -> Vec<(String, String)> {
    match provider {
        Provider::Claude => vec![("x-api-key".into(), key.into()), ("anthropic-version".into(), "2023-06-01".into())],
        Provider::Gemini => vec![("x-goog-api-key".into(), key.into())],
        _ if key.is_empty() => vec![],
        _ => vec![("Authorization".into(), format!("Bearer {key}"))],
    }
}

/// Model ids from a provider's model list response.
pub fn model_ids(provider: Provider, body: &Value) -> Vec<String> {
    let list = match provider {
        Provider::Gemini => &body["models"],
        _ => &body["data"],
    };
    list.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| m["id"].as_str().or_else(|| m["name"].as_str()))
                .map(|id| id.strip_prefix("models/").unwrap_or(id).to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Version numbers in a model id, for picking the newest: "gpt-5.1-mini" → [5, 1].
fn version(id: &str) -> Vec<u32> {
    id.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty() && s.len() < 4).filter_map(|s| s.parse().ok()).collect()
}

/// The model Talaan uses when the user hasn't picked one: the newest of the provider's fast, capable tier,
/// chosen from what the key can actually use (model names change too often to hard-code).
pub fn recommended_model(provider: Provider, ids: &[String]) -> Option<String> {
    let fits = |id: &str| -> bool {
        let id = id.to_lowercase();
        match provider {
            Provider::Claude => id.starts_with("claude-sonnet"),
            Provider::OpenAi => id.starts_with("gpt-") && id.ends_with("-mini") && !id.contains("audio") && !id.contains("realtime") && !id.contains("search"),
            Provider::Gemini => id.starts_with("gemini-") && id.contains("flash") && !id.contains("lite") && !id.contains("image") && !id.contains("tts") && !id.contains("live") && !id.contains("exp") && !id.contains("preview"),
            Provider::Compatible => !id.contains("embed"),
        }
    };
    let mut ok: Vec<&String> = ids.iter().filter(|id| fits(id)).collect();
    if provider == Provider::Compatible {
        return ok.first().map(|s| s.to_string());
    }
    ok.sort_by_key(|id| version(id));
    ok.last().map(|s| s.to_string())
}

/// Lists the models a key (or a local server) offers. Used by the Test button.
pub fn list_models(provider: Provider, base: &str, key: &str) -> Result<Vec<String>, EngineError> {
    let body = send(provider, reqwest::Method::GET, &format!("{base}/models"), &auth_headers(provider, key), None, Duration::from_secs(20))?;
    Ok(model_ids(provider, &body))
}

pub fn default_base(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "https://api.anthropic.com/v1",
        Provider::OpenAi => "https://api.openai.com/v1",
        Provider::Gemini => "https://generativelanguage.googleapis.com/v1beta",
        Provider::Compatible => "http://localhost:11434/v1",
    }
}

/// "http://localhost:11434" or ".../v1/" → "http://localhost:11434/v1".
pub fn compatible_base(address: &str) -> String {
    let a = address.trim().trim_end_matches('/');
    let a = if a.contains("://") { a.to_string() } else { format!("http://{a}") };
    if a.ends_with("/v1") { a } else { format!("{a}/v1") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summarizer::prompt::SCHEMA;

    fn engine(provider: Provider) -> HttpEngine {
        HttpEngine { provider, base: default_base(provider).into(), key: Some("sk-test".into()), model: "m-1".into(), context: CLOUD_CONTEXT, label: provider.name().into() }
    }

    fn prompt() -> Prompt {
        Prompt { system: "Rules.".into(), user: "Transcript.".into() }
    }

    #[test]
    fn claude_request_and_reply() {
        let e = engine(Provider::Claude);
        let r = e.request(&prompt(), SCHEMA, 1500);
        assert_eq!(r.url, "https://api.anthropic.com/v1/messages");
        assert!(r.headers.contains(&("x-api-key".into(), "sk-test".into())));
        assert_eq!(r.body["messages"][0]["content"], "Transcript.");
        assert!(r.body["system"].as_str().unwrap().contains("\"recap\""));
        assert_eq!(e.reply_text(&json!({ "content": [{ "type": "text", "text": "{\"recap\":\"x\"}" }] })).unwrap(), "{\"recap\":\"x\"}");
    }

    #[test]
    fn openai_and_local_servers_use_a_strict_schema() {
        let e = engine(Provider::OpenAi);
        let r = e.request(&prompt(), SCHEMA, 1500);
        assert_eq!(r.url, "https://api.openai.com/v1/chat/completions");
        assert_eq!(r.headers, vec![("Authorization".to_string(), "Bearer sk-test".to_string())]);
        assert_eq!(r.body["response_format"]["json_schema"]["strict"], true);
        assert_eq!(r.body["messages"][0]["role"], "system");
        assert_eq!(e.reply_text(&json!({ "choices": [{ "message": { "content": "hi" } }] })).unwrap(), "hi");
        // A local server without a key sends no auth header.
        let local = HttpEngine { key: None, ..engine(Provider::Compatible) };
        assert!(local.request(&prompt(), SCHEMA, 10).headers.is_empty());
    }

    #[test]
    fn gemini_gets_its_schema_dialect_and_key_in_a_header() {
        let r = engine(Provider::Gemini).request(&prompt(), SCHEMA, 1500);
        assert_eq!(r.url, "https://generativelanguage.googleapis.com/v1beta/models/m-1:generateContent");
        assert!(!r.url.contains("sk-test"));
        assert!(!r.body.to_string().contains("additionalProperties"));
        assert_eq!(r.body["generationConfig"]["responseMimeType"], "application/json");
        let reply = json!({ "candidates": [{ "content": { "parts": [{ "text": "{}" }] } }] });
        assert_eq!(engine(Provider::Gemini).reply_text(&reply).unwrap(), "{}");
    }

    #[test]
    fn empty_replies_and_http_errors_read_plainly() {
        assert!(matches!(engine(Provider::OpenAi).reply_text(&json!({ "choices": [] })), Err(EngineError::Failed(_))));
        assert!(matches!(http_error(Provider::Claude, 401, &Value::Null), EngineError::Unavailable(m) if m.contains("didn't accept the API key")));
        let e = http_error(Provider::OpenAi, 429, &json!({ "error": { "message": "You exceeded your quota." } }));
        assert_eq!(e, EngineError::Failed("OpenAI is rate limiting or out of credit. Try again later. You exceeded your quota.".into()));
    }

    #[test]
    fn the_newest_fast_model_is_recommended_from_what_the_key_offers() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(recommended_model(Provider::OpenAi, &ids(&["gpt-4o", "gpt-4.1-mini", "gpt-5-mini", "gpt-5-mini-audio", "text-embedding-3"])).as_deref(), Some("gpt-5-mini"));
        assert_eq!(recommended_model(Provider::Claude, &ids(&["claude-haiku-5-5", "claude-sonnet-4-6", "claude-sonnet-5-5", "claude-opus-5-5"])).as_deref(), Some("claude-sonnet-5-5"));
        assert_eq!(recommended_model(Provider::Gemini, &ids(&["gemini-2.5-flash", "gemini-3-flash", "gemini-3-flash-lite", "gemini-3-pro"])).as_deref(), Some("gemini-3-flash"));
        assert_eq!(recommended_model(Provider::OpenAi, &ids(&["whisper-1"])), None);
    }

    #[test]
    fn model_lists_parse_for_each_provider() {
        assert_eq!(model_ids(Provider::Gemini, &json!({ "models": [{ "name": "models/gemini-3-flash" }] })), vec!["gemini-3-flash"]);
        assert_eq!(model_ids(Provider::Compatible, &json!({ "data": [{ "id": "llama3.2:3b" }] })), vec!["llama3.2:3b"]);
    }

    #[test]
    fn local_server_addresses_are_normalized() {
        assert_eq!(compatible_base("http://localhost:11434"), "http://localhost:11434/v1");
        assert_eq!(compatible_base("localhost:1234/v1/"), "http://localhost:1234/v1");
    }
}
