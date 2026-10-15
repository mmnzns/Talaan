//! Call detection wired to the app: a once-a-second loop reads who is using the mic, runs the rules, and turns
//! their answers into recordings and the floating prompt window (nudge, Always banner, call ended).

use super::recorder::{is_recording, start_recording, stop_recording};
use super::{AppState, CmdResult};
use crate::calls::apps::{identify, CALL_APPS};
use crate::calls::rules::{Action, Detector, Rule, Settings, COUNTDOWN_MS};
use crate::calls::tabs::{browser_for, classify, BrowserCall, Family, Tabs};
use serde::Serialize;
use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

pub const PROMPT_WINDOW: &str = "prompt";
pub const PROMPT_EVENT: &str = "call-prompt";

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    /// "nudge" | "banner" | "ended"
    pub kind: &'static str,
    pub app: String,
    pub label: String,
    /// For "ended": when the countdown stops the recording (ms since the Unix epoch).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<u64>,
}

pub struct CallsState {
    detector: Mutex<Detector>,
    prompt: Mutex<Option<Prompt>>,
}

/// A browser call found by tab detection is its own app id, so its rule allows Always; its label is the call's
/// ("Google Meet"), set by the calls loop.
pub const BROWSER_CALL: &str = "browser-call";
static BROWSER_CALL_LABEL: Mutex<String> = Mutex::new(String::new());

fn label_of(app: &str) -> String {
    if app == BROWSER_CALL {
        let l = BROWSER_CALL_LABEL.lock().unwrap_or_else(|e| e.into_inner()).clone();
        return if l.is_empty() { "Browser call".into() } else { l };
    }
    CALL_APPS.iter().find(|(id, _, _)| *id == app).map(|(_, l, _)| l.to_string()).unwrap_or_else(|| "Call".into())
}

/// What each browser said last time it was asked, for Settings › Connections and a short cache.
#[derive(Default)]
struct TabCache {
    by_app: std::collections::HashMap<&'static str, (Instant, Tabs)>,
}

/// How long one answer is reused: the loop runs every second, osascript costs ~50 ms.
const TAB_CACHE: Duration = Duration::from_secs(4);

static BROWSER_ACCESS: Mutex<Vec<(&'static str, &'static str)>> = Mutex::new(Vec::new());

