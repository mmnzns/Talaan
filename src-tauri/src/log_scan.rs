//! Test-only: reads Talaan's source and checks every log call against the log file's privacy rules
//! (see `logging`). A call must be `log::<level>!(…)`; any value that isn't a plain id or number goes through
//! `logging::text(…)` or `.for_log()`; and nothing may name a field that can hold meeting content, people or
//! secrets, or print with `{:?}` (Debug output quotes whatever text it holds).

use std::fs;
use std::path::{Path, PathBuf};

/// Fields that can hold what was said, who was there, or a secret.
const FORBIDDEN: &[&str] = &[
    "title", "titles", "text", "texts", "recap", "summary", "decisions", "action_items", "open_questions", "prompt", "prompts", "attendee",
    "attendees", "email", "emails", "key", "api_key", "secret", "url", "address", "body", "note", "notes", "notice", "transcript", "lines",
    "your_name", "yourName", "workspace", "destination", "reply", "answer", "question", "passages", "events",
];

/// Names that usually hold an error or other free text: only through a sanitizer.
const GENERIC: &[&str] = &["e", "err", "error", "message", "msg", "m", "detail", "details", "reason", "why"];

const LEVELS: &[&str] = &["error", "warn", "info", "debug", "trace", "log"];

/// Which bytes of `src` are code (not inside a string, char literal or comment).
pub fn code_mask(src: &str) -> Vec<bool> {
    let b = src.as_bytes();
    let mut mask = vec![true; b.len()];
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut i = 0;
    while i < b.len() {
        let start = i;
        if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
        } else if b[i] == b'r' && (i == 0 || !ident(b[i - 1])) && matches!(b.get(i + 1), Some(b'"') | Some(b'#')) {
            // r"…" or r#"…"#
            let mut j = i + 1;
            let mut hashes = 0;
            while b.get(j) == Some(&b'#') {
                hashes += 1;
                j += 1;
            }
            if b.get(j) != Some(&b'"') {
                i += 1;
                continue;
            }
            j += 1;
            loop {
                if j >= b.len() {
                    break;
                }
                if b[j] == b'"' && b[j + 1..].iter().take(hashes).filter(|c| **c == b'#').count() == hashes {
                    j += 1 + hashes;
                    break;
                }
                j += 1;
            }
            i = j;
        } else if b[i] == b'"' {
            i += 1;
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
        } else if b[i] == b'\'' {
            // A char literal ('x', '\n', '\''), not a lifetime.
            if b.get(i + 1) == Some(&b'\\') {
                let mut j = i + 2;
                while j < b.len() && b[j] != b'\'' {
                    j += 1;
                }
                i = j + 1;
            } else {
                let width = src[i + 1..].chars().next().map(char::len_utf8).unwrap_or(1);
                if b.get(i + 1 + width) == Some(&b'\'') {
                    i += 2 + width;
                } else {
                    i += 1;
                    continue;
                }
            }
        } else {
            i += 1;
            continue;
        }
        let end = i.min(b.len());
        mask[start..end].iter_mut().for_each(|m| *m = false);
        i = end;
    }
    mask
}

/// One macro call found in the source.
#[derive(Debug)]
pub struct Call {
    pub line: usize,
    /// `log::warn` or, for a call that breaks the form rule, what it was (`warn`, `log::log`).
    pub name: String,
    pub bracket: char,
    /// The arguments: code with string literals blanked, and the literals' contents.
    pub code: String,
    pub strings: Vec<String>,
}

