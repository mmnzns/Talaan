//! The summary engines. Request bodies and reply parsing are pure functions (tested with recorded shapes);
//! the network call is one small function. Only transcript text is ever sent, never audio.

use super::local::LocalAi;
use crate::i18n::tr_with;
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
    Groq,
    Cerebras,
    Mistral,
    /// One key for many companies' models.
    OpenRouter,
    /// Ollama, LM Studio, or any OpenAI-compatible server.
    Compatible,
}

/// The cloud models offered in Settings for each provider, the same set Parlato supports. The first one is
/// what "Recommended" uses when the key can reach it. Names for the menu live in src/data/byokModels.ts.
pub fn catalog(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::Claude => &["claude-sonnet-5-5", "claude-opus-5-5", "claude-haiku-5-5", "claude-fable-5-1"],
        Provider::OpenAi => &["gpt-6-sol", "gpt-6.1-sol", "gpt-6-luna", "gpt-6-astra", "gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna", "gpt-5.5", "gpt-5.4", "gpt-5.4-mini"],
        Provider::Gemini => &["gemini-3.8-flash", "gemini-3.1-pro-preview", "gemini-3.6-flash", "gemini-3.5-flash-lite", "gemini-3.1-flash-lite"],
        Provider::Groq => &["openai/gpt-oss-120b", "openai/gpt-oss-20b", "qwen/qwen3.8-27b"],
        Provider::Cerebras => &["gpt-oss-120b", "qwen-3.8-27b"],
        Provider::Mistral => &["mistral-medium-latest", "mistral-large-latest", "mistral-small-latest", "ministral-14b-latest", "ministral-8b-latest"],
        Provider::OpenRouter => &[
            "anthropic/claude-sonnet-5.5",
            "anthropic/claude-opus-5.5",
            "anthropic/claude-haiku-5.5",
            "openai/gpt-6-sol",
            "openai/gpt-6-luna",
            "openai/gpt-oss-120b",
            "google/gemini-3.8-flash",
            "google/gemini-3.1-pro-preview",
            "deepseek/deepseek-v4.1-flash",
            "qwen/qwen3.8-flash",
            "mistralai/mistral-medium-3-5",
            "x-ai/grok-4.7",
            "meta-llama/llama-4-maverick",
        ],
        Provider::Compatible => &[],
    }
}

impl Provider {
    pub const ALL: [Provider; 8] = [Provider::Claude, Provider::OpenAi, Provider::Gemini, Provider::Groq, Provider::Cerebras, Provider::Mistral, Provider::OpenRouter, Provider::Compatible];

