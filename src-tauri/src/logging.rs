//! Talaan's log file: what the app did and what went wrong, never what was said.
//!
//! Where: `~/Library/Logs/Talaan/` on the Mac and `%LOCALAPPDATA%\com.craftconceptsdigital.talaan\logs\` on Windows
//! (Tauri's `app_log_dir`), or `<data>/logs` when `TALAAN_DATA_DIR` is set, so dev runs never write into the real
//! log. Not `%LOCALAPPDATA%\Talaan`: that is the installer's `$INSTDIR` (program files; the uninstaller deletes its
//! own files there and would leave a log behind). Tauri's NSIS uninstaller deletes `$LOCALAPPDATA\<bundle id>` only when "Delete the application data" is
//! ticked, and never during an update. `talaan.log` rotates at 2 MB into
//! `talaan.1.log` .. `talaan.4.log` (about 10 MB at most).
//!
//! What: startup (version, OS, CPU, GPU plan), recorder start/stop/pause and device changes, call detection
//! decisions, transcription and summary jobs, update checks, and every error the app shows. Only Talaan's own
//! `log::` records are written (a library's could carry a request URL). Never logged: transcript text, summaries,
//! prompts, meeting titles, names or emails of people, API keys, the calendar address, the Notion secret.
//! Two guards: every line goes through `redact` (URLs cut to their host, emails, key-shaped tokens, quoted text and
//! the home folder), and a test reads every `log::` call in the source and refuses the fields that could carry
//! such content.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const FILE: &str = "talaan.log";
pub const MAX_BYTES: u64 = 2 * 1024 * 1024;
/// talaan.log plus four older files.
pub const KEEP: usize = 5;
/// Longest line written; anything longer is cut.
const MAX_LINE: usize = 1000;

/// The bundle identifier (`tauri.conf.json`), which names the Windows log folder.
pub const BUNDLE_ID: &str = "com.craftconceptsdigital.talaan";

/// The log folder: `TALAAN_DATA_DIR/logs` if set, else the OS log location.
pub fn resolve_dir(env_override: Option<String>, home: Option<PathBuf>, local_data: Option<PathBuf>) -> PathBuf {
    if let Some(p) = env_override.filter(|s| !s.trim().is_empty()) {
        return PathBuf::from(p).join("logs");
    }
    if cfg!(target_os = "macos") {
        home.unwrap_or_else(std::env::temp_dir).join("Library").join("Logs").join("Talaan")
    } else {
        local_data.unwrap_or_else(std::env::temp_dir).join(BUNDLE_ID).join("logs")
    }
}

/// The folder in use, once `init` ran.
pub fn dir() -> Option<&'static Path> {
    DIR.get().map(PathBuf::as_path)
}

static DIR: OnceLock<PathBuf> = OnceLock::new();
static HOME: OnceLock<Option<String>> = OnceLock::new();

/// A file that rotates: once the next line would take it past `max` bytes, `talaan.log` becomes `talaan.1.log`,
/// `.1` becomes `.2` and so on, and the oldest beyond `keep` files is deleted.
pub struct RotatingFile {
    dir: PathBuf,
    file: File,
    size: u64,
    max: u64,
    keep: usize,
}

fn numbered(dir: &Path, i: usize) -> PathBuf {
    if i == 0 { dir.join(FILE) } else { dir.join(format!("talaan.{i}.log")) }
}

impl RotatingFile {
    pub fn open(dir: &Path, max: u64, keep: usize) -> io::Result<RotatingFile> {
        fs::create_dir_all(dir)?;
        let file = OpenOptions::new().create(true).append(true).open(dir.join(FILE))?;
        let size = file.metadata()?.len();
        Ok(RotatingFile { dir: dir.to_path_buf(), file, size, max, keep: keep.max(1) })
    }