/// Settings › Connections: each browser's Automation state as last seen ("allowed" | "denied").
pub fn browser_access() -> Vec<(&'static str, &'static str)> {
    BROWSER_ACCESS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn note_access(name: &'static str, tabs: &Tabs) {
    let state = match tabs {
        Tabs::Urls(_) => "allowed",
        Tabs::Denied => "denied",
        Tabs::Unknown => return,
    };
    let mut v = BROWSER_ACCESS.lock().unwrap_or_else(|e| e.into_inner());
    v.retain(|(n, _)| *n != name);
    v.push((name, state));
}

/// With tab detection on, turns the "browser" entry into the call it is, drops it when no tab is a call, or
/// leaves it (Ask) when Talaan can't tell.
fn resolve_browsers(users: &[crate::platform::mic_users::MicUser], apps: &mut Vec<&str>, cache: &mut TabCache) {
    if !apps.contains(&"browser") {
        return;
    }
    let mut seen: Vec<(&'static str, &'static str, Family)> = users.iter().filter_map(|u| browser_for(&u.bundle_id)).collect();
    seen.sort_by_key(|b| b.0);
    seen.dedup_by_key(|b| b.0);
    let lookups: Vec<Tabs> = seen
        .iter()
        .map(|(app, name, family)| {
            let fresh = cache.by_app.get(app).filter(|(t, _)| t.elapsed() < TAB_CACHE).map(|(_, r)| r.clone());
            let r = fresh.unwrap_or_else(|| crate::platform::browser_tabs::tab_urls(app, *family));
            note_access(name, &r);
            cache.by_app.insert(app, (Instant::now(), r.clone()));
            r
        })
        .collect();
    match classify(&lookups) {
        BrowserCall::Call(label) => {
            *BROWSER_CALL_LABEL.lock().unwrap_or_else(|e| e.into_inner()) = label.to_string();
            apps.retain(|a| *a != "browser");
            apps.push(BROWSER_CALL);
        }
        BrowserCall::NotACall => apps.retain(|a| *a != "browser"),
        BrowserCall::Unsure => {}
    }
}

fn epoch_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Sizes of the prompt window per kind (points).
fn size_of(kind: &str) -> (f64, f64) {
    match kind {
        "banner" => (420.0, 96.0),
        "ended" => (380.0, 168.0),
        _ => (380.0, 148.0),
    }
}

fn show_prompt(app: &AppHandle, prompt: Prompt) {
    let (w, h) = size_of(prompt.kind);
    *app.state::<CallsState>().prompt.lock().unwrap_or_else(|e| e.into_inner()) = Some(prompt.clone());
    let window = match app.get_webview_window(PROMPT_WINDOW) {
        Some(win) => win,
        None => match WebviewWindowBuilder::new(app, PROMPT_WINDOW, WebviewUrl::App("index.html#/prompt".into()))
            .title("Talaan")
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            .focused(false)
            .visible(false)
            .inner_size(w, h)
            .build()
        {
            Ok(win) => win,
            Err(_) => return,
        },
    };
    let _ = window.set_size(tauri::LogicalSize::new(w, h));
    super::tray::place_under_icon(&window, w);
    let _ = window.show();
    let _ = app.emit(PROMPT_EVENT, Some(prompt));
}

fn hide_prompt(app: &AppHandle) {
    *app.state::<CallsState>().prompt.lock().unwrap_or_else(|e| e.into_inner()) = None;
    if let Some(w) = app.get_webview_window(PROMPT_WINDOW) {
        let _ = w.hide();
    }
    let _ = app.emit(PROMPT_EVENT, None::<Prompt>);
}

fn rules_from(settings: &Value) -> (bool, Vec<(String, Rule)>, Vec<String>) {
    let enabled = settings["detectCalls"].as_bool().unwrap_or(true);
    let rules = settings["callApps"]
        .as_array()
        .map(|a| a.iter().filter_map(|r| Some((r["id"].as_str()?.to_string(), Rule::parse(r["rule"].as_str().unwrap_or("ask"))))).collect())
        .unwrap_or_default();
    let ignored = settings["ignoredApps"].as_array().map(|a| a.iter().filter_map(|r| r["name"].as_str().map(str::to_string)).collect()).unwrap_or_default();
    (enabled, rules, ignored)
}

fn run(app: &AppHandle, action: Action) {
    match action {
        Action::Nudge { app: id } => show_prompt(app, Prompt { kind: "nudge", label: label_of(&id), app: id, deadline: None }),
        Action::CloseNudge | Action::ClosePrompt => hide_prompt(app),
        Action::AutoStart { app: id } => {
            let label = label_of(&id);
            if start_recording(app, &id, &label).is_ok() {
                show_prompt(app, Prompt { kind: "banner", app: id, label, deadline: None });
            }
        }
        Action::CallEnded { app: id } => show_prompt(app, Prompt { kind: "ended", label: label_of(&id), app: id, deadline: Some(epoch_ms() + COUNTDOWN_MS) }),
        Action::AutoStop => {
            let _ = stop_recording(app);
            hide_prompt(app);
        }
    }
}

pub fn setup(app: &AppHandle) {
    app.manage(CallsState { detector: Mutex::new(Detector::default()), prompt: Mutex::new(None) });
    {
        let app = app.clone();
        std::thread::Builder::new()
            .name("calls".into())
            .spawn(move || {
                let started = Instant::now();
                let mut tab_cache = TabCache::default();
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                    let settings = app.state::<AppState>().lock().settings_get().ok().flatten().unwrap_or(Value::Null);
                    let (enabled, rules, ignored) = rules_from(&settings);
                    #[cfg_attr(not(debug_assertions), allow(unused_mut))] // only the debug hook below adds to it
                    let mut users = crate::platform::mic_users::mic_users();
                    // Debug builds: `dev-mic-users.txt` in the data folder adds pretend mic users (one bundle id
                    // per line), so the rules can be exercised end to end without a real call.
                    #[cfg(debug_assertions)]
                    if let Ok(fake) = std::fs::read_to_string(app.state::<AppState>().lock().root().join("dev-mic-users.txt")) {
                        users.extend(fake.lines().filter(|l| !l.trim().is_empty()).map(|b| crate::platform::mic_users::MicUser { pid: 0, bundle_id: b.trim().into(), path: String::new() }));
                    }
                    let mut apps: Vec<&str> = users.iter().filter_map(|u| identify(&u.bundle_id, &u.path, &ignored).map(|(id, _)| id)).collect();
                    apps.sort();
                    apps.dedup();
                    if settings["tabDetection"].as_bool().unwrap_or(false) {
                        resolve_browsers(&users, &mut apps, &mut tab_cache);
                    }
                    // A verified browser call follows the Browser calls rule, Always included.
                    let rule = |a: &str| {
                        let a = if a == BROWSER_CALL { "browser" } else { a };
                        rules.iter().find(|(id, _)| id == a).map(|(_, r)| *r).unwrap_or(Rule::Ask)
                    };
                    let recording = is_recording(&app);
                    let actions = app.state::<CallsState>().detector.lock().unwrap_or_else(|e| e.into_inner()).tick(
                        started.elapsed().as_millis() as u64,
                        &apps,
                        recording,
                        &Settings { enabled, rule: &rule },
                    );
                    for a in actions {
                        run(&app, a);
                    }
                }
            })
            .expect("calls thread");
    }
}