    pub fn parse(s: &str) -> Option<Provider> {
        Some(match s {
            "claude" => Provider::Claude,
            "openai" => Provider::OpenAi,
            "gemini" => Provider::Gemini,
            "groq" => Provider::Groq,
            "cerebras" => Provider::Cerebras,
            "mistral" => Provider::Mistral,
            "openrouter" => Provider::OpenRouter,
            "compatible" => Provider::Compatible,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Provider::Claude => "Claude",
            Provider::OpenAi => "OpenAI",
            Provider::Gemini => "Gemini",
            Provider::Groq => "Groq",
            Provider::Cerebras => "Cerebras",
            Provider::Mistral => "Mistral",
            Provider::OpenRouter => "OpenRouter",
            Provider::Compatible => "OpenAI-compatible",
        }
    }

    /// Keychain account name.
    pub fn key_id(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::OpenAi => "openai",
            Provider::Gemini => "gemini",
            Provider::Groq => "groq",
            Provider::Cerebras => "cerebras",
            Provider::Mistral => "mistral",
            Provider::OpenRouter => "openrouter",
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
    fn failure_detail(&mut self) -> Option<String> {
        self.ai.take_detail()
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

/// A schema without some keywords: Gemini's dialect has no `additionalProperties`, and the fast open-model
/// hosts refuse `maxItems` in strict mode (the prompt still asks for short lists).
fn without(v: &Value, keys: &[&str]) -> Value {
    match v {
        Value::Object(m) => Value::Object(m.iter().filter(|(k, _)| !keys.contains(&k.as_str())).map(|(k, v)| (k.clone(), without(v, keys))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(|v| without(v, keys)).collect()),
        other => other.clone(),
    }
}

/// Gemini 3 thinking level, as Parlato sets it: a little for 3.8 Flash and the Pro models, minimal for the rest.
fn gemini_thinking(model: &str) -> Option<&'static str> {
    let m = model.to_lowercase();
    if !m.starts_with("gemini-3") {
        return None;
    }
    Some(if m.starts_with("gemini-3.8-flash") || m.contains("-pro") { "low" } else { "minimal" })
}

/// The request body for a chat-completions provider, tuned per model the way Parlato does it: the token
/// limit field each host accepts, reasoning kept short (and out of the reply), and no temperature for the
/// models that refuse one.
fn chat_body(provider: Provider, model: &str, p: &Prompt, schema: &Value, max_tokens: u32) -> Value {
    let m = model.to_lowercase();
    let oss = m.contains("gpt-oss");
    let qwen = m.contains("qwen");
    let mut body = json!({
        "model": model,
        "messages": [{ "role": "system", "content": p.system }, { "role": "user", "content": p.user }],
    });
    match provider {
        Provider::OpenAi => {
            // Reasoning models refuse `max_tokens` and any temperature but their own.
            body["max_completion_tokens"] = json!(max_tokens);
            if m.starts_with("gpt-6-astra") || m.starts_with("gpt-6.1-sol") {
                body["reasoning_effort"] = json!("low");
            } else if m.starts_with("gpt-6") || m.starts_with("gpt-5.") {
                body["reasoning_effort"] = json!("none");
            }
        }
        Provider::Groq | Provider::Cerebras => {
            body["max_completion_tokens"] = json!(max_tokens);
            body["temperature"] = json!(0.2);
            if oss {
                body["reasoning_effort"] = json!("low");
                if provider == Provider::Groq {
                    body["include_reasoning"] = json!(false);
                } else {
                    body["reasoning_format"] = json!("hidden");
                }
            } else if qwen {
                body["reasoning_effort"] = json!("none");
            }
        }
        // OpenRouter passes the request on; several of its models (Claude, GPT-6) refuse a temperature.
        Provider::OpenRouter => body["max_tokens"] = json!(max_tokens),
        _ => {
            body["max_tokens"] = json!(max_tokens);
            body["temperature"] = json!(0.2);
        }
    }
    let (schema, strict) = match provider {
        Provider::OpenAi | Provider::Compatible => (schema.clone(), true),
        // Groq holds to a schema strictly only on its GPT-OSS models; elsewhere it's best effort.
        Provider::Groq => (without(schema, &["maxItems"]), oss),
        _ => (without(schema, &["maxItems"]), true),
    };
    body["response_format"] = json!({ "type": "json_schema", "json_schema": { "name": "meeting_summary", "strict": strict, "schema": schema } });
    body
}

impl HttpEngine {
    pub fn request(&self, p: &Prompt, schema: &str, max_tokens: u32) -> HttpRequest {
        let schema: Value = serde_json::from_str(schema).expect("schema is valid JSON");
        let key = self.key.clone().unwrap_or_default();
        match self.provider {
            Provider::Claude => HttpRequest {
                url: format!("{}/messages", self.base),
                headers: vec![("x-api-key".into(), key), ("anthropic-version".into(), "2023-06-01".into())],
                // No temperature: the current Claude models set their own.
                body: json!({
                    "model": self.model,
                    "max_tokens": max_tokens,
                    "system": format!("{}\n\nThe JSON schema:\n{}", p.system, schema),
                    "messages": [{ "role": "user", "content": p.user }],
                }),
            },
            Provider::Gemini => {
                let mut config = json!({ "temperature": 0.2, "maxOutputTokens": max_tokens, "responseMimeType": "application/json", "responseSchema": without(&schema, &["additionalProperties"]) });
                if let Some(level) = gemini_thinking(&self.model) {
                    config["thinkingConfig"] = json!({ "thinkingLevel": level });
                }
                HttpRequest {
                    url: format!("{}/models/{}:generateContent", self.base, self.model),
                    headers: vec![("x-goog-api-key".into(), key)],
                    body: json!({
                        "systemInstruction": { "parts": [{ "text": p.system }] },
                        "contents": [{ "role": "user", "parts": [{ "text": p.user }] }],
                        "generationConfig": config,
                    }),
                }
            }
            _ => HttpRequest {
                url: format!("{}/chat/completions", self.base),
                headers: auth_headers(self.provider, &key),
                body: chat_body(self.provider, &self.model, p, &schema, max_tokens),
            },
        }
    }

    /// The reply text from a provider's response body.
    pub fn reply_text(&self, body: &Value) -> Result<String, EngineError> {
        let text = match self.provider {
            Provider::Claude => body["content"].as_array().and_then(|c| c.iter().find_map(|b| b["text"].as_str())),
            Provider::Gemini => body["candidates"][0]["content"]["parts"][0]["text"].as_str(),
            _ => body["choices"][0]["message"]["content"].as_str(),
        };
        text.map(str::to_string).ok_or_else(|| EngineError::Failed(tr_with("ai.empty_reply", &[("provider", self.provider.name())])))
    }
}

/// Turns an HTTP error into words: the provider's own message when it gives one.
pub fn http_error(provider: Provider, status: u16, body: &Value) -> EngineError {
    let detail = body["error"]["message"].as_str().or_else(|| body["error"].as_str()).or_else(|| body["message"].as_str()).unwrap_or("");
    match status {
        401 | 403 => EngineError::Unavailable(tr_with("ai.key_rejected", &[("provider", provider.name())])),
        404 => EngineError::Unavailable(tr_with("ai.no_such_model", &[("provider", provider.name()), ("detail", detail)]).trim().to_string()),
        429 => EngineError::Failed(tr_with("ai.rate_limited", &[("provider", provider.name()), ("detail", detail)]).trim().to_string()),
        _ => EngineError::Failed(tr_with("ai.http_error", &[("provider", provider.name()), ("status", &status.to_string()), ("detail", detail)]).trim().to_string()),
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
            EngineError::Unavailable(tr_with("ai.unreachable", &[("provider", provider.name()), ("url", url)]))
        } else if e.is_timeout() {
            EngineError::Failed(tr_with("ai.too_slow", &[("provider", provider.name())]))
        } else {
            EngineError::Failed(tr_with("ai.failed", &[("provider", provider.name()), ("error", &e.to_string())]))
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

/// The model Talaan uses when the user hasn't picked one: the first of the provider's list that the key can
/// use, else the newest of its fast, capable tier among what the key offers (names change faster than releases).
pub fn recommended_model(provider: Provider, ids: &[String]) -> Option<String> {
    if let Some(m) = catalog(provider).iter().find(|m| ids.iter().any(|id| id == *m)) {
        return Some(m.to_string());
    }
    let fits = |id: &str| -> bool {
        let id = id.to_lowercase();
        match provider {
            Provider::Claude => id.starts_with("claude-sonnet"),
            Provider::OpenAi => id.starts_with("gpt-") && id.ends_with("-mini") && !id.contains("audio") && !id.contains("realtime") && !id.contains("search"),
            Provider::Gemini => id.starts_with("gemini-") && id.contains("flash") && !id.contains("lite") && !id.contains("image") && !id.contains("tts") && !id.contains("live") && !id.contains("exp") && !id.contains("preview"),
            Provider::Groq | Provider::Cerebras | Provider::OpenRouter => id.contains("gpt-oss-120b"),
            Provider::Mistral => id.starts_with("mistral-medium"),
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
        Provider::Groq => "https://api.groq.com/openai/v1",
        Provider::Cerebras => "https://api.cerebras.ai/v1",
        Provider::Mistral => "https://api.mistral.ai/v1",
        Provider::OpenRouter => "https://openrouter.ai/api/v1",
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
    fn every_provider_has_its_address_key_and_models() {
        let menu = include_str!("../../../src/data/byokModels.ts");
        for (id, base) in [("groq", "https://api.groq.com/openai/v1"), ("cerebras", "https://api.cerebras.ai/v1"), ("mistral", "https://api.mistral.ai/v1"), ("openrouter", "https://openrouter.ai/api/v1")] {
            let p = Provider::parse(id).unwrap();
            assert_eq!(p.key_id(), id);
            assert_eq!(default_base(p), base);
        }
        for p in ["claude", "openai", "gemini", "groq", "cerebras", "mistral", "openrouter"].map(|p| Provider::parse(p).unwrap()) {
            assert!(!catalog(p).is_empty());
            // The Settings menu offers the same models, in the same order (its first is "Recommended").
            let mut at = 0;
            for m in catalog(p) {
                let found = menu[at..].find(&format!("id: \"{m}\"")).unwrap_or_else(|| panic!("{m} missing or out of order in byokModels.ts"));
                at += found;
            }
        }
    }

    fn chat(provider: Provider, model: &str) -> Value {
        HttpEngine { model: model.into(), ..engine(provider) }.request(&prompt(), SCHEMA, 1500).body
    }

    #[test]
    fn requests_are_tuned_per_model_like_parlato() {
        // Claude: no temperature.
        assert!(chat(Provider::Claude, "claude-sonnet-5-5").get("temperature").is_none());
        // OpenAI: the reasoning models' token field, no temperature, short reasoning.
        let b = chat(Provider::OpenAi, "gpt-6-sol");
        assert_eq!((b["max_completion_tokens"].as_u64(), b.get("max_tokens"), b.get("temperature")), (Some(1500), None, None));
        assert_eq!(b["reasoning_effort"], "none");
        assert_eq!(chat(Provider::OpenAi, "gpt-6-astra")["reasoning_effort"], "low");
        assert_eq!(chat(Provider::OpenAi, "gpt-6.1-sol")["reasoning_effort"], "low");
        assert_eq!(chat(Provider::OpenAi, "gpt-5.4-mini")["reasoning_effort"], "none");
        assert!(chat(Provider::OpenAi, "gpt-4o").get("reasoning_effort").is_none());
        // Groq and Cerebras keep GPT-OSS reasoning short and out of the reply; Qwen doesn't reason.
        let g = chat(Provider::Groq, "openai/gpt-oss-120b");
        assert_eq!((g["reasoning_effort"].as_str(), g["include_reasoning"].as_bool()), (Some("low"), Some(false)));
        assert_eq!(g["response_format"]["json_schema"]["strict"], true);
        let q = chat(Provider::Groq, "qwen/qwen3.8-27b");
        assert_eq!(q["reasoning_effort"], "none");
        assert_eq!(q["response_format"]["json_schema"]["strict"], false);
        let c = chat(Provider::Cerebras, "gpt-oss-120b");
        assert_eq!((c["reasoning_effort"].as_str(), c["reasoning_format"].as_str()), (Some("low"), Some("hidden")));
        assert!(!c.to_string().contains("maxItems"));
        assert_eq!(chat(Provider::Cerebras, "qwen-3.8-27b")["reasoning_effort"], "none");
        // Mistral: the plain chat fields. OpenRouter: no temperature (it forwards to Claude, GPT-6...).
        let m = chat(Provider::Mistral, "mistral-medium-latest");
        assert_eq!((m["max_tokens"].as_u64(), m["temperature"].as_f64()), (Some(1500), Some(0.2)));
        let o = HttpEngine { model: "anthropic/claude-sonnet-5.5".into(), ..engine(Provider::OpenRouter) }.request(&prompt(), SCHEMA, 1500);
        assert_eq!(o.url, "https://openrouter.ai/api/v1/chat/completions");
        assert_eq!(o.headers, vec![("Authorization".to_string(), "Bearer sk-test".to_string())]);
        assert!(o.body.get("temperature").is_none());
        assert_eq!(engine(Provider::Groq).reply_text(&json!({ "choices": [{ "message": { "content": "{}" } }] })).unwrap(), "{}");
        // Gemini 3: a little thinking for 3.8 Flash and Pro, minimal for the rest, none for older models.
        let level = |m: &str| chat(Provider::Gemini, m)["generationConfig"]["thinkingConfig"]["thinkingLevel"].as_str().map(str::to_string);
        assert_eq!(level("gemini-3.8-flash").as_deref(), Some("low"));
        assert_eq!(level("gemini-3.1-pro-preview").as_deref(), Some("low"));
        assert_eq!(level("gemini-3.5-flash-lite").as_deref(), Some("minimal"));
        assert_eq!(level("gemini-2.5-flash"), None);
    }

    #[test]
    fn recommended_prefers_the_listed_default_the_key_can_use() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(recommended_model(Provider::OpenAi, &ids(&["gpt-5.4-mini", "gpt-6-sol", "gpt-6-luna"])).as_deref(), Some("gpt-6-sol"));
        assert_eq!(recommended_model(Provider::Groq, &ids(&["llama-3.3-70b", "openai/gpt-oss-20b"])).as_deref(), Some("openai/gpt-oss-20b"));
        assert_eq!(recommended_model(Provider::Mistral, &ids(&["mistral-small-latest", "mistral-medium-latest"])).as_deref(), Some("mistral-medium-latest"));
        assert_eq!(recommended_model(Provider::OpenRouter, &ids(&["x-ai/grok-4.7", "anthropic/claude-sonnet-5.5"])).as_deref(), Some("anthropic/claude-sonnet-5.5"));
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

    /// The whole HTTP path against a fake OpenAI-compatible server: request shape, auth, reply, parsing.
    #[test]
    fn a_summary_round_trips_through_an_openai_compatible_server() {
        use crate::summarizer::prompt::tests::{line, meeting};
        use crate::summarizer::summarize::summarize;
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://{}/v1", server.server_addr().to_ip().unwrap());
        let seen = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let seen2 = seen.clone();
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                let mut body = String::new();
                req.as_reader().read_to_string(&mut body).unwrap();
                let auth = req.headers().iter().find(|h| h.field.equiv("Authorization")).map(|h| h.value.to_string());
                seen2.lock().unwrap().push((req.url().to_string(), auth, body));
                let content = r#"{\"title\":\"Onboarding plan\",\"recap\":\"We chose onboarding first.\",\"decisions\":[{\"text\":\"Pricing moves to Q1.\",\"time\":\"01:00\"}],\"action_items\":[],\"open_questions\":[]}"#;
                let reply = format!(r#"{{"choices":[{{"message":{{"role":"assistant","content":"{content}"}}}}]}}"#);
                let _ = req.respond(tiny_http::Response::from_string(reply).with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()));
            }
        });
        let mut e = HttpEngine { provider: Provider::Compatible, base, key: Some("sk-local".into()), model: "qwen".into(), context: LOCAL_CONTEXT, label: "Test server".into() };
        let m = meeting(vec![line("l0", 55_000, "me", "Where did we land on pricing?"), line("l1", 60_000, "them:1", "Pricing moves to Q1."), line("l2", 66_000, "me", "Good, onboarding ships first then.")]);
        let out = summarize(&m, &mut e, "en", &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(out.summary.recap, "We chose onboarding first.");
        assert_eq!(out.summary.decisions[0].at, Some(60_000));
        assert_eq!(out.summary.engine, "Test server");
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].0, "/v1/chat/completions");
        assert_eq!(seen[0].1.as_deref(), Some("Bearer sk-local"));
        assert!(seen[0].2.contains("Dana Reyes: Pricing moves to Q1."));
        // Only text goes out: no audio, no file paths.
        assert!(!seen[0].2.contains(".wav"));
    }
}
