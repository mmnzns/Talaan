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

/// Settings that are safe to show as they are: switches, choices and model names. Anything not listed here (a new
/// setting, or one that holds words, names or addresses) is left out of the report, named only in `notShown`.
const SAFE: &[&str] = &[
    "theme", "detectCalls", "autoTagApps", "launchAtLogin", "checkUpdates", "warnOnSpeakers", "notifySummaryReady", "activeSpeechModel",
    "separateSpeakers", "reuseModels", "meetingLanguage", "uiLanguage", "useGpu", "summaryEngine", "summarizeAutomatically", "summaryLanguage",
    "localModel", "ollamaModel", "byokProvider", "byokKeys", "byokModel", "calendarConnected", "tabDetection", "suggestSpeakerNames",
    "retention", "transcriptRetention", "onboardingDone",
];
/// Settings shown only as set or empty: the user's own words.
const PERSONAL: &[&str] = &["yourName", "recordingNotice"];
/// Addresses: shown when they point at this computer (the usual Ollama / LM Studio setup), without a query.
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
    let host = rest.split(['/', ':', '?', '#']).next().unwrap_or("");
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "0.0.0.0")
}

/// A safe value: strings redacted, arrays and objects of them too.
fn safe(v: &Value) -> Value {
    match v {
        Value::String(s) => Value::String(logging::redact(s)),
        Value::Array(a) => Value::Array(a.iter().map(safe).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), safe(x))).collect()),
        other => other.clone(),
    }
}

/// A call app rule's id: Talaan's own ids as they are; an app the user added (its id is made from the name they
/// typed) as "custom".
fn call_app_id(id: &Value) -> Value {
    let known = |s: &str| crate::calls::apps::CALL_APPS.iter().any(|(k, _, _)| *k == s) || s == super::calls::BROWSER_CALL;
    match id.as_str() {
        Some(s) if known(s) => json!(s),
        Some(_) => json!("custom"),
        None => Value::Null,
    }
}

/// settings.json for a bug report, built from an allowlist: known-safe settings as they are; names and the
/// recording notice as set or empty; the microphone without its owner's name; local addresses without their query,
/// others as set; Notion as connected or not; call app rules by id; everything else left out.
pub fn settings_for_report(v: &Value) -> Value {
    let Some(o) = v.as_object() else { return json!("(not an object)") };
    let mut out = serde_json::Map::new();
    for (k, x) in o {
        let shown = if SAFE.contains(&k.as_str()) {
            safe(x)
        } else if PERSONAL.contains(&k.as_str()) {
            marker(x)
        } else if ADDRESSES.contains(&k.as_str()) {
            match x.as_str() {
                Some(a) if is_local(a) => json!(a.split(['?', '#']).next().unwrap_or("").trim()),
                _ => marker(x),
            }
        } else if k == "microphone" {
            x.as_str().map(|m| json!(logging::device(m))).unwrap_or(Value::Null)
        } else if k == "notion" {
            json!({ "connected": x["connected"].as_bool().unwrap_or(false), "destination": marker(&x["destination"]) })
        } else if k == "callApps" {
            let rules = x.as_array().map(|a| a.iter().map(|r| json!({ "id": call_app_id(&r["id"]), "rule": safe(&r["rule"]) })).collect()).unwrap_or_default();
            Value::Array(rules)
        } else if k == "ignoredApps" {
            json!(x.as_array().map(Vec::len).unwrap_or(0))
        } else {
            continue;
        };
        out.insert(k.clone(), shown);
    }
    let hidden: Vec<&String> = o.keys().filter(|k| !out.contains_key(k.as_str())).collect();
    if !hidden.is_empty() {
        out.insert("notShown".into(), json!(hidden));
    }
    Value::Object(out)
}
/// The facts about this computer at the top of the report (and in the log at startup).
pub struct Facts {
    pub version: String,
    pub os: String,
    pub cpu: String,
    pub graphics: String,
    pub log_dir: String,
}

