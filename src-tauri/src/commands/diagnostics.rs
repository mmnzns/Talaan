//! Check setup › Copy diagnostics and Open log folder, and the startup lines of the log.
//!
//! The report is plain text for a bug report: the app version, the system, the Check setup results (passed in by
//! the screen), the settings with personal values removed, and the last lines of the log. It never holds meeting
//! content: the log has none (see `logging`), and the settings lose names, the recording notice and addresses.

use super::{AppState, CmdResult};
use crate::gpu::{describe, Gpu, Kind};
use crate::logging;
use crate::storage::StorageError;
use serde_json::{json, Value};
use std::path::Path;
use tauri::{AppHandle, Manager};

/// Log lines in the report.
pub const TAIL_LINES: usize = 200;

/// Settings that hold the user's own words or names.
const PERSONAL: &[&str] = &["yourName", "recordingNotice", "calendarIcsUrl"];
/// Addresses: kept when they point at this computer (the usual Ollama / LM Studio setup), else only "(set)".
const ADDRESSES: &[&str] = &["ollamaAddress", "byokBaseUrl"];

fn marker(v: &Value) -> Value {
    match v {
        Value::Null => Value::Null,
        Value::String(s) if s.trim().is_empty() => json!("(empty)"),
        _ => json!("(set)"),
    }
}

fn is_local(address: &str) -> bool {
    let rest = address.trim().split_once("://").map(|(_, r)| r).unwrap_or(address.trim());
    let host = rest.split(['/', ':']).next().unwrap_or("");
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "0.0.0.0")
}

/// A key name that sounds like it holds a secret or an address.
fn sensitive_key(k: &str) -> bool {
    let k = k.to_ascii_lowercase();
    ["secret", "token", "password", "apikey", "api_key", "url", "address", "email"].iter().any(|s| k.contains(s))
}

fn scrub(v: &mut Value, key: Option<&str>) {
    match v {
        Value::String(s) => {
            if key.is_some_and(sensitive_key) {
                *v = marker(v);
            } else {
                *s = logging::redact(s);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| scrub(x, None)),
        Value::Object(o) => o.iter_mut().for_each(|(k, x)| scrub(x, Some(k))),
        _ => {}
    }
}

/// settings.json for a bug report: no names, no recording notice, no addresses off this computer, no secrets.
pub fn settings_for_report(v: &Value) -> Value {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
        for k in PERSONAL {
            if let Some(x) = o.get_mut(*k) {
                *x = marker(x);
            }
        }
        for k in ADDRESSES {
            if let Some(x) = o.get_mut(*k) {
                if !x.as_str().is_some_and(is_local) {
                    *x = marker(x);
                }
            }
        }
        if let Some(n) = o.get_mut("notion") {
            *n = json!({ "connected": n["connected"].as_bool().unwrap_or(false), "destination": marker(&n["destination"]) });
        }
    }
    let local: Vec<(String, Value)> = ADDRESSES.iter().filter_map(|k| Some((k.to_string(), v.get(*k)?.clone()))).filter(|(_, x)| x.as_str().is_some_and(is_local)).collect();
    scrub(&mut v, None);
    // A local address survives the address rule above.
    if let Some(o) = v.as_object_mut() {
        for (k, x) in local {
            o.insert(k, x);
        }
    }
    v
}

/// The facts about this computer at the top of the report (and in the log at startup).
pub struct Facts {
    pub version: String,
    pub os: String,
    pub cpu: String,
    pub graphics: String,
    pub log_dir: String,
}