#[derive(Serialize)]
pub struct BrowserStatus {
    name: &'static str,
    /// "allowed" | "denied" | "not_asked" | "not_installed"
    state: &'static str,
}

/// True when any of the candidate paths exists.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn installed_from(candidates: &[std::path::PathBuf], exists: impl Fn(&std::path::Path) -> bool) -> bool {
    candidates.iter().any(|c| exists(c))
}

/// (name, installed) for each browser Settings lists.
#[cfg(not(target_os = "windows"))]
fn installed_browsers() -> Vec<(&'static str, bool)> {
    let home = dirs::home_dir().unwrap_or_default();
    [("Chrome", "Google Chrome.app"), ("Safari", "Safari.app"), ("Arc", "Arc.app"), ("Edge", "Microsoft Edge.app"), ("Brave", "Brave Browser.app")]
        .into_iter()
        .map(|(name, bundle)| {
            let installed = name == "Safari" || std::path::Path::new("/Applications").join(bundle).exists() || home.join("Applications").join(bundle).exists();
            (name, installed)
        })
        .collect()
}

/// Windows: each exe under Program Files, Program Files (x86) or %LOCALAPPDATA%; Arc is a packaged app.
#[cfg(target_os = "windows")]
fn installed_browsers() -> Vec<(&'static str, bool)> {
    use std::path::PathBuf;
    let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"].iter().filter_map(|v| std::env::var_os(v)).map(PathBuf::from).collect();
    let under = |rel: &str| -> Vec<PathBuf> { roots.iter().map(|r| r.join(rel)).collect() };
    let arc = std::env::var_os("LOCALAPPDATA")
        .and_then(|l| std::fs::read_dir(PathBuf::from(l).join("Packages")).ok())
        .map(|d| d.filter_map(Result::ok).any(|e| e.file_name().to_string_lossy().starts_with("TheBrowserCompany.Arc_")))
        .unwrap_or(false);
    let exists = |p: &std::path::Path| p.exists();
    vec![
        ("Chrome", installed_from(&under(r"Google\Chrome\Application\chrome.exe"), exists)),
        ("Arc", arc),
        ("Edge", installed_from(&under(r"Microsoft\Edge\Application\msedge.exe"), exists)),
        ("Brave", installed_from(&under(r"BraveSoftware\Brave-Browser\Application\brave.exe"), exists)),
        ("Opera", installed_from(&under(r"Programs\Opera\opera.exe"), exists)),
        ("Vivaldi", installed_from(&under(r"Vivaldi\Application\vivaldi.exe"), exists)),
        ("Comet", installed_from(&under(r"Perplexity\Comet\Application\comet.exe"), exists)),
    ]
}