    pub fn write_line(&mut self, line: &str) -> io::Result<()> {
        let len = line.len() as u64 + 1;
        if self.size > 0 && self.size + len > self.max {
            self.rotate()?;
        }
        self.file.write_all(line.as_bytes())?;
        self.file.write_all(b"\n")?;
        self.size += len;
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        let _ = fs::remove_file(numbered(&self.dir, self.keep - 1));
        for i in (0..self.keep - 1).rev() {
            let from = numbered(&self.dir, i);
            if from.exists() {
                let _ = fs::rename(&from, numbered(&self.dir, i + 1));
            }
        }
        // A rename that failed (the file held open elsewhere) leaves talaan.log in place: start it over.
        self.file = OpenOptions::new().create(true).write(true).truncate(true).open(self.dir.join(FILE))?;
        self.size = 0;
        Ok(())
    }
}

/// The last `n` lines across the current and older files, oldest first.
pub fn tail(dir: &Path, n: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for i in 0..KEEP {
        if out.len() >= n {
            break;
        }
        let Ok(bytes) = fs::read(numbered(dir, i)) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        let take = lines.len().min(n - out.len());
        let mut newest = lines.split_off(lines.len() - take);
        newest.append(&mut out);
        out = newest;
    }
    out
}

/// "Delete all Talaan data": the log files go too.
pub fn delete_files() {
    if let Some(d) = dir() {
        for i in 0..KEEP {
            let _ = fs::remove_file(numbered(d, i));
        }
    }
}

struct FileLogger {
    out: Mutex<Option<RotatingFile>>,
    /// Debug builds also print each line to the terminal (`tauri dev`).
    echo: bool,
}

/// Only Talaan's own records (`talaan_lib::…`, `talaan::…`), at Info and above (Debug in debug builds).
fn wanted(target: &str, level: log::Level) -> bool {
    let max = if cfg!(debug_assertions) { log::Level::Debug } else { log::Level::Info };
    (target == "talaan_lib" || target.starts_with("talaan_lib::") || target == "talaan" || target.starts_with("talaan::")) && level <= max
}

fn short_target(target: &str) -> &str {
    target.strip_prefix("talaan_lib::").or_else(|| target.strip_prefix("talaan::")).unwrap_or(target)
}

/// One log line: time, level, where, and the redacted message.
pub fn format_line(time: &str, level: log::Level, target: &str, message: &str) -> String {
    format!("{time} {:<5} {}: {}", level.as_str(), short_target(target), redact(message))
}

impl log::Log for FileLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        wanted(m.target(), m.level())
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let time = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string();
        let line = format_line(&time, r.level(), r.target(), &r.args().to_string());
        if self.echo {
            eprintln!("{line}");
        }
        if let Some(f) = self.out.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            let _ = f.write_line(&line);
        }
    }

    fn flush(&self) {
        if let Some(f) = self.out.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            let _ = f.file.flush();
        }
    }
}

/// Starts the log (once). If the folder can't be written, lines still go to stderr in debug builds.
pub fn init(dir: &Path) {
    let _ = HOME.set(dirs::home_dir().map(|h| h.to_string_lossy().into_owned()).filter(|h| h.len() > 3));
    let file = RotatingFile::open(dir, MAX_BYTES, KEEP).ok();
    if file.is_some() {
        let _ = DIR.set(dir.to_path_buf());
    }
    let logger: &'static FileLogger = Box::leak(Box::new(FileLogger { out: Mutex::new(file), echo: cfg!(debug_assertions) }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(if cfg!(debug_assertions) { log::LevelFilter::Debug } else { log::LevelFilter::Info });
    }
    // A panic's location (and its message, redacted) before the release build aborts.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let msg = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_default();
        log::error!("panic at {at}: {msg}");
        previous(info);
    }));
}

// ---- Redaction ----

/// Removes what must never be in the log from a line: URLs keep only their host, emails and key-shaped tokens are
/// replaced, anything in double quotes or backticks is dropped (error messages quote the text they choked on), and
/// the home folder becomes `~`. The line is cut at 1000 bytes.
pub fn redact(s: &str) -> String {
    redact_with(s, HOME.get().and_then(|h| h.as_deref()))
}

pub fn redact_with(s: &str, home: Option<&str>) -> String {
    let mut s = s.to_string();
    if let Some(h) = home.filter(|h| !h.is_empty()) {
        s = s.replace(h, "~");
        if h.contains('\\') {
            s = s.replace(&h.replace('\\', "/"), "~");
        }
    }
    let s = drop_quoted(&s);
    let s = cut_urls(&s);
    let s = replace_words(&s);
    cut(&s, MAX_LINE)
}