pub fn report(facts: &Facts, setup: &[String], settings: Option<&Value>, log: &[String]) -> String {
    let mut s = String::new();
    s.push_str("Talaan diagnostics\n");
    s.push_str("No meeting content: no transcripts, summaries, titles, names, addresses or keys.\n\n");
    s.push_str(&format!("App: Talaan {}\n", facts.version));
    s.push_str(&format!("System: {}\n", facts.os));
    s.push_str(&format!("Processor: {}\n", facts.cpu));
    s.push_str(&format!("Graphics: {}\n", facts.graphics));
    s.push_str(&format!("Log folder: {}\n", logging::redact(&facts.log_dir)));
    s.push_str("\n== Check setup ==\n");
    if setup.is_empty() {
        s.push_str("(not run)\n");
    }
    for line in setup {
        s.push_str(&logging::redact(line));
        s.push('\n');
    }
    s.push_str("\n== Settings (personal values removed) ==\n");
    match settings {
        Some(v) => s.push_str(&serde_json::to_string_pretty(&settings_for_report(v)).unwrap_or_default()),
        None => s.push_str("(none yet)"),
    }
    s.push_str(&format!("\n\n== Last {} log lines ==\n", log.len()));
    for line in log {
        // Written redacted already; again in case the file came from an older build.
        s.push_str(&logging::redact(line));
        s.push('\n');
    }
    s
}

fn cpu() -> String {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0);
    let name = crate::platform::system::cpu_name();
    let name = if name.is_empty() { std::env::consts::ARCH.to_string() } else { format!("{name}, {}", std::env::consts::ARCH) };
    format!("{name}, {threads} threads")
}

/// What speech and local summaries run on (no helper is started for this).
fn graphics(use_gpu: bool, data_dir: &Path) -> String {
    if cfg!(target_os = "macos") {
        return if use_gpu { "Metal (Apple GPU)".into() } else { "processor (GPU turned off)".into() };
    }
    let gpu = Gpu::shared(data_dir);
    let drivers = crate::gpu::driver_fingerprint();
    format!(
        "speech: {}; summaries: {}; drivers: {}",
        describe(&gpu.plan(Kind::Stt, use_gpu)),
        describe(&gpu.plan(Kind::Llm, use_gpu)),
        if drivers.is_empty() { "unknown".into() } else { drivers }
    )
}

fn facts(app: &AppHandle) -> Facts {
    let state = app.state::<AppState>();
    let (root, settings) = {
        let s = state.lock();
        (s.root().to_path_buf(), s.settings_get().ok().flatten())
    };
    let use_gpu = settings.as_ref().and_then(|s| s["useGpu"].as_bool()).unwrap_or(true);
    Facts {
        version: app.package_info().version.to_string(),
        os: crate::platform::system::os_version(),
        cpu: cpu(),
        graphics: graphics(use_gpu, &root),
        log_dir: logging::dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_else(|| "(not writable)".into()),
    }
}

/// The first lines of each run.
pub fn log_startup(app: &AppHandle) {
    let f = facts(app);
    log::info!("Talaan {} starting on {}{}", f.version, f.os, if cfg!(debug_assertions) { " (debug build)" } else { "" });
    log::info!("processor: {}", f.cpu);
    log::info!("graphics: {}", f.graphics);
}

/// Copy diagnostics: the report as plain text (the screen puts it on the clipboard).
#[tauri::command(async)]
pub fn diagnostics_report(app: AppHandle, setup: Vec<String>) -> String {
    let f = facts(&app);
    let settings = app.state::<AppState>().lock().settings_get().ok().flatten();
    let log = logging::dir().map(|d| logging::tail(d, TAIL_LINES)).unwrap_or_default();
    log::info!("diagnostics copied ({} log lines)", log.len());
    report(&f, &setup, settings.as_ref(), &log)
}