/// Every `[log::]level!` call in `src` (code only, not strings or comments).
pub fn calls(src: &str) -> Vec<Call> {
    let mask = code_mask(src);
    let b = src.as_bytes();
    let mut out = vec![];
    for level in LEVELS {
        let needle = format!("{level}!");
        for (i, _) in src.match_indices(&needle) {
            if !mask[i] || (i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')) {
                continue;
            }
            let prefixed = src[..i].ends_with("log::");
            if *level == "log" && !prefixed && i > 0 && b[i - 1] == b':' {
                continue; // some_other::log!
            }
            let name = if prefixed { format!("log::{level}") } else { level.to_string() };
            let mut j = i + needle.len();
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            let Some(&open) = b.get(j) else { continue };
            let close = match open {
                b'(' => b')',
                b'{' => b'}',
                b'[' => b']',
                _ => continue, // a macro definition or a name, not a call
            };
            let mut depth = 0;
            let mut end = b.len();
            for k in j..b.len() {
                if !mask[k] {
                    continue;
                }
                if b[k] == open {
                    depth += 1;
                } else if b[k] == close {
                    depth -= 1;
                    if depth == 0 {
                        end = k;
                        break;
                    }
                }
            }
            let body = &src[j + 1..end];
            let body_mask = &mask[j + 1..end];
            let mut code = String::new();
            let mut strings = vec![];
            let mut current = String::new();
            for (k, c) in body.char_indices() {
                if body_mask[k] {
                    if !current.is_empty() {
                        strings.push(std::mem::take(&mut current));
                    }
                    code.push(c);
                } else {
                    current.push(c);
                    code.push(' ');
                }
            }
            if !current.is_empty() {
                strings.push(current);
            }
            out.push(Call { line: src[..i].lines().count(), name, bracket: open as char, code, strings });
        }
    }
    out
}

/// `code` without the values that go through a sanitizer: `text(…)` spans and `x.for_log()`.
fn without_sanitized(code: &str) -> String {
    let mut out = String::new();
    let mut rest = code;
    while let Some(i) = rest.find("text(") {
        if i > 0 && rest[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
            out.push_str(&rest[..i + 5]);
            rest = &rest[i + 5..];
            continue;
        }
        out.push_str(&rest[..i]);
        let mut depth = 0;
        let mut end = rest.len();
        for (j, c) in rest[i..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i + j + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push_str(" sanitized ");
        rest = &rest[end..];
    }
    out.push_str(rest);
    while let Some(i) = out.find(".for_log()") {
        let start = out[..i].rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')).map(|s| s + 1).unwrap_or(0);
        out.replace_range(start..i + ".for_log()".len(), " sanitized ");
    }
    out
}

/// `{name}`, `{x.y:>6}` → `name`, `x`, `y`; the format spec after `:` is returned separately.
fn placeholders(s: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        if rest[i + 1..].starts_with('{') {
            rest = &rest[i + 2..];
            continue;
        }
        let Some(j) = rest[i..].find('}') else { break };
        let inner = &rest[i + 1..i + j];
        let (name, spec) = inner.split_once(':').unwrap_or((inner, ""));
        out.push((name.to_string(), spec.to_string()));
        rest = &rest[i + j + 1..];
    }
    out
}

fn words(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty())
}

/// What is wrong with a call, if anything.
pub fn problems(call: &Call) -> Vec<String> {
    let mut bad = vec![];
    if !call.name.starts_with("log::") || call.name == "log::log" {
        bad.push(format!("`{}!` (use log::error!/warn!/info!/debug!)", call.name));
    }
    if call.bracket != '(' {
        bad.push(format!("`{}!{}` (use parentheses)", call.name, call.bracket));
    }
    let mut ids: Vec<String> = words(&without_sanitized(&call.code)).map(str::to_string).collect();
    for s in &call.strings {
        for (name, spec) in placeholders(s) {
            if spec.contains('?') {
                bad.push(format!("`{{{name}:{spec}}}` (Debug output quotes the text it holds)"));
            }
            ids.extend(words(&name).map(str::to_string));
        }
    }
    for id in ids {
        if FORBIDDEN.contains(&id.as_str()) {
            bad.push(format!("names `{id}`"));
        } else if GENERIC.contains(&id.as_str()) {
            bad.push(format!("`{id}` without logging::text(…) or .for_log()"));
        }
    }
    bad
}

fn rust_files() -> Vec<PathBuf> {
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
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![];
    // The app, and the libraries it links that could log (the speech crate and the helpers' protocol).
    for dir in ["src", "stt/src", "llm-protocol/src"] {
        walk(&root.join(dir), &mut files);
    }
    files
}