fn cut(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// `"anything"` → `"…"`, `` `anything` `` → `` `…` ``. An unclosed quote drops the rest of the line.
fn drop_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut open: Option<char> = None;
    for c in s.chars() {
        match open {
            Some(q) if c == q => {
                out.push('…');
                out.push(c);
                open = None;
            }
            Some(_) => {}
            None if c == '"' || c == '`' => {
                out.push(c);
                open = Some(c);
            }
            None => out.push(c),
        }
    }
    if open.is_some() {
        out.push('…');
    }
    out
}

fn url_end(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | ')' | ']' | '}' | ',')
}

/// `https://user@host:443/path?q` → `https://host:443/…`.
fn cut_urls(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let rest = &lower[i..];
        let scheme = ["https://", "http://", "webcal://", "webcals://"].into_iter().find(|p| rest.starts_with(p));
        let Some(scheme) = scheme else {
            let c = s[i..].chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
            continue;
        };
        let start = i + scheme.len();
        let end = s[start..].find(url_end).map(|e| start + e).unwrap_or(s.len());
        let body = &s[start..end];
        let authority = body.split(['/', '?', '#']).next().unwrap_or("");
        let host = authority.rsplit('@').next().unwrap_or("");
        out.push_str(&s[i..start]);
        out.push_str(host);
        if body.len() > authority.len() {
            out.push_str("/…");
        }
        i = end;
    }
    out
}

/// Known key prefixes (OpenAI, Anthropic, Groq, Notion, Gemini, xAI, Cerebras, Perplexity, OpenRouter, Mistral
/// has none). Matched at the start of a token.
const KEY_PREFIXES: &[&str] = &["sk-", "sk_", "gsk_", "ntn_", "secret_", "aiza", "xai-", "csk-", "pplx-", "sk-or-"];

fn token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+' | '@' | '%')
}

