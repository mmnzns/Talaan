//! Talaan's log file: what the app did and what went wrong, never what was said.
//!
//! Where: `~/Library/Logs/Talaan/` on the Mac (the system's log folder, named like the app; Tauri's `app_log_dir`
//! would use the bundle id) and `%LOCALAPPDATA%\com.craftconceptsdigital.talaan\logs\` on Windows (what Tauri's
//! `app_log_dir` gives), or `<data>/logs` when `TALAAN_DATA_DIR` is set, so dev runs never write into the real log.
//! Not `%LOCALAPPDATA%\Talaan` on Windows: that is the installer's `$INSTDIR` (program files; the uninstaller
//! deletes its own files there and would leave a log behind). Tauri's NSIS uninstaller deletes
//! `$LOCALAPPDATA\<bundle id>` only when "Delete the application data" is ticked, and never during an update.
//! `talaan.log` rotates at 2 MB into `talaan.1.log` .. `talaan.4.log` (about 10 MB at most).
//!
//! What: startup (version, OS, CPU, GPU plan), recorder start/stop/pause and device changes, call detection
//! decisions, transcription and summary jobs, update checks, and every error the app shows. Only Talaan's own
//! records are written (`talaan_lib`, `talaan`, and the `talaan_stt` speech crate, which logs nothing today but would
//! go through the same scrubbing); a library's could carry a request URL. Never logged: transcript text,
//! summaries, prompts, meeting titles, names or emails of people, API keys, the calendar address, the Notion secret.
//! Three guards: errors and other free text go through `text` (an app message is logged as its key only), every
//! line goes through `redact` (URLs, emails, key-shaped tokens, quoted text, profile folders), and `log_scan` reads
//! every log call in the source and refuses the ones that could carry such content.

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
        // A rename that failed (the file held open elsewhere) leaves talaan.log in place: start it over. If even
        // that fails, keep writing to the file we have and count from zero, so the next try is another 2 MB away
        // (not on every line).
        if let Ok(f) = OpenOptions::new().create(true).write(true).truncate(true).open(self.dir.join(FILE)) {
            self.file = f;
        }
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

/// Talaan's own crates: the app, its binary and the speech crate (`log_scan` checks their calls).
const OWN: &[&str] = &["talaan_lib", "talaan", "talaan_stt"];

/// Only Talaan's own records, at Info and above (Debug in debug builds).
fn wanted(target: &str, level: log::Level) -> bool {
    let max = if cfg!(debug_assertions) { log::Level::Debug } else { log::Level::Info };
    let crate_name = target.split("::").next().unwrap_or("");
    OWN.contains(&crate_name) && level <= max
}

fn short_target(target: &str) -> &str {
    target.strip_prefix("talaan_lib::").or_else(|| target.strip_prefix("talaan::")).unwrap_or(target)
}

/// A panic's source file without the build machine's folders: a dependency's path from its crate folder on
/// (`log-0.4.34/src/lib.rs`), Talaan's own as written (`src\lib.rs`).
pub fn panic_location(file: &str) -> String {
    let f = file.replace('\\', "/");
    for marker in ["/registry/src/", "/git/checkouts/", "/rustc/"] {
        if let Some(i) = f.find(marker) {
            let rest = &f[i + marker.len()..];
            // registry/src/<index>/<crate>/…, git/checkouts/<repo>/<rev>/…, rustc/<hash>/library/…: drop the first part.
            return rest.split_once('/').map(|(_, r)| r).unwrap_or(rest).to_string();
        }
    }
    redact(file)
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
        let at = info.location().map(|l| format!("{}:{}", panic_location(l.file()), l.line())).unwrap_or_default();
        let msg = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_default();
        log::error!("panic at {at}: {}", text(&msg));
        previous(info);
    }));
}

// ---- Redaction ----