/// Meeting ids (`2026-10-16_1430_ab12`) become `m-3f2a9d`: the same id gives the same hash within one report, so
/// lines about one meeting still match up, but the report doesn't say when anyone met. `salt` is new per report.
pub fn hash_ids(line: &str, salt: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let b = line.as_bytes();
    let shape = |w: &[u8]| {
        w.len() == 20
            && w.iter().enumerate().all(|(i, c)| match i {
                4 | 7 => *c == b'-',
                10 | 15 => *c == b'_',
                16..=19 => c.is_ascii_lowercase() || c.is_ascii_digit(),
                _ => c.is_ascii_digit(),
            })
    };
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < b.len() {
        if i + 20 <= b.len() && shape(&b[i..i + 20]) && (i + 20 == b.len() || !b[i + 20].is_ascii_alphanumeric()) {
            let digest = Sha256::new().chain_update(salt).chain_update(&b[i..i + 20]).finalize();
            out.push_str("m-");
            out.extend(digest[..3].iter().map(|x| format!("{x:02x}")));
            i += 20;
        } else {
            let c = line[i..].chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

/// A line with every known device name replaced by its owner-free form (`logging::device`), longest names first.
pub fn scrub_devices(line: &str, devices: &[String]) -> String {
    let mut names: Vec<&String> = devices.iter().filter(|d| !d.trim().is_empty()).collect();
    names.sort_by_key(|d| std::cmp::Reverse(d.len()));
    let mut out = line.to_string();
    for name in names {
        let clean = logging::device(name);
        if clean != *name {
            out = out.replace(name.as_str(), &clean);
        }
    }
    out
}

/// `devices`: the connected microphones' names; the chosen one from the settings is added here. Check setup lines
/// name them ("Miguel's AirPods isn't connected"), so they lose their owner's name.
pub fn report(facts: &Facts, setup: &[String], settings: Option<&Value>, devices: &[String], log: &[String]) -> String {
    let salt = rand::random::<u64>().to_le_bytes();
    let mut devices = devices.to_vec();
    if let Some(m) = settings.and_then(|s| s["microphone"].as_str()) {
        devices.push(m.to_string());
    }
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
        s.push_str(&hash_ids(&logging::redact(&scrub_devices(line, &devices)), &salt));
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
        s.push_str(&hash_ids(&logging::redact(line), &salt));
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
    report(&f, &setup, settings.as_ref(), &crate::recorder::mic::input_devices(), &log)
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
        // Anything not on the allowlist is left out, by name only.
        assert_eq!(out.get("someFutureToken"), None);
        assert_eq!(out.get("calendarIcsUrl"), None);
        assert_eq!(out["notShown"], json!(["calendarIcsUrl", "someFutureToken"]));
        assert_eq!(out["callApps"], json!([{ "id": "zoom", "rule": "ask" }]));
        let custom = settings_for_report(&json!({ "callApps": [{ "id": "browser", "rule": "never" }, { "id": "ana-s-secret-tool", "name": "Ana's secret tool", "rule": "always" }] }));
        assert_eq!(custom["callApps"], json!([{ "id": "browser", "rule": "never" }, { "id": "custom", "rule": "always" }]));
        assert_eq!(out["ignoredApps"], 1);
        // What a bug report needs stays.
        assert_eq!(out["ollamaAddress"], "http://localhost:11434");
        assert_eq!(out["summaryEngine"], "byok");
        assert_eq!(out["byokKeys"], json!(["claude"]));
        assert_eq!(out["callApps"][0]["rule"], "ask");
        assert_eq!(out["retention"], "7d");
        assert_eq!(settings_for_report(&json!({ "yourName": "" }))["yourName"], "(empty)");
        // A local address loses its query; a microphone its owner.
        let more = settings_for_report(&json!({ "ollamaAddress": "http://localhost:11434/v1?token=abc#x", "microphone": "Microphone (AirPods de Ana)" }));
        assert_eq!(more["ollamaAddress"], "http://localhost:11434/v1");
        assert_eq!(more["microphone"], "Microphone (AirPods de …)");
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
        let r = report(&facts, &setup, Some(&personal_settings()), &[], &log);
        for part in ["App: Talaan 1.1.0", "System: Windows 11", "Processor: AMD", "Graphics: speech", "== Check setup ==", "ok · Microphone · Allowed", "== Settings (personal values removed) ==", "\"retention\": \"7d\"", "== Last 2 log lines ==", "commands::recorder: started", "https://outlook.office365.com/…"] {
            assert!(r.contains(part), "missing {part}:\n{r}");
        }
        for leak in ["Ana", "Acme", "secret123", "/secret", "Board meetings", "office365.com/owa"] {
            assert!(!r.contains(leak), "{leak} leaked:\n{r}");
        }
        assert!(report(&facts, &[], None, &[], &[]).contains("(not run)"));
    }

    #[test]
    fn meeting_ids_are_hashed_but_still_match_up() {
        let line = "transcribing meeting 2026-10-16_1430_ab12; later 2026-10-16_1430_ab12 and 2026-10-17_0900_zz99.";
        let out = hash_ids(line, b"salt");
        assert!(!out.contains("2026-10"), "{out}");
        let ids: Vec<&str> = out.split(|c: char| c == ' ' || c == '.' || c == ';').filter(|w| w.starts_with("m-")).collect();
        assert_eq!(ids.len(), 3, "{out}");
        assert_eq!(ids[0], ids[1]);
        assert_ne!(ids[0], ids[2]);
        assert_eq!(ids[0].len(), 8);
        assert_ne!(hash_ids("2026-10-16_1430_ab12", b"other"), hash_ids("2026-10-16_1430_ab12", b"salt"), "a new salt per report");
        assert_eq!(hash_ids("model 2026-10-16 v3 and 2026-10-16_1430_ab12x", b"s"), "model 2026-10-16 v3 and 2026-10-16_1430_ab12x");
        // In the report.
        let facts = Facts { version: "1".into(), os: "x".into(), cpu: "x".into(), graphics: "x".into(), log_dir: "x".into() };
        let r = report(&facts, &[], None, &[], &["2026-10-16T14:30:00.000-04:00 INFO  commands::recorder: recording stopped: meeting 2026-10-16_1430_ab12".to_string()]);
        assert!(r.contains("recording stopped: meeting m-") && !r.contains("1430_ab12"), "{r}");
    }

    #[test]
    fn microphone_names_lose_their_owner_in_the_report() {
        let facts = Facts { version: "1.1.0".into(), os: "macOS 15.1".into(), cpu: "Apple M2".into(), graphics: "Metal".into(), log_dir: "~/Library/Logs/Talaan".into() };
        let setup = vec![
            "warn · Microphone · Miguel's AirPods Pro isn't connected, so the system default is used.".to_string(),
            "ok · Microphone access · Allowed. Your side of the call is recorded.".to_string(),
            "ok · Microphone · AirPods de Ana is ready".to_string(),
        ];
        let settings = json!({ "microphone": "Miguel's AirPods Pro" });
        let r = report(&facts, &setup, Some(&settings), &["AirPods de Ana".to_string(), "MacBook Pro Microphone".to_string()], &[]);
        assert!(r.contains("warn · Microphone · …'s AirPods Pro isn't connected"), "{r}");
        assert!(r.contains("ok · Microphone · AirPods de … is ready"), "{r}");
        assert!(r.contains("\"microphone\": \"…'s AirPods Pro\""), "{r}");
        assert!(!r.contains("Miguel") && !r.contains("Ana"), "{r}");
        assert_eq!(scrub_devices("MacBook Pro Microphone is ready", &["MacBook Pro Microphone".into()]), "MacBook Pro Microphone is ready");
    }
}