fn is_email(t: &str) -> bool {
    let Some((user, domain)) = t.split_once('@') else { return false };
    !user.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

fn is_secret(t: &str) -> bool {
    let t = t.trim_matches(|c: char| c == '.' || c == '-');
    let lower = t.to_ascii_lowercase();
    if t.len() >= 16 && KEY_PREFIXES.iter().any(|p| lower.starts_with(p)) {
        return true;
    }
    // Long random-looking tokens: upper, lower and digits all present, no dots (a Windows package name like
    // `Microsoft.SkypeApp_kzf8qxf38zg5c` has them; keys don't).
    t.len() >= 32 && !t.contains('.') && t.chars().any(|c| c.is_ascii_uppercase()) && t.chars().any(|c| c.is_ascii_lowercase()) && t.chars().any(|c| c.is_ascii_digit())
}

fn replace_words(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if is_email(word) {
            out.push_str("[email]");
        } else if is_secret(word) {
            out.push_str("[secret]");
        } else {
            out.push_str(word);
        }
        word.clear();
    };
    for c in s.chars() {
        if token_char(c) {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// A device name without a person's name in it: "Miguel's AirPods Pro" → "…'s AirPods Pro",
/// "AirPods de Miguel" → "AirPods de …".
pub fn device(name: &str) -> String {
    let name = name.trim();
    for apostrophe in ["'s ", "’s "] {
        if let Some(i) = name.find(apostrophe) {
            return format!("…{}", &name[i..]);
        }
    }
    for of in [" de ", " of ", " von "] {
        if let Some(i) = name.rfind(of) {
            let after = &name[i + of.len()..];
            if after.chars().next().is_some_and(char::is_uppercase) && !after.contains(['(', ')']) {
                return format!("{}{of}…", &name[..i]);
            }
        }
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_keep_only_their_host() {
        let line = "calendar: couldn't reach https://outlook.office365.com/owa/calendar/abc123/def456/calendar.ics?x=1 (timeout)";
        assert_eq!(redact_with(line, None), "calendar: couldn't reach https://outlook.office365.com/… (timeout)");
        assert_eq!(redact_with("webcal://p01-caldav.icloud.com/published/2/MTIz", None), "webcal://p01-caldav.icloud.com/…");
        assert_eq!(redact_with("http://user:pw@localhost:11434", None), "http://localhost:11434");
        assert_eq!(redact_with("HTTPS://API.EXAMPLE.COM/v1?key=AIzaSyA1b2C3d4E5f6G7h8I9j0", None), "HTTPS://API.EXAMPLE.COM/…");
    }

    #[test]
    fn emails_and_keys_are_replaced() {
        assert_eq!(redact_with("attendee ana.cruz@example.com joined", None), "attendee [email] joined");
        for key in ["sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWx", "sk-proj-abcdefghijklmnop1234", "gsk_0123456789abcdefABCD", "ntn_123456789012ABCDEFGHIJ", "secret_abcdefghijklmnop", "AIzaSyA1b2C3d4E5f6G7h8I9j0"] {
            let out = redact_with(&format!("key {key} refused"), None);
            assert_eq!(out, "key [secret] refused", "{key}");
        }
        assert_eq!(redact_with("Bearer Ab3dEf6hIj9lMn2pQr5tUv8xYz1bCd4fGh7", None), "Bearer [secret]");
    }

    #[test]
    fn ordinary_words_ids_and_model_names_stay() {
        let line = "transcriber: meeting 2026-10-16_1430_ab12 done with whisper-large-v3-turbo-q5_0 in 42.1s (exit code 5, 0xC0000135)";
        assert_eq!(redact_with(line, None), line);
        let sha = "sha256 3f2a9d8c1b0e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a";
        assert_eq!(redact_with(sha, None), sha, "lowercase hex is not a secret");
        assert_eq!(redact_with("Qwen3.5 4B on NVIDIA GeForce RTX 5070 Ti", None), "Qwen3.5 4B on NVIDIA GeForce RTX 5070 Ti");
        assert_eq!(redact_with("mic users: Microsoft.SkypeApp_kzf8qxf38zg5c, us.zoom.xos", None), "mic users: Microsoft.SkypeApp_kzf8qxf38zg5c, us.zoom.xos");
    }

    #[test]
    fn quoted_text_is_dropped() {
        assert_eq!(
            redact_with("the reply wasn't valid JSON (invalid type: string \"We agreed to ship Friday\", expected a map)", None),
            "the reply wasn't valid JSON (invalid type: string \"…\", expected a map)"
        );
        assert_eq!(redact_with("byte index 3 is not a char boundary of `Salary numbers for Ana`", None), "byte index 3 is not a char boundary of `…`");
        assert_eq!(redact_with("unclosed \"secret plans", None), "unclosed \"…");
        assert_eq!(redact_with("Couldn't start: it's busy", None), "Couldn't start: it's busy");
    }

    #[test]
    fn the_home_folder_becomes_a_tilde() {
        assert_eq!(redact_with(r"no file C:\Users\ana\AppData\Roaming\Talaan\models", Some(r"C:\Users\ana")), r"no file ~\AppData\Roaming\Talaan\models");
        assert_eq!(redact_with("no file C:/Users/ana/x", Some(r"C:\Users\ana")), "no file ~/x");
        assert_eq!(redact_with("/Users/ana/Library/Logs/Talaan", Some("/Users/ana")), "~/Library/Logs/Talaan");
    }

    #[test]
    fn long_lines_are_cut() {
        let out = redact_with(&"é".repeat(800), None);
        assert!(out.len() <= MAX_LINE + "…".len(), "{}", out.len());
        assert!(out.ends_with('…'));
    }

    #[test]
    fn device_names_lose_their_owner() {
        assert_eq!(device("Miguel's AirPods Pro"), "…'s AirPods Pro");
        assert_eq!(device("Ana’s MacBook Pro Microphone"), "…’s MacBook Pro Microphone");
        assert_eq!(device("AirPods de Miguel"), "AirPods de …");
        assert_eq!(device("Microphone (Realtek(R) Audio)"), "Microphone (Realtek(R) Audio)");
        assert_eq!(device("MacBook Pro Microphone"), "MacBook Pro Microphone");
    }

    #[test]
    fn only_talaans_own_records_are_kept() {
        assert!(wanted("talaan_lib::commands::recorder", log::Level::Info));
        assert!(wanted("talaan_lib", log::Level::Error));
        assert!(!wanted("reqwest::connect", log::Level::Error), "a library's line could carry a URL");
        assert!(!wanted("tauri_plugin_updater", log::Level::Info));
        assert!(!wanted("talaan_lib::x", log::Level::Trace));
        let line = format_line("2026-10-16T14:30:00.000-04:00", log::Level::Warn, "talaan_lib::commands::recorder", "mic error: \"x\"");
        assert_eq!(line, "2026-10-16T14:30:00.000-04:00 WARN  commands::recorder: mic error: \"…\"");
    }

    #[test]
    fn records_reach_the_file_redacted_and_filtered() {
        use log::Log;
        let dir = tempfile::tempdir().unwrap();
        let logger = FileLogger { out: Mutex::new(Some(RotatingFile::open(dir.path(), MAX_BYTES, KEEP).unwrap())), echo: false };
        let write = |target: &str, level, msg: std::fmt::Arguments| logger.log(&log::Record::builder().target(target).level(level).args(msg).build());
        write("talaan_lib::commands::calendar", log::Level::Warn, format_args!("calendar download failed: https://calendar.example.com/private/abc.ics for ana@example.com"));
        write("reqwest::connect", log::Level::Warn, format_args!("GET https://calendar.example.com/private/abc.ics"));
        logger.flush();
        let lines = tail(dir.path(), 10);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].ends_with("WARN  commands::calendar: calendar download failed: https://calendar.example.com/… for [email]"), "{}", lines[0]);
    }

    #[test]
    fn the_log_lives_in_the_os_log_folder() {
        let home = Some(PathBuf::from("/Users/ana"));
        let local = Some(PathBuf::from(r"C:\Users\ana\AppData\Local"));
        let d = resolve_dir(None, home.clone(), local.clone());
        if cfg!(target_os = "macos") {
            assert_eq!(d, PathBuf::from("/Users/ana/Library/Logs/Talaan"));
        } else {
            assert_eq!(d, PathBuf::from(r"C:\Users\ana\AppData\Local").join("com.craftconceptsdigital.talaan").join("logs"));
        }
        assert_eq!(resolve_dir(Some("/tmp/dev".into()), home.clone(), local.clone()), PathBuf::from("/tmp/dev").join("logs"));
        assert_eq!(resolve_dir(Some(" ".into()), home, local), d);
    }

    /// The Windows folder follows the bundle identifier, which the uninstaller's "Delete the application data" uses.
    #[test]
    fn the_windows_folder_is_the_bundle_identifier() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], BUNDLE_ID);
    }

    #[test]
    fn the_file_rotates_and_keeps_five() {
        let dir = tempfile::tempdir().unwrap();
        let mut f = RotatingFile::open(dir.path(), 100, 5).unwrap();
        for i in 0..40 {
            f.write_line(&format!("line {i:02} ......................")).unwrap(); // 33 bytes with the newline
        }
        let mut names: Vec<String> = fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["talaan.1.log", "talaan.2.log", "talaan.3.log", "talaan.4.log", "talaan.log"]);
        for i in 0..KEEP {
            assert!(fs::metadata(numbered(dir.path(), i)).unwrap().len() <= 100);
        }
        // The newest lines come back in order, across files.
        let t = tail(dir.path(), 5);
        assert_eq!(t.len(), 5);
        assert!(t[4].starts_with("line 39"), "{t:?}");
        assert!(t[0].starts_with("line 35"), "{t:?}");
        // Reopening appends.
        drop(f);
        let mut f = RotatingFile::open(dir.path(), 100, 5).unwrap();
        f.write_line("again").unwrap();
        assert_eq!(tail(dir.path(), 1), ["again"]);
        assert!(tail(&dir.path().join("missing"), 10).is_empty());
    }

    /// Fields that can hold what was said, who was there, or a secret. A `log::` call may not name them.
    const FORBIDDEN: &[&str] = &[
        "title", "titles", "text", "texts", "recap", "summary", "decisions", "action_items", "open_questions", "prompt", "prompts", "attendee",
        "attendees", "email", "emails", "key", "api_key", "secret", "url", "address", "body", "note", "notes", "notice", "transcript", "lines",
        "your_name", "yourName", "workspace", "destination", "reply", "answer", "question", "passages", "events",
    ];

    /// Every `log::…!(` call in the source, with its file and line.
    fn log_calls() -> Vec<(String, usize, String)> {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for e in fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        let mut files = vec![];
        walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let mut calls = vec![];
        for f in files {
            let src = fs::read_to_string(&f).unwrap();
            let name = f.file_name().unwrap().to_string_lossy().into_owned();
            for level in ["error", "warn", "info", "debug", "trace"] {
                let needle = format!("log::{level}!(");
                for (i, _) in src.match_indices(&needle) {
                    // This file's own description of the rule isn't a call.
                    if name == "logging.rs" && src[..i].lines().last().is_some_and(|l| l.trim_start().starts_with("//")) {
                        continue;
                    }
                    let rest = &src[i + needle.len()..];
                    let mut depth = 1;
                    let mut end = rest.len();
                    for (j, c) in rest.char_indices() {
                        match c {
                            '(' => depth += 1,
                            ')' => {
                                depth -= 1;
                                if depth == 0 {
                                    end = j;
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    let line = src[..i].lines().count();
                    calls.push((name.clone(), line, rest[..end].to_string()));
                }
            }
        }
        calls
    }

    /// The identifiers a call uses: words outside string literals, plus `{name}` / `{x.name}` placeholders.
    fn identifiers(call: &str) -> Vec<String> {
        let mut code = String::new();
        let mut in_str = false;
        let mut prev = ' ';
        let mut placeholder: Option<String> = None;
        for c in call.chars() {
            if in_str {
                match (&mut placeholder, c) {
                    (None, '{') => placeholder = Some(String::new()),
                    (Some(p), '}') => {
                        code.push(' ');
                        code.push_str(p.split(':').next().unwrap_or(""));
                        code.push(' ');
                        placeholder = None;
                    }
                    (Some(p), c) => p.push(c),
                    _ => {}
                }
                if c == '"' && prev != '\\' {
                    in_str = false;
                    placeholder = None;
                }
            } else if c == '"' {
                in_str = true;
            } else {
                code.push(c);
            }
            prev = c;
        }
        code.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty()).map(str::to_string).collect()
    }

    /// An installed app has no terminal: app code logs through `log::`, and only tests print. (This file's echo
    /// for `tauri dev` is the one exception.)
    #[test]
    fn app_code_never_prints_to_stderr() {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for e in fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        let mut files = vec![];
        walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let mut bad = vec![];
        for f in files {
            if f.file_name().is_some_and(|n| n == "logging.rs") {
                continue;
            }
            let src = fs::read_to_string(&f).unwrap().replace("\r\n", "\n");
            let app_code = &src[..src.find("#[cfg(test)]\nmod ").unwrap_or(src.len())];
            for (i, _) in app_code.match_indices("eprintln!(") {
                bad.push(format!("{}:{}", f.display(), app_code[..i].lines().count()));
            }
        }
        assert!(bad.is_empty(), "eprintln! in app code (use log::warn! / log::error!):\n{}", bad.join("\n"));
    }

    #[test]
    fn identifiers_skip_literal_text_but_read_placeholders() {
        assert_eq!(identifiers(r#""summary for {id} took {secs:.1}s", engine.label()"#), ["id", "secs", "engine", "label"]);
        assert_eq!(identifiers(r#""meeting {m.title}""#), ["m", "title"]);
    }

    #[test]
    fn no_log_call_names_meeting_content_people_or_secrets() {
        let calls = log_calls();
        assert!(calls.len() >= 20, "the scan found the log calls ({})", calls.len());
        let mut bad = vec![];
        for (file, line, call) in &calls {
            for id in identifiers(call) {
                if FORBIDDEN.contains(&id.as_str()) {
                    bad.push(format!("{file}:{line} uses `{id}`: {call}"));
                }
            }
        }
        assert!(bad.is_empty(), "log calls that could write meeting content, names or secrets:\n{}", bad.join("\n"));
    }
}
