//! Calendar on Windows: the calendar's private iCal address (Google, Outlook, iCloud), downloaded every 15
//! minutes. The address comes from settings through `set_source` (this module never reads settings). The file is
//! read once per download; each `events` call only expands the window asked for. Windows' own calendar store
//! (AppointmentStore) doesn't see new Outlook or Google calendars: see docs/WINDOWS-PORT.md.

use crate::calendar::fetch;
use crate::calendar::ics::Calendar;
use crate::calendar::Event;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Allowed,
    NotAsked,
    #[allow(dead_code)] // macOS only; kept so both platforms share one contract
    Denied,
}

struct Source {
    url: Option<String>,
    me: Vec<String>,
    calendar: Option<Arc<Calendar>>,
    fetched_ms: Option<i64>,
    failed_ms: Option<i64>,
}

static SOURCE: Mutex<Source> = Mutex::new(Source { url: None, me: Vec::new(), calendar: None, fetched_ms: None, failed_ms: None });
/// Held during a download so two callers don't download the same calendar at once.
static DOWNLOADING: Mutex<()> = Mutex::new(());

fn source() -> std::sync::MutexGuard<'static, Source> {
    SOURCE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The calendar address from settings (empty or `None`: no calendar). A new address drops the old copy.
pub fn set_source(url: Option<String>) {
    let url = url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
    let mut s = source();
    if s.url != url {
        *s = Source { me: url.as_deref().map(fetch::me_from_url).unwrap_or_default(), url, calendar: None, fetched_ms: None, failed_ms: None };
    }
}

/// Allowed once an address is saved; there's no system permission on Windows.
pub fn access() -> Access {
    if source().url.is_some() { Access::Allowed } else { Access::NotAsked }
}

/// No system prompt: the address is asked for in Settings.
pub fn request() -> Access {
    access()
}

/// Events overlapping the window, downloading the calendar first when the copy is older than 15 minutes. A
/// failed download keeps the old copy. A download blocks the caller for at most about 15 s in all (`fetch::download`
/// has one overall deadline), so call this off the main thread.
pub fn events(from_ms: i64, to_ms: i64) -> Vec<Event> {
    refresh_if_due();
    let calendar = source().calendar.clone();
    calendar.map(|c| c.events(from_ms, to_ms)).unwrap_or_default()
}

fn refresh_if_due() {
    let due = |s: &Source, now: i64| s.url.is_some() && fetch::needs_refresh(s.fetched_ms, now) && fetch::may_retry(s.failed_ms, now);
    if !due(&source(), now_ms()) {
        return;
    }
    let _one_at_a_time = DOWNLOADING.lock().unwrap_or_else(|e| e.into_inner());
    let (url, me) = {
        let s = source();
        if !due(&s, now_ms()) {
            return; // another caller just downloaded it
        }
        (s.url.clone().unwrap_or_default(), s.me.clone())
    };
    let result = fetch::download(&url).map(|body| Calendar::read(&body, &me));
    let mut s = source();
    if s.url.as_deref() != Some(url.as_str()) {
        return; // the address changed meanwhile
    }
    match result {
        Ok(calendar) => {
            s.calendar = Some(Arc::new(calendar));
            s.fetched_ms = Some(now_ms());
            s.failed_ms = None;
        }
        Err(e) => {
            eprintln!("calendar: download failed: {e}");
            s.failed_ms = Some(now_ms());
        }
    }
}

/// Whether the last download attempt failed (the old copy, if any, is still in use).
pub fn last_download_failed() -> bool {
    source().failed_ms.is_some()
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Events in the next 7 days at an address, for the Test button. Doesn't touch the saved calendar.
pub fn test_address(url: &str) -> Result<usize, String> {
    let body = fetch::download(url)?;
    let now = now_ms();
    Ok(Calendar::read(&body, &fetch::me_from_url(url)).events(now, now + 7 * 86_400_000).len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real feed through the real path (network). Run with
    /// `cargo test public_holiday_feed_smoke -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn public_holiday_feed_smoke() {
        let url = "https://calendar.google.com/calendar/ical/en.canadian%23holiday%40group.v.calendar.google.com/public/basic.ics";
        let body = fetch::download(url).expect("download");
        let cal = Calendar::read(&body, &fetch::me_from_url(url));
        let now = now_ms();
        let year = cal.events(now - 183 * 86_400_000, now + 183 * 86_400_000);
        println!("{} bytes, {} events read, {} within six months either side, next 7 days: {:?}", body.len(), cal.len(), year.len(), test_address(url));
        for e in year.iter().take(5) {
            println!("  {} {} all_day={}", chrono::DateTime::from_timestamp_millis(e.start_ms).unwrap().format("%Y-%m-%d"), e.title, e.all_day);
        }
        assert!(cal.len() > 0 && year.iter().all(|e| e.all_day));
    }
}
