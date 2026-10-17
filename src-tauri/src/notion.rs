//! Notion export. Talaan has no server, so there's no public OAuth (it needs a client secret): the user makes an
//! internal integration in Notion, shares the pages or databases it may write to, and pastes its secret, which is
//! kept in secure key storage like API keys. Nothing is sent unless the user clicks Export.

use crate::i18n::{tr, tr_err, tr_with};
use crate::storage::model::Meeting;
use crate::summarizer::prompt::{speaker_name, stamp};
use serde::Serialize;
use serde_json::{json, Value};
use std::time::Duration;

pub const API: &str = "https://api.notion.com";
/// Pinned so the request shapes below keep working when Notion changes its defaults.
const VERSION: &str = "2022-06-28";
/// Notion's limits: 100 blocks per request, 2000 characters per text item.
const BLOCKS_PER_CALL: usize = 100;
const TEXT_MAX: usize = 2000;
pub const KEY_ID: &str = "notion";

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    pub id: String,
    /// "page" | "database"
    pub kind: String,
    pub title: String,
}

fn rich(text: &str) -> Value {
    // Split on character boundaries, never inside a multi-byte character.
    let chars: Vec<char> = text.chars().collect();
    let parts: Vec<Value> = chars.chunks(TEXT_MAX).map(|c| json!({ "type": "text", "text": { "content": c.iter().collect::<String>() } })).collect();
    Value::Array(if parts.is_empty() { vec![json!({ "type": "text", "text": { "content": "" } })] } else { parts })
}

fn block(kind: &str, text: &str) -> Value {
    json!({ "object": "block", "type": kind, kind: { "rich_text": rich(text) } })
}

fn when(at: Option<u64>) -> String {
    at.map(|a| format!(" ({})", stamp(a))).unwrap_or_default()
}

/// The page body: a meta line, the summary, and the transcript when asked for. Uses the edited text the user sees.
pub fn blocks(m: &Meeting, include_transcript: bool) -> Vec<Value> {
    let mut out = vec![];
    let meta = format!("{} · {} · {} · {}", m.item.app_label, m.item.started_at.get(..10).unwrap_or(""), stamp(m.item.duration_ms), tr("notion.recorded_with"));
    out.push(json!({ "object": "block", "type": "paragraph", "paragraph": { "rich_text": [{ "type": "text", "text": { "content": meta }, "annotations": { "italic": true, "color": "gray" } }] } }));
    if let Some(s) = &m.summary {
        out.push(block("heading_2", &tr("notion.recap")));
        out.push(block("paragraph", s.recap.trim()));
    }
    let notes: Vec<_> = m.marks.iter().filter_map(|k| k.note_text().map(|n| (k, n))).collect();
    if !notes.is_empty() {
        out.push(block("heading_2", &tr("notion.your_notes")));
        for (k, n) in notes {
            let at = if k.later { format!(" ({})", tr("notion.after_call")) } else { when(Some(k.at)) };
            out.push(block("bulleted_list_item", &format!("{n}{at}")));
        }
    }
    if let Some(s) = &m.summary {
        if !s.decisions.is_empty() {
            out.push(block("heading_2", &tr("notion.decisions")));
            out.extend(s.decisions.iter().map(|d| block("bulleted_list_item", &format!("{}{}", d.text.trim(), when(d.at)))));
        }
        if !s.action_items.is_empty() {
            out.push(block("heading_2", &tr("notion.action_items")));
            for a in &s.action_items {
                let mut who = vec![];
                if let Some(o) = a.owner.as_deref().filter(|o| !o.trim().is_empty()) {
                    who.push(o.trim().to_string());
                }
                if let Some(d) = a.due.as_deref().filter(|d| !d.trim().is_empty()) {
                    who.push(tr_with("notion.due", &[("due", d.trim())]));
                }
                let who = if who.is_empty() { String::new() } else { format!(" — {}", who.join(", ")) };
                out.push(json!({ "object": "block", "type": "to_do", "to_do": { "rich_text": rich(&format!("{}{}{}", a.text.trim(), who, when(a.at))), "checked": a.done } }));
            }
        }
        if !s.open_questions.is_empty() {
            out.push(block("heading_2", &tr("notion.open_questions")));
            out.extend(s.open_questions.iter().map(|q| block("bulleted_list_item", q.text.trim())));
        }
        for sec in &s.sections {
            out.push(block("heading_2", sec.title.trim()));
            out.extend(sec.items.iter().map(|i| block("bulleted_list_item", &format!("{}{}", i.text.trim(), when(i.at)))));
        }
        if !s.highlights.is_empty() {
            out.push(block("heading_2", &tr("notion.highlights")));
            out.extend(s.highlights.iter().map(|h| block("bulleted_list_item", &format!("{}{}", h.text.trim(), when(h.at)))));
        }
    }
    if include_transcript && !m.transcript.is_empty() {
        out.push(block("heading_2", &tr("notion.transcript")));
        out.extend(m.transcript.iter().map(|l| block("paragraph", &format!("[{}] {}: {}", stamp(l.at), speaker_name(m, &l.speaker), l.text.trim()))));
    }
    out
}