/// A non-literal value for a log call, made safe: one of the app's own messages becomes its key (`[ai.http_error]`;
/// its placeholders can hold what a provider, Notion or a model said, so none of them is kept), anything else is
/// redacted. Every error or other free text in a `log::` call goes through this (a test checks).
pub fn text(m: impl std::fmt::Display) -> Text {
    let s = m.to_string();
    Text(match crate::i18n::key_for(&s) {
        Some(key) => format!("[{key}]"),
        None => redact(&s),
    })
}

pub struct Text(String);

impl std::fmt::Display for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Removes what must never be in the log from a line: URLs keep only their host, emails and key-shaped tokens are
/// replaced, anything in double quotes or backticks is dropped (error messages quote the text they choked on), and
/// the home folder becomes `~`. The line is cut at 1000 bytes.
pub fn redact(s: &str) -> String {
    redact_with(s, HOME.get().and_then(|h| h.as_deref()))
}

pub fn redact_with(s: &str, home: Option<&str>) -> String {
    // One line per record.
    let mut s = s.replace("\r\n", " | ").replace(['\n', '\r'], " | ");
    if let Some(h) = home.filter(|h| !h.is_empty()) {
        s = replace_ignore_case(&s, h, "~");
        if h.contains('\\') {
            s = replace_ignore_case(&s, &h.replace('\\', "/"), "~");
        }
    }
    let s = scrub_profiles(&s);
    let s = drop_quoted(&s);
    let s = cut_urls(&s);
    let s = replace_words(&s);
    cut(&s, MAX_LINE)
}

/// `s` with every ASCII-case-insensitive match of `from` replaced.
fn replace_ignore_case(s: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return s.to_string();
    }
    let (hay, needle) = (s.to_ascii_lowercase(), from.to_ascii_lowercase());
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for (i, _) in hay.match_indices(&needle) {
        if i < last {
            continue;
        }
        out.push_str(&s[last..i]);
        out.push_str(to);
        last = i + needle.len();
    }
    out.push_str(&s[last..]);
    out
}

fn path_end(c: char) -> bool {
    matches!(c, '\\' | '/') || c.is_whitespace() || matches!(c, '"' | '\'' | '`' | ')' | ']' | ',' | ';' | ':')
}

/// Any user profile in a path, whoever's it is and however it is written: `C:\Users\<name>` (also with `/`, in any
/// case, and 8.3 names like `ANACRU~1`), `/Users/<name>` and `/home/<name>` become `~`; a network path
/// `\\server\share\<folder>` (a redirected profile) becomes `\\…`.
fn scrub_profiles(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let rest = &lower[i..];
        let prev_ok = i == 0 || !(bytes[i - 1] as char).is_ascii_alphanumeric();
        // C:\Users\name or C:/Users/name
        let drive = prev_ok && rest.len() > 9 && bytes[i].is_ascii_alphabetic() && (rest[1..].starts_with(":\\users\\") || rest[1..].starts_with(":/users/"));
        // /Users/name, /home/name
        let unix = prev_ok && (rest.starts_with("/users/") || rest.starts_with("/home/"));
        // \\server\share\folder
        let unc = rest.starts_with("\\\\") && (i == 0 || bytes[i - 1].is_ascii_whitespace() || b"(\"'`=:,".contains(&bytes[i - 1]));
        if drive || unix {
            let start = i + if drive { 9 } else if rest.starts_with("/users/") { 7 } else { 6 };
            let end = s[start..].find(path_end).map(|e| start + e).unwrap_or(s.len());
            if end > start {
                out.push('~');
                i = end;
                continue;
            }
        }
        if unc {
            // Server, share and the folder under it (the profile's name in a redirected profile).
            let mut j = i + 2;
            let mut parts = 0;
            while parts < 3 {
                match s[j..].find(|c: char| c == '\\' || c == '/' || c.is_whitespace() || matches!(c, '"' | '\'' | '`' | ')' | ',')) {
                    Some(e) if matches!(s[j + e..].chars().next(), Some('\\' | '/')) => {
                        j += e + 1;
                        parts += 1;
                    }
                    Some(e) => {
                        j += e;
                        break;
                    }
                    None => {
                        j = s.len();
                        break;
                    }
                }
            }
            out.push_str("\\\\…");
            i = if parts == 3 { j - 1 } else { j };
            continue;
        }
        let c = s[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
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

/// The closing mark for a quote that opens with `c`.
fn closing(c: char) -> Option<char> {
    Some(match c {
        '"' => '"',
        '`' => '`',
        '\'' => '\'',
        '“' => '”',
        '‘' => '’',
        '«' => '»',
        '„' => '“',
        _ => return None,
    })
}

/// Drops whatever is quoted: `"anything"` → `"…"`, also `` `…` ``, `'…'`, `“…”`, `‘…’`, `«…»`. Escapes inside a
/// quote (`\"`, as `{:?}` writes them) don't close it, and an unclosed quote drops the rest of the line. A single
/// quote or ’ only opens after a space or bracket and only closes before one, so apostrophes ("couldn't",
/// "l’audio") stay.
fn drop_quoted(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let boundary = |i: Option<usize>| i.and_then(|i| chars.get(i)).is_none_or(|c| !c.is_alphanumeric());
    let mut out = String::with_capacity(s.len());
    let mut open: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match open {
            Some(_) if c == '\\' => i += 1, // the escaped character can't close the quote
            Some(close) if c == close && (!matches!(c, '\'' | '’') || boundary(Some(i + 1))) => {
                out.push('…');
                out.push(c);
                open = None;
            }
            Some(_) => {}
            None => {
                let after_space = i.checked_sub(1).is_none_or(|j| chars[j].is_whitespace() || "([{=:,".contains(chars[j]));
                let opens = closing(c).filter(|_| c != '\'' || after_space);
                out.push(c);
                open = opens;
            }
        }
        i += 1;
    }
    if open.is_some() {
        out.push('…');
    }
    out
}

fn url_end(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | ')' | ']' | '}' | ',')
}