/// The browsers tab detection works with, and whether Talaan may read their tabs (as last seen; macOS asks
/// the first time a call happens in each one).
#[tauri::command]
pub fn browsers_status() -> Vec<BrowserStatus> {
    let seen = browser_access();
    installed_browsers()
        .into_iter()
        .map(|(name, installed)| {
            let state = seen.iter().find(|(n, _)| *n == name).map(|(_, s)| *s).unwrap_or(if installed { "not_asked" } else { "not_installed" });
            BrowserStatus { name, state }
        })
        .collect()
}

/// What the prompt window should show right now (it asks when it first loads).
#[tauri::command]
pub fn calls_prompt(state: State<CallsState>) -> Option<Prompt> {
    state.prompt.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Nudge → Record: start recording this call and show the banner.
#[tauri::command]
pub fn calls_accept(app: AppHandle, state: State<CallsState>, app_id: String) -> CmdResult<()> {
    state.detector.lock().unwrap_or_else(|e| e.into_inner()).accepted(&app_id);
    let label = label_of(&app_id);
    start_recording(&app, &app_id, &label)?;
    show_prompt(&app, Prompt { kind: "banner", app: app_id, label, deadline: None });
    Ok(())
}

/// Nudge → Not now.
#[tauri::command]
pub fn calls_dismiss(app: AppHandle, state: State<CallsState>, app_id: String) {
    state.detector.lock().unwrap_or_else(|e| e.into_inner()).dismissed(&app_id);
    hide_prompt(&app);
}

/// Call ended → Keep recording.
#[tauri::command]
pub fn calls_keep(app: AppHandle, state: State<CallsState>) {
    state.detector.lock().unwrap_or_else(|e| e.into_inner()).keep_recording();
    hide_prompt(&app);
}

/// Banner or Call ended → Stop.
#[tauri::command]
pub fn calls_stop(app: AppHandle) -> CmdResult<String> {
    hide_prompt(&app);
    stop_recording(&app)
}

/// Closes the prompt without changing anything (banner's close button).
#[tauri::command]
pub fn calls_hide(app: AppHandle) {
    hide_prompt(&app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_browser_is_installed_when_any_candidate_exists() {
        use std::path::{Path, PathBuf};
        let found = PathBuf::from(r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe");
        let cands = [PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"), found.clone()];
        assert!(installed_from(&cands, |p: &Path| p == found));
        assert!(!installed_from(&cands, |_| false));
        assert!(!installed_from(&[], |_| true));
    }

    #[test]
    fn settings_become_rules() {
        let (enabled, rules, ignored) = rules_from(&json!({
            "detectCalls": true,
            "callApps": [{ "id": "zoom", "rule": "always" }, { "id": "facetime", "rule": "never" }],
            "ignoredApps": [{ "name": "VoiceInk" }]
        }));
        assert!(enabled);
        assert_eq!(rules, vec![("zoom".to_string(), Rule::Always), ("facetime".to_string(), Rule::Never)]);
        assert_eq!(ignored, vec!["VoiceInk".to_string()]);
        assert!(rules_from(&Value::Null).0, "detection is on by default");
    }

    #[test]
    fn labels_come_from_the_app_table() {
        assert_eq!(label_of("zoom"), "Zoom");
        assert_eq!(label_of("browser"), "Browser");
        assert_eq!(label_of("nope"), "Call");
    }
}