/// The title text of a search result (pages keep it in their title property, databases at the top).
fn title_of(v: &Value) -> String {
    let plain = |arr: &Value| arr.as_array().map(|a| a.iter().filter_map(|t| t["plain_text"].as_str()).collect::<String>()).unwrap_or_default();
    if v["object"] == "database" {
        return plain(&v["title"]);
    }
    v["properties"].as_object().and_then(|p| p.values().find(|x| x["type"] == "title")).map(|x| plain(&x["title"])).unwrap_or_default()
}

pub struct Notion {
    base: String,
    secret: String,
}

impl Notion {
    pub fn new(base: &str, secret: &str) -> Self {
        Notion { base: base.trim_end_matches('/').to_string(), secret: secret.trim().to_string() }
    }

    fn call(&self, method: reqwest::Method, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let client = reqwest::blocking::Client::builder().connect_timeout(Duration::from_secs(10)).timeout(Duration::from_secs(60)).build().map_err(|e| e.to_string())?;
        let mut req = client.request(method, format!("{}{path}", self.base)).bearer_auth(&self.secret).header("Notion-Version", VERSION);
        if let Some(b) = body {
            req = req.json(b);
        }
        let res = req.send().map_err(|e| if e.is_connect() || e.is_timeout() { tr("notion.offline") } else { tr_err("notion.unreachable", e) })?;
        let status = res.status().as_u16();
        let v: Value = res.json().unwrap_or(Value::Null);
        match status {
            200..=299 => Ok(v),
            401 => Err(tr("notion.bad_secret")),
            403 | 404 => Err(tr("notion.no_access")),
            429 => Err(tr("notion.busy")),
            _ => Err(tr_with("notion.said", &[("message", v["message"].as_str().unwrap_or("something went wrong"))])),
        }
    }

    /// The workspace's name, which also proves the secret works.
    pub fn whoami(&self) -> Result<String, String> {
        let v = self.call(reqwest::Method::GET, "/v1/users/me", None)?;
        Ok(v["bot"]["workspace_name"].as_str().filter(|s| !s.is_empty()).unwrap_or("Notion").to_string())
    }

