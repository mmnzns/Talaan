//! Downloading a calendar from its private iCal address (Windows calendar). Talaan only downloads from the
//! address; nothing about meetings is sent. The pure helpers decide when to refetch and what the address says.

use std::io::Read;
use std::time::{Duration, Instant};

/// A downloaded calendar is fresh for this long.
pub const REFRESH_MS: i64 = 15 * 60_000;
/// After a failed download, wait this long before trying again (the old copy is kept meanwhile).
pub const RETRY_MS: i64 = 60_000;
/// Downloads larger than this are refused.
pub const MAX_BYTES: u64 = 20 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(15);

/// Whether a calendar downloaded at `last_fetch_ms` (never: `None`) should be downloaded again.
pub fn needs_refresh(last_fetch_ms: Option<i64>, now_ms: i64) -> bool {
    last_fetch_ms.is_none_or(|t| now_ms < t || now_ms - t >= REFRESH_MS)
}

/// Whether a download may be tried again after one failed at `last_failure_ms`.
pub fn may_retry(last_failure_ms: Option<i64>, now_ms: i64) -> bool {
    last_failure_ms.is_none_or(|t| now_ms < t || now_ms - t >= RETRY_MS)
}

/// The address as typed, cleaned up: `webcal://` becomes `https://`. Errors are written for the user.
pub fn normalize_url(url: &str) -> Result<String, String> {
    const BAD: &str = "That doesn't look like a calendar address. Copy the whole address, starting with https://.";
    let url = url.trim();
    let (scheme, rest) = url.split_once("://").ok_or(BAD)?;
    let scheme = match scheme.to_ascii_lowercase().as_str() {
        "https" | "webcal" | "webcals" => "https",
        "http" => "http",
        _ => return Err(BAD.into()),
    };
    if rest.is_empty() || rest.starts_with('/') || rest.chars().any(char::is_whitespace) {
        return Err(BAD.into());
    }
    Ok(format!("{scheme}://{rest}"))
}

/// The user's own address, when the calendar address says it: Google's are
/// `https://calendar.google.com/calendar/ical/<address, percent-encoded>/private-…/basic.ics`.
pub fn me_from_url(url: &str) -> Vec<String> {
    let Some(rest) = url.trim().split_once("://").map(|(_, r)| r) else { return vec![] };
    let Some(path) = rest.strip_prefix("calendar.google.com/calendar/ical/") else { return vec![] };
    let id = percent_decode(path.split('/').next().unwrap_or_default());
    // Group calendars (holidays, shared calendars) aren't a person.
    match id {
        Some(id) if id.contains('@') && !id.ends_with("calendar.google.com") => vec![id],
        _ => vec![],
    }
}