/// Hosts a log line may name: this computer, and the public services Talaan talks to (updates, models, the AI
/// providers, Notion, the big calendar services). Any other host (a company's AI server, a private calendar host)
/// becomes `[host]`.
const KNOWN_HOSTS: &[&str] = &[
    "github.com", "githubusercontent.com", "huggingface.co", "hf.co", "anthropic.com", "openai.com", "googleapis.com", "groq.com", "cerebras.ai",
    "mistral.ai", "openrouter.ai", "notion.com", "office365.com", "outlook.com", "live.com", "google.com", "icloud.com",
];

fn known_host(authority: &str) -> bool {
    let host = authority.rsplit_once(':').filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit())).map(|(h, _)| h).unwrap_or(authority);
    let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
    if matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1" | "0.0.0.0") || host.ends_with(".localhost") {
        return true;
    }
    KNOWN_HOSTS.iter().any(|k| host == *k || host.ends_with(&format!(".{k}")))
}

/// A host written without a scheme at the start of `s` (`outlook.office365.com/owa/…`): its length, when what
/// follows is a path. Needs a dot and a letters-only last label, so `log-0.4.34/src` and `src/lib.rs` aren't hosts.
fn bare_host(s: &str) -> Option<usize> {
    let end = s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':'))?;
    if !s[end..].starts_with('/') {
        return None;
    }
    let host = s[..end].split(':').next().unwrap_or("");
    let last = host.rsplit('.').next().unwrap_or("");
    (host.contains('.') && last.len() >= 2 && last.chars().all(|c| c.is_ascii_alphabetic())).then_some(end)
}