/// The source up to its test module (`#[cfg(test)]` followed by `mod`).
fn app_code(src: &str) -> &str {
    &src[..src.find("#[cfg(test)]\nmod ").unwrap_or(src.len())]
}

#[test]
fn no_log_call_can_write_meeting_content_people_or_secrets() {
    let mut found = 0;
    let mut bad = vec![];
    for f in rust_files() {
        let src = fs::read_to_string(&f).unwrap().replace("\r\n", "\n");
        // Helper binaries' main.rs print to stderr on purpose (Talaan keeps only their error lines).
        for call in calls(app_code(&src)) {
            found += 1;
            for p in problems(&call) {
                bad.push(format!("{}:{}: {p}", f.display(), call.line));
            }
        }
    }
    assert!(found >= 40, "the scan found the log calls ({found})");
    assert!(bad.is_empty(), "log calls that break the log's privacy rules:\n{}", bad.join("\n"));
}

/// An installed app has no terminal: app code logs through `log::`, and only tests print. (The logger's echo for
/// `tauri dev`, and the helper binaries' main.rs, are the exceptions.)
#[test]
fn app_code_never_prints_to_stderr() {
    let mut bad = vec![];
    for f in rust_files() {
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        if name == "logging.rs" || name == "main.rs" {
            continue;
        }
        let src = fs::read_to_string(&f).unwrap().replace("\r\n", "\n");
        let code = app_code(&src);
        let mask = code_mask(code);
        for (i, _) in code.match_indices("eprintln!").chain(code.match_indices("println!")) {
            if mask[i] && !(i > 0 && code.as_bytes()[i - 1] == b'e' && code[i..].starts_with("println!")) {
                bad.push(format!("{}:{}", f.display(), code[..i].lines().count()));
            }
        }
    }
    assert!(bad.is_empty(), "printing in app code (use log::warn! / log::error!):\n{}", bad.join("\n"));
}

#[test]
fn the_scanner_reads_calls_the_way_rust_does() {
    let src = r##"
fn f() {
    // log::warn!("{title}") in a comment is not a call
    let s = "log::info!(\"{text}\")"; // nor in a string
    log::info!("ok {id} {n:>4}", n = 3);
    log::warn!("paren in a string ) and a char {}", ')' as u8);
    log::warn!("failed: {}", crate::logging::text(&e));
    log::warn!("failed: {}", e.for_log());
    log::warn!(r#"raw "{secret}""#);
    warn!("bare");
    log::log!(log::Level::Warn, "generic");
    log::warn!{"braces"};
    log::info!("{e}");
    log::info!("{}", message);
    log::info!("{x:?} {:#?}", y);
    log::info!("{m.title}");
}
"##;
    let calls = calls(src);
    let report: Vec<(String, Vec<String>)> = calls.iter().map(|c| (c.code.trim().to_string(), problems(c))).collect();
    let problems_of = |needle: &str| calls.iter().find(|c| c.strings.iter().any(|s| s.contains(needle)) || c.code.contains(needle)).map(problems).unwrap_or_else(|| panic!("{needle} not found in {report:?}"));
    assert_eq!(calls.len(), 12, "{report:?}");
    assert!(problems_of("ok ").is_empty());
    assert!(problems_of("paren in a string").is_empty());
    assert!(problems_of("crate::logging::text").is_empty());
    assert!(problems_of("for_log").is_empty());
    assert!(problems_of("raw ").iter().any(|p| p.contains("secret")));
    assert!(problems_of("bare").iter().any(|p| p.contains("`warn!`")));
    assert!(problems_of("generic").iter().any(|p| p.contains("log::log")));
    assert!(problems_of("braces").iter().any(|p| p.contains("parentheses")));
    assert!(problems_of("{e}").iter().any(|p| p.contains("`e` without")));
    assert!(problems_of("message").iter().any(|p| p.contains("`message` without")));
    let debug = problems_of("{x:?}");
    assert_eq!(debug.iter().filter(|p| p.contains("Debug")).count(), 2, "{debug:?}");
    assert!(problems_of("{m.title}").iter().any(|p| p.contains("`title`")));
}