    /// Pages and databases shared with the integration, the ones it can write to.
    pub fn destinations(&self) -> Result<Vec<Destination>, String> {
        let v = self.call(reqwest::Method::POST, "/v1/search", Some(&json!({ "page_size": 100 })))?;
        Ok(v["results"]
            .as_array()
            .map(|r| {
                r.iter()
                    .filter(|x| !x["archived"].as_bool().unwrap_or(false))
                    .filter_map(|x| {
                        let kind = x["object"].as_str()?;
                        (kind == "page" || kind == "database").then(|| Destination { id: x["id"].as_str().unwrap_or_default().to_string(), kind: kind.into(), title: title_of(x) })
                    })
                    .filter(|d| !d.id.is_empty())
                    .map(|mut d| {
                        if d.title.trim().is_empty() {
                            d.title = "Untitled".into();
                        }
                        d
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Creates the meeting's page and returns its URL. In a database it becomes a new row (its title property
    /// gets the meeting title); under a page, a child page.
    pub fn export(&self, dest: &Destination, title: &str, body: &[Value]) -> Result<String, String> {
        let (parent, properties) = if dest.kind == "database" {
            let db = self.call(reqwest::Method::GET, &format!("/v1/databases/{}", dest.id), None)?;
            let prop = db["properties"].as_object().and_then(|p| p.iter().find(|(_, v)| v["type"] == "title")).map(|(k, _)| k.clone()).unwrap_or_else(|| "Name".into());
            (json!({ "database_id": dest.id }), json!({ prop: { "title": rich(title) } }))
        } else {
            (json!({ "page_id": dest.id }), json!({ "title": { "title": rich(title) } }))
        };
        let mut chunks = body.chunks(BLOCKS_PER_CALL);
        let first = chunks.next().unwrap_or(&[]);
        let page = self.call(reqwest::Method::POST, "/v1/pages", Some(&json!({ "parent": parent, "properties": properties, "children": first })))?;
        let id = page["id"].as_str().ok_or_else(|| tr("notion.no_page"))?.to_string();
        for rest in chunks {
            self.call(reqwest::Method::PATCH, &format!("/v1/blocks/{id}/children"), Some(&json!({ "children": rest })))?;
        }
        Ok(page["url"].as_str().unwrap_or_default().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summarizer::prompt::tests::{line, meeting};
    use std::sync::{Arc, Mutex};

    fn sample() -> Meeting {
        let mut m = meeting(vec![line("l0", 0, "me", "Hi."), line("l1", 4_000, "them:1", "Pricing moves to Q1.")]);
        m.speakers.insert("them:1".into(), "Priya".into());
        m.summary = Some(crate::storage::store::tests::summary("We planned Q1."));
        m.marks.clear();
        m
    }

    #[test]
    fn notes_sections_and_highlights_get_their_own_headings() {
        use crate::storage::model::{Mark, SummaryItem, SummarySection};
        let mut m = sample();
        m.marks = vec![Mark { id: "k0".into(), at: 61_000, note: Some("ask about legal".into()), later: false }, Mark { id: "n0".into(), at: 0, note: Some("send the deck".into()), later: true }, Mark { id: "k1".into(), at: 5_000, note: None, later: false }];
        let s = m.summary.as_mut().unwrap();
        s.sections = vec![SummarySection { id: "s0".into(), title: "Blockers".into(), items: vec![SummaryItem { id: "s0i0".into(), text: "Dana: legal".into(), at: None }] }];
        s.highlights = vec![SummaryItem { id: "h0".into(), text: "Budget agreed.".into(), at: Some(5_000) }];
        let texts: Vec<String> = blocks(&m, false).iter().map(|b| {
            let kind = b["type"].as_str().unwrap();
            b[kind]["rich_text"].as_array().map(|r| r.iter().map(|t| t["text"]["content"].as_str().unwrap_or("")).collect::<String>()).unwrap_or_default()
        }).collect();
        let at = |t: &str| texts.iter().position(|x| x == t).unwrap_or_else(|| panic!("{t} in {texts:?}"));
        assert!(at("Recap") < at("Your notes") && at("Your notes") < at("Decisions"));
        assert_eq!(texts[at("Your notes") + 1], "ask about legal (01:01)");
        assert_eq!(texts[at("Your notes") + 2], "send the deck (after the call)");
        assert_eq!(texts[at("Blockers") + 1], "Dana: legal");
        assert_eq!(texts[at("Highlights") + 1], "Budget agreed. (00:05)");
    }

    #[test]
    fn the_body_has_the_summary_and_optionally_the_transcript() {
        let b = blocks(&sample(), false);
        let types: Vec<&str> = b.iter().map(|x| x["type"].as_str().unwrap()).collect();
        assert_eq!(types, vec!["paragraph", "heading_2", "paragraph", "heading_2", "bulleted_list_item", "heading_2", "to_do"]);
        assert_eq!(b[4]["bulleted_list_item"]["rich_text"][0]["text"]["content"], "Ship onboarding first (00:21)");
        assert_eq!(b[6]["to_do"]["rich_text"][0]["text"]["content"], "Send wireframes — Me");
        let with = blocks(&sample(), true);
        assert_eq!(with.last().unwrap()["paragraph"]["rich_text"][0]["text"]["content"], "[00:04] Priya: Pricing moves to Q1.");
    }

    #[test]
    fn long_text_is_split_into_notions_2000_character_items() {
        let long = "é".repeat(4500);
        let r = rich(&long);
        assert_eq!(r.as_array().unwrap().len(), 3);
        assert_eq!(r[2]["text"]["content"].as_str().unwrap().chars().count(), 500);
    }

    /// A fake Notion that records requests and answers like the real API.
    fn fake(log: Arc<Mutex<Vec<(String, String, Value)>>>) -> String {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.server_addr().to_ip().unwrap());
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let auth = req.headers().iter().find(|h| h.field.equiv("Authorization")).map(|h| h.value.to_string()).unwrap_or_default();
                let path = req.url().to_string();
                log.lock().unwrap().push((req.method().to_string(), path.clone(), serde_json::from_str(&body).unwrap_or(Value::Null)));
                let (code, reply) = if auth != "Bearer secret_ok" {
                    (401, json!({ "message": "API token is invalid." }))
                } else if path == "/v1/users/me" {
                    (200, json!({ "bot": { "workspace_name": "Miguel's Notion" } }))
                } else if path == "/v1/search" {
                    (200, json!({ "results": [
                        { "object": "database", "id": "db1", "title": [{ "plain_text": "Meeting Notes" }] },
                        { "object": "page", "id": "pg1", "properties": { "title": { "type": "title", "title": [{ "plain_text": "Talaan Journey" }] } } },
                        { "object": "page", "id": "old", "archived": true, "properties": {} }
                    ] }))
                } else if path == "/v1/databases/db1" {
                    (200, json!({ "properties": { "Meeting": { "type": "title" }, "Date": { "type": "date" } } }))
                } else if path == "/v1/pages" {
                    (200, json!({ "id": "new1", "url": "https://www.notion.so/new1" }))
                } else {
                    (200, json!({}))
                };
                let _ = req.respond(tiny_http::Response::from_string(reply.to_string()).with_status_code(code));
            }
        });
        base
    }

    #[test]
    fn connect_list_and_export_into_a_database() {
        let log = Arc::new(Mutex::new(vec![]));
        let n = Notion::new(&fake(log.clone()), "secret_ok");
        assert_eq!(n.whoami().unwrap(), "Miguel's Notion");
        let d = n.destinations().unwrap();
        assert_eq!(d, vec![Destination { id: "db1".into(), kind: "database".into(), title: "Meeting Notes".into() }, Destination { id: "pg1".into(), kind: "page".into(), title: "Talaan Journey".into() }]);
        // 230 blocks: one create with 100, then two appends.
        let body: Vec<Value> = (0..230).map(|i| block("paragraph", &i.to_string())).collect();
        assert_eq!(n.export(&d[0], "Q4 sync", &body).unwrap(), "https://www.notion.so/new1");
        let log = log.lock().unwrap();
        let create = log.iter().find(|(m, p, _)| m == "POST" && p == "/v1/pages").unwrap();
        assert_eq!(create.2["parent"]["database_id"], "db1");
        assert_eq!(create.2["properties"]["Meeting"]["title"][0]["text"]["content"], "Q4 sync");
        assert_eq!(create.2["children"].as_array().unwrap().len(), 100);
        let appends: Vec<usize> = log.iter().filter(|(m, p, _)| m == "PATCH" && p == "/v1/blocks/new1/children").map(|(_, _, b)| b["children"].as_array().unwrap().len()).collect();
        assert_eq!(appends, vec![100, 30]);
    }

    #[test]
    fn under_a_page_it_is_a_child_page_and_a_bad_secret_says_so() {
        let log = Arc::new(Mutex::new(vec![]));
        let base = fake(log.clone());
        let page = Destination { id: "pg1".into(), kind: "page".into(), title: "Talaan Journey".into() };
        Notion::new(&base, "secret_ok").export(&page, "Q4 sync", &[]).unwrap();
        let create = log.lock().unwrap().iter().find(|(_, p, _)| p == "/v1/pages").unwrap().2.clone();
        assert_eq!(create["parent"]["page_id"], "pg1");
        assert_eq!(create["properties"]["title"]["title"][0]["text"]["content"], "Q4 sync");
        let e = Notion::new(&base, "wrong").whoami().unwrap_err();
        assert!(e.contains("didn't accept"), "{e}");
    }
}