/// `https://user@host:443/path?q` → `https://host:443/…`; a host without a scheme the same way; `file://…` loses
/// its whole path. Hosts that aren't this computer or a known service become `[host]`.
fn cut_urls(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let rest = &lower[i..];
        let at_word = i == 0 || !s[..i].ends_with(|c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '@' | '/' | '\\' | '_'));
        if rest.starts_with("file://") {
            let end = s[i..].find(url_end).map(|e| i + e).unwrap_or(s.len());
            out.push_str("file://…");
            i = end;
            continue;
        }
        let scheme = ["https://", "http://", "webcal://", "webcals://"].into_iter().find(|p| rest.starts_with(p));
        let scheme = match scheme {
            Some(p) => p,
            None if at_word && bare_host(&s[i..]).is_some() => "",
            None => {
                let c = s[i..].chars().next().unwrap();
                out.push(c);
                i += c.len_utf8();
                continue;
            }
        };
        let start = i + scheme.len();
        let end = s[start..].find(url_end).map(|e| start + e).unwrap_or(s.len());
        let body = &s[start..end];
        let authority = body.split(['/', '?', '#']).next().unwrap_or("");
        let host = authority.rsplit('@').next().unwrap_or("");
        out.push_str(&s[i..start]);
        out.push_str(if known_host(host) { host } else { "[host]" });
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
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+' | '@' | '%' | '/' | '=')
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
    // Long random-looking tokens, 32 characters or more with no dots (a Windows package name like
    // `Microsoft.SkypeApp_kzf8qxf38zg5c` has them; keys don't), and not a path (one `/` per dozen characters at most):
    // hex in any case, base64, or upper, lower and digits mixed. Long model names with many `-` stay.
    let t = t.trim_end_matches('=');
    if t.len() < 32 || t.contains('.') || t.matches('/').count() * 12 > t.len() {
        return false;
    }
    let digit = t.chars().any(|c| c.is_ascii_digit());
    let letter = t.chars().any(|c| c.is_ascii_alphabetic());
    let hex = t.chars().all(|c| c.is_ascii_hexdigit());
    let mixed = t.chars().any(|c| c.is_ascii_uppercase()) && t.chars().any(|c| c.is_ascii_lowercase()) && digit;
    let separators = t.matches(['-', '_']).count();
    hex || mixed || (digit && letter && separators <= 2)
}

/// `[email]` for an address (sentence punctuation after it isn't part of it; `%40` is an encoded `@`).
fn email_or_word(piece: &str) -> String {
    let core = piece.trim_end_matches(['.', '-', '+', '%']);
    if is_email(&replace_ignore_case(core, "%40", "@")) { format!("[email]{}", &piece[core.len()..]) } else { piece.to_string() }
}