/// `%40` → `@`; `None` when the result isn't UTF-8.
fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match (b[i], b.get(i + 1).copied().and_then(hex), b.get(i + 2).copied().and_then(hex)) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (c, _, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// Whether a download is an iCal file and not, say, a sign-in page.
pub fn looks_like_calendar(body: &[u8]) -> bool {
    let body = body.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(body);
    let start = body.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(body.len());
    body[start..].get(..15).is_some_and(|head| head.eq_ignore_ascii_case(b"BEGIN:VCALENDAR"))
}

const TOO_SLOW: &str = "The calendar didn't finish downloading within 15 seconds. Check your connection and try again.";

/// Downloads the calendar: 15 s in all (connecting, waiting and reading together), at most 20 MB. Errors are
/// written for the user. The work runs on its own thread so a stalled read can't hold the caller past the deadline.
pub fn download(url: &str) -> Result<Vec<u8>, String> {
    let url = normalize_url(url)?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("calendar-download".into())
        .spawn(move || {
            let _ = tx.send(download_until(&url, Instant::now() + TIMEOUT));
        })
        .map_err(|_| TOO_SLOW.to_string())?;
    rx.recv_timeout(TIMEOUT + Duration::from_millis(500)).unwrap_or_else(|_| Err(TOO_SLOW.to_string()))
}

fn download_until(url: &str, deadline: Instant) -> Result<Vec<u8>, String> {
    // Each network step gets at most the full timeout; the deadline also bounds the body as a whole.
    let client = reqwest::blocking::Client::builder().timeout(TIMEOUT).build().map_err(|e| e.to_string())?;
    let response = client.get(url).send().map_err(|e| {
        if e.is_timeout() {
            TOO_SLOW.to_string()
        } else {
            "Couldn't reach the calendar address. Check your connection and the address.".to_string()
        }
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("The calendar address didn't work (HTTP {}). Copy it again from your calendar.", status.as_u16()));
    }
    let body = read_body(response, MAX_BYTES, || Instant::now() >= deadline)?;
    if !looks_like_calendar(&body) {
        return Err("That address didn't return a calendar. Copy the address in iCal format (it ends in .ics).".to_string());
    }
    Ok(body)
}

/// Reads a body in chunks, refusing more than `max` bytes and stopping once `out_of_time` says so.
fn read_body(mut r: impl Read, max: u64, out_of_time: impl Fn() -> bool) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        if out_of_time() {
            return Err(TOO_SLOW.to_string());
        }
        let n = r.read(&mut chunk).map_err(|e| if e.kind() == std::io::ErrorKind::TimedOut { TOO_SLOW.to_string() } else { "The download was interrupted. Try again.".to_string() })?;
        if n == 0 {
            return Ok(body);
        }
        body.extend_from_slice(&chunk[..n]);
        if body.len() as u64 > max {
            return Err("This calendar is larger than 20 MB, which Talaan doesn't read.".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;

    #[test]
    fn a_calendar_is_refetched_after_fifteen_minutes() {
        let now = 1_000 * MIN;
        assert!(needs_refresh(None, now));
        assert!(!needs_refresh(Some(now - 14 * MIN), now));
        assert!(needs_refresh(Some(now - 15 * MIN), now));
        // A clock that went backwards refetches rather than waiting.
        assert!(needs_refresh(Some(now + 5 * MIN), now));
    }

    #[test]
    fn a_failed_download_is_retried_after_a_minute() {
        let now = 1_000 * MIN;
        assert!(may_retry(None, now));
        assert!(!may_retry(Some(now - 30_000), now));
        assert!(may_retry(Some(now - MIN), now));
        assert!(may_retry(Some(now + MIN), now));
    }

    #[test]
    fn addresses_are_cleaned_up_or_refused_plainly() {
        assert_eq!(normalize_url("  https://example.com/cal.ics \n").unwrap(), "https://example.com/cal.ics");
        assert_eq!(normalize_url("webcal://p01-caldav.icloud.com/published/2/abc").unwrap(), "https://p01-caldav.icloud.com/published/2/abc");
        assert_eq!(normalize_url("WEBCALS://example.com/x.ics").unwrap(), "https://example.com/x.ics");
        assert_eq!(normalize_url("http://example.com/x.ics").unwrap(), "http://example.com/x.ics");
        for bad in ["", "   ", "example.com/cal.ics", "ftp://example.com/x.ics", "https://", "file:///C:/cal.ics"] {
            let e = normalize_url(bad).unwrap_err();
            assert!(e.contains("address"), "{bad:?}: {e}");
        }
    }

    #[test]
    fn google_addresses_name_the_user() {
        let url = "https://calendar.google.com/calendar/ical/miguel.monzones%40example.com/private-0123abcd/basic.ics";
        assert_eq!(me_from_url(url), vec!["miguel.monzones@example.com"]);
        assert_eq!(me_from_url("https://calendar.google.com/calendar/ical/a%2Bb%40example.com/public/basic.ics"), vec!["a+b@example.com"]);
        // Group calendars (holidays, shared) and other providers name nobody.
        assert!(me_from_url("https://calendar.google.com/calendar/ical/en.canadian%23holiday%40group.v.calendar.google.com/public/basic.ics").is_empty());
        assert!(me_from_url("https://outlook.office365.com/owa/calendar/abc@example.com/def/calendar.ics").is_empty());
        assert!(me_from_url("https://calendar.google.com/calendar/ical/%E0%40x/basic.ics").is_empty());
        assert!(me_from_url("not a url").is_empty());
    }

    /// Hands out `chunk` bytes per read, `total` in all.
    struct Chunks {
        left: usize,
        chunk: usize,
        reads: std::rc::Rc<std::cell::Cell<usize>>,
    }
    impl Read for Chunks {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.reads.set(self.reads.get() + 1);
            let n = self.left.min(self.chunk).min(buf.len());
            buf[..n].fill(b'x');
            self.left -= n;
            Ok(n)
        }
    }

    #[test]
    fn the_body_is_read_against_one_deadline_and_a_size_cap() {
        let reads = std::rc::Rc::new(std::cell::Cell::new(0));
        let body = |left, chunk| Chunks { left, chunk, reads: reads.clone() };
        assert_eq!(read_body(body(1000, 100), 5000, || false).unwrap().len(), 1000);
        assert!(read_body(body(6000, 100), 5000, || false).unwrap_err().contains("20 MB"));
        // The deadline is checked between reads: a slow trickle stops instead of running on.
        reads.set(0);
        let ticks = std::cell::Cell::new(0);
        let err = read_body(body(1_000_000, 10), 5_000_000, || {
            ticks.set(ticks.get() + 1);
            ticks.get() > 3
        })
        .unwrap_err();
        assert!(err.contains("15 seconds"), "{err}");
        assert!(reads.get() <= 4, "{}", reads.get());
    }

    #[test]
    fn only_ical_downloads_count_as_a_calendar() {
        assert!(looks_like_calendar(b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nEND:VCALENDAR\r\n"));
        assert!(looks_like_calendar(b"\xEF\xBB\xBF\r\n  begin:vcalendar\nEND:VCALENDAR"));
        assert!(!looks_like_calendar(b"<!doctype html><title>Sign in</title>"));
        assert!(!looks_like_calendar(b""));
    }
}