/// Open log folder: shows talaan.log in the file manager.
#[tauri::command]
pub fn logs_reveal() -> CmdResult<()> {
    let Some(dir) = logging::dir() else {
        return Err(StorageError::Refused(crate::i18n::tr("error.no_log")));
    };
    tauri_plugin_opener::reveal_item_in_dir(dir.join(logging::FILE)).map_err(|e| StorageError::Refused(crate::i18n::tr_err("error.open_folder", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn personal_settings() -> Value {
        json!({
            "theme": "auto",
            "yourName": "Ana Cruz",
            "recordingNotice": "Hi, Ana from Acme here. I record this call.",
            "summaryEngine": "byok",
            "byokProvider": "claude",
            "byokKeys": ["claude"],
            "byokBaseUrl": "https://llm.acme.internal/v1",
            "ollamaAddress": "http://localhost:11434",
            "calendarConnected": true,
            "calendarIcsUrl": "https://outlook.office365.com/owa/calendar/secret123/calendar.ics",
            "notion": { "connected": true, "workspace": "Ana's Workspace", "destination": { "id": "abc", "kind": "page", "title": "Board meetings" } },
            "callApps": [{ "id": "zoom", "name": "Zoom", "note": "", "rule": "ask" }],
            "ignoredApps": [{ "name": "VoiceInk", "why": "dictation" }],
            "someFutureToken": "tok_live_123",
            "retention": "7d"
        })
    }

    #[test]
    fn the_settings_lose_everything_personal() {
        let out = settings_for_report(&personal_settings());
        let text = serde_json::to_string(&out).unwrap();
        for leak in ["Ana", "Acme", "acme.internal", "office365", "secret123", "Board meetings", "tok_live"] {
            assert!(!text.contains(leak), "{leak} leaked: {text}");
        }
        assert_eq!(out["yourName"], "(set)");
        assert_eq!(out["recordingNotice"], "(set)");
        assert_eq!(out["byokBaseUrl"], "(set)");
        assert_eq!(out["notion"], json!({ "connected": true, "destination": "(set)" }));
        assert_eq!(out["someFutureToken"], "(set)");
        // What a bug report needs stays.
        assert_eq!(out["ollamaAddress"], "http://localhost:11434");
        assert_eq!(out["summaryEngine"], "byok");
        assert_eq!(out["byokKeys"], json!(["claude"]));
        assert_eq!(out["callApps"][0]["rule"], "ask");
        assert_eq!(out["retention"], "7d");
        assert_eq!(settings_for_report(&json!({ "yourName": "" }))["yourName"], "(empty)");
    }

    #[test]
    fn local_addresses_are_recognised() {
        assert!(is_local("http://localhost:11434"));
        assert!(is_local("http://127.0.0.1:1234/v1"));
        assert!(!is_local("https://llm.acme.internal/v1"));
        assert!(!is_local("http://localhost.evil.com"));
    }

    #[test]
    fn the_report_has_each_part_and_no_content() {
        let facts = Facts { version: "1.1.0".into(), os: "Windows 11 Pro 24H2 (build 26100)".into(), cpu: "AMD Ryzen 7, x86_64, 16 threads".into(), graphics: "speech: GPU helper".into(), log_dir: "C:\\logs".into() };
        let setup = vec!["ok · Microphone · Allowed".to_string()];
        let log = vec!["2026-10-16T14:30:00.000-04:00 INFO  commands::recorder: started".to_string(), "x see https://outlook.office365.com/owa/calendar/secret/calendar.ics".to_string()];
        let r = report(&facts, &setup, Some(&personal_settings()), &log);
        for part in ["App: Talaan 1.1.0", "System: Windows 11", "Processor: AMD", "Graphics: speech", "== Check setup ==", "ok · Microphone · Allowed", "== Settings (personal values removed) ==", "\"retention\": \"7d\"", "== Last 2 log lines ==", "commands::recorder: started", "https://outlook.office365.com/…"] {
            assert!(r.contains(part), "missing {part}:\n{r}");
        }
        for leak in ["Ana", "Acme", "secret123", "/secret", "Board meetings", "office365.com/owa"] {
            assert!(!r.contains(leak), "{leak} leaked:\n{r}");
        }
        assert!(report(&facts, &[], None, &[]).contains("(not run)"));
    }
}