fn replace_words(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut word = String::new();
    // The word before: whatever follows `Bearer` or `Basic` is a credential, however it looks.
    let mut previous = String::new();
    let mut flush = |word: &mut String, out: &mut String| {
        if word.is_empty() {
            return;
        }
        let after_scheme = previous.eq_ignore_ascii_case("bearer") || previous.eq_ignore_ascii_case("basic");
        previous = word.clone();
        if after_scheme {
            out.push_str("[secret]");
            word.clear();
            return;
        }
        if is_secret(word) {
            out.push_str("[secret]");
        } else {
            // Emails between `=` and `/` (a query value, a path) are found piece by piece.
            for piece in word.split_inclusive(['=', '/']) {
                let body = piece.strip_suffix(['=', '/']).unwrap_or(piece);
                out.push_str(&email_or_word(body));
                out.push_str(&piece[body.len()..]);
            }
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

/// Devices people name after themselves, for the apostrophe-less genitive ("Miguels AirPods").
const PERSONAL_DEVICES: &[&str] = &["airpods", "iphone", "ipad", "macbook", "imac", "mac", "galaxy", "pixel", "buds", "beats", "headset", "headphones", "kopfhörer", "mikrofon", "telefon", "handy"];

/// A device name without a person's name in it: "Miguel's AirPods Pro" → "…'s AirPods Pro",
/// "AirPods de Miguel" → "AirPods de …".
pub fn device(name: &str) -> String {
    let name = name.trim();
    // Windows wraps the device: "Micrófono (AirPods de Miguel)". Clean the part in brackets, keep the rest.
    if let Some(open) = name.find('(').filter(|_| name.ends_with(')')) {
        let inner = &name[open + 1..name.len() - 1];
        let clean = device(inner);
        if clean != inner.trim() {
            return format!("{}({clean})", &name[..open]);
        }
    }
    // German genitive without an apostrophe: "Miguels AirPods".
    let mut words = name.splitn(2, ' ');
    if let (Some(first), Some(rest)) = (words.next(), words.next()) {
        let next = rest.split(' ').next().unwrap_or("").to_lowercase();
        let owner = first.len() > 2 && first.ends_with('s') && !first.contains(['\'', '’']) && first.chars().next().is_some_and(char::is_uppercase);
        if owner && PERSONAL_DEVICES.iter().any(|d| next.starts_with(d)) {
            return format!("… {rest}");
        }
    }
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
        assert_eq!(redact_with("HTTPS://GENERATIVELANGUAGE.GOOGLEAPIS.COM/v1?key=AIzaSyA1b2C3d4E5f6G7h8I9j0", None), "HTTPS://GENERATIVELANGUAGE.GOOGLEAPIS.COM/…");
        // A host that isn't this computer or a known public service (a company's AI server) isn't named.
        assert_eq!(redact_with("can't reach https://llm.acme.internal:8443/v1/chat", None), "can't reach https://[host]/…");
        assert_eq!(redact_with("http://192.168.1.20:11434", None), "http://[host]");
        assert_eq!(redact_with("http://localhost:1234/v1", None), "http://localhost:1234/…");
    }

    #[test]
    fn emails_and_keys_are_replaced() {
        assert_eq!(redact_with("attendee ana.cruz@example.com joined", None), "attendee [email] joined");
        assert_eq!(redact_with("invite sent to ana@example.com.", None), "invite sent to [email].");
        assert_eq!(redact_with("to ana@example.com, bob@example.org; done", None), "to [email], [email]; done");
        assert_eq!(redact_with("q=ana%40example.com&x", None), "q=[email]&x");
        assert_eq!(redact_with("q=ANA%40EXAMPLE.COM.", None), "q=[email].");
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
        assert_eq!(redact_with("model whisper-large-v3-turbo-q5_0-ggml-model-q8_0-extra-long-name ok", None), "model whisper-large-v3-turbo-q5_0-ggml-model-q8_0-extra-long-name ok");
        assert_eq!(redact_with("read ~/Library/Application Support/Talaan/models/parakeet/encoder-model", None), "read ~/Library/Application Support/Talaan/models/parakeet/encoder-model");
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
    fn escaped_and_typographic_quotes_dont_leak() {
        // serde's errors and `{:?}` escape inner quotes.
        assert_eq!(redact_with(r#"invalid type: string "We agreed \"Ana Cruz is fired\" today", expected a map"#, None), r#"invalid type: string "…", expected a map"#);
        assert_eq!(redact_with(r#"Err("say \"hi\" to \\ Ana")"#, None), r#"Err("…")"#);
        assert_eq!(redact_with("the reply said “Ana Cruz is fired” and stopped", None), "the reply said “…” and stopped");
        assert_eq!(redact_with("it quoted ‘the budget’s fine’ back", None), "it quoted ‘…’ back");
        assert_eq!(redact_with("it quoted 'Ana is fired' back", None), "it quoted '…' back");
        assert_eq!(redact_with("Notion said «page Ana» no", None), "Notion said «…» no");
        // Apostrophes are not quotes.
        assert_eq!(redact_with("couldn't start: l’audio isn't there", None), "couldn't start: l’audio isn't there");
        assert_eq!(redact_with("microphone switched to …'s AirPods", None), "microphone switched to …'s AirPods");
    }

    #[test]
    fn app_messages_are_logged_by_key_and_never_with_their_details() {
        let leaked = crate::i18n::tr_err("error.summary_bad_output", "invalid type: string \"Ana Cruz is fired\"");
        assert_eq!(text(&leaked).to_string(), "[error.summary_bad_output]");
        let provider = crate::i18n::tr_with("ai.http_error", &[("provider", "Claude"), ("status", "400"), ("detail", "prompt mentions Ana Cruz")]);
        assert_eq!(text(&provider).to_string(), "[ai.http_error]");
        let notion = crate::i18n::tr_with("notion.said", &[("message", "Page Board meeting with Ana not found")]);
        assert_eq!(text(&notion).to_string(), "[notion.said]");
        for lang in [crate::i18n::Lang::Fr, crate::i18n::Lang::Es] {
            let m = crate::i18n::tr_in(lang, "ai.http_error", &[("provider", "Claude"), ("status", "500"), ("detail", "Ana")]);
            assert_eq!(text(&m).to_string(), "[ai.http_error]", "{m}");
        }
        // Not one of the app's messages: redacted.
        assert_eq!(text("The system cannot find the file specified. (os error 2)").to_string(), "The system cannot find the file specified. (os error 2)");
        // A refusal that isn't an app message (an Ask parse error) isn't logged at all.
        let ask = crate::storage::StorageError::Refused("the reply wasn't valid JSON (invalid type: string \"Ana\")".into());
        assert_eq!(ask.for_log(), "(message not logged)");
        assert_eq!(crate::storage::StorageError::Refused(crate::i18n::tr("error.no_log")).for_log(), "[error.no_log]");
        assert_eq!(crate::storage::StorageError::NotFound("2026-10-16_1430_ab12".into()).for_log(), "not found: 2026-10-16_1430_ab12");
    }

    #[test]
    fn hex_base64_and_bearer_credentials_are_hidden() {
        assert_eq!(redact_with("sha256 3f2a9d8c1b0e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a done", None), "sha256 [secret] done");
        assert_eq!(redact_with("key 0123456789abcdef0123456789abcdef", None), "key [secret]");
        assert_eq!(redact_with("token dGhpcyBpcyBhIHNlY3JldCB0b2tlbiB2YWx1ZQ== ok", None), "token [secret] ok");
        assert_eq!(redact_with("token abcdefghij0123456789abcdefghij0123/xyz+ab ok", None), "token [secret] ok");
        assert_eq!(redact_with("Authorization: Bearer short-1 and Basic YWxhZGRpbjpvcGVu", None), "Authorization: Bearer [secret] and Basic [secret]");
        assert_eq!(redact_with("bearer x", None), "bearer [secret]");
    }

    #[test]
    fn hosts_without_a_scheme_and_file_urls_are_cut() {
        assert_eq!(redact_with("GET outlook.office365.com/owa/calendar/abc/def/calendar.ics failed", None), "GET outlook.office365.com/… failed");
        assert_eq!(redact_with("at llm.acme.internal:8443/v1/chat", None), "at [host]/…");
        assert_eq!(redact_with("open file:///C:/Users/ana/Documents/notes.txt now", None), "open file://… now");
        // Not hosts.
        assert_eq!(redact_with("in log-0.4.34/src/lib.rs:10 and src/commands/recorder.rs:5", None), "in log-0.4.34/src/lib.rs:10 and src/commands/recorder.rs:5");
        assert_eq!(redact_with("models/whisper.cpp/ggml-small.bin", None), "models/whisper.cpp/ggml-small.bin");
    }

    #[test]
    fn panic_locations_drop_the_build_machines_folders() {
        assert_eq!(panic_location(r"C:\Users\ana\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\log-0.4.34\src\lib.rs"), "log-0.4.34/src/lib.rs");
        assert_eq!(panic_location("/Users/ana/.cargo/git/checkouts/whisper-rs-abc/1a2b3c/src/lib.rs"), "1a2b3c/src/lib.rs");
        assert_eq!(panic_location("/rustc/90b35a6239c3d8bdabc530a6a0816f7ff89a0aaf/library/core/src/str/mod.rs"), "library/core/src/str/mod.rs");
        assert_eq!(panic_location(r"src\commands\recorder.rs"), r"src\commands\recorder.rs");
    }

    #[test]
    fn a_failed_rotation_does_not_retry_on_every_line() {
        let dir = tempfile::tempdir().unwrap();
        let mut f = RotatingFile::open(dir.path(), 64, 3).unwrap();
        // Rotation can't rename or reopen in a folder that is gone; writing carries on into the open file.
        f.write_line("first line, long enough to fill up").unwrap();
        f.dir = dir.path().join("gone");
        f.write_line("second line, long enough to rotate").unwrap();
        assert_eq!(f.size, "second line, long enough to rotate".len() as u64 + 1, "counting restarted from zero");
        f.write_line("third").unwrap();
        assert!(f.size < 64, "no rotation attempt on the next line");
        assert_eq!(tail(dir.path(), 3), ["first line, long enough to fill up", "second line, long enough to rotate", "third"]);
    }

    #[test]
    fn talaans_crates_are_logged_and_libraries_are_not() {
        assert!(wanted("talaan_stt::engine", log::Level::Warn));
        assert!(wanted("talaan", log::Level::Info));
        assert!(!wanted("talaan_stt_extra", log::Level::Warn));
        assert!(!wanted("hyper::client", log::Level::Error));
    }

    #[test]
    fn multi_line_messages_become_one_line() {
        assert_eq!(redact_with("helper stopped: exit code 5\nggml_vulkan: Device lost\r\nnext", None), "helper stopped: exit code 5 | ggml_vulkan: Device lost | next");
    }

    #[test]
    fn the_home_folder_becomes_a_tilde() {
        assert_eq!(redact_with(r"no file C:\Users\ana\AppData\Roaming\Talaan\models", Some(r"C:\Users\ana")), r"no file ~\AppData\Roaming\Talaan\models");
        assert_eq!(redact_with("no file C:/Users/ana/x", Some(r"C:\Users\ana")), "no file ~/x");
        assert_eq!(redact_with("/Users/ana/Library/Logs/Talaan", Some("/Users/ana")), "~/Library/Logs/Talaan");
    }

    #[test]
    fn any_profile_folder_is_scrubbed() {
        // The exact home folder, in any case.
        assert_eq!(redact_with(r"no file c:\users\ana\x", Some(r"C:\Users\Ana")), r"no file ~\x");
        // Someone else's profile, 8.3 short names, forward slashes.
        assert_eq!(redact_with(r"open C:\Users\ANACRU~1\AppData\Local\Temp\x.wav failed", None), r"open ~\AppData\Local\Temp\x.wav failed");
        assert_eq!(redact_with(r"D:/users/Bob.Smith/Music", None), "~/Music");
        assert_eq!(redact_with("/Users/ana/Library/Logs and /home/bob/x", None), "~/Library/Logs and ~/x");
        // Redirected profiles on a network share.
        assert_eq!(redact_with(r"read \\fs01\profiles$\ana\Documents\x failed", None), r"read \\…\Documents\x failed");
        assert_eq!(redact_with(r"open \\nas\homes failed", None), r"open \\… failed");
        // Not profiles.
        assert_eq!(redact_with("GET /v1/users/me ok", None), "GET /v1/users/me ok");
        assert_eq!(redact_with(r"C:\Program Files\Talaan\talaan.exe", None), r"C:\Program Files\Talaan\talaan.exe");
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
        // Windows: the owner's name inside "Microphone (…)", the rest kept.
        assert_eq!(device("Micrófono (AirPods de Miguel)"), "Micrófono (AirPods de …)");
        assert_eq!(device("Microphone (AirPods von Ana)"), "Microphone (AirPods von …)");
        assert_eq!(device("Kopfhörer (Miguels AirPods)"), "Kopfhörer (… AirPods)");
        assert_eq!(device("Microphone (Ana’s AirPods)"), "Microphone (…’s AirPods)");
        assert_eq!(device("Miguels AirPods Pro"), "… AirPods Pro");
        assert_eq!(device("Headset (Plantronics Blackwire 3220)"), "Headset (Plantronics Blackwire 3220)");
        assert_eq!(device("Microphone Array (Intel® Smart Sound Technology)"), "Microphone Array (Intel® Smart Sound Technology)");
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
        assert!(lines[0].ends_with("WARN  commands::calendar: calendar download failed: https://[host]/… for [email]"), "{}", lines[0]);
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

}
