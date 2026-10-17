//! Calendar link: access for Settings, and giving a recording its calendar event (title and attendees).

use super::{notify, AppState, MEETINGS_CHANGED};
use crate::calendar::{pick, EARLY_MS};
use crate::platform::calendar::{self as system, Access};
use crate::storage::model::CalendarEvent;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

fn name(a: Access) -> &'static str {
    match a {
        Access::Allowed => "allowed",
        Access::NotAsked => "not_asked",
        Access::Denied => "denied",
    }
}

#[tauri::command]
pub fn calendar_access() -> &'static str {
    name(system::access())
}

/// Asks the system the first time; answers "allowed" | "denied".
#[tauri::command]
pub async fn calendar_request() -> &'static str {
    tauri::async_runtime::spawn_blocking(|| name(system::request())).await.unwrap_or("denied")
}

/// Startup, before `sync_source`: deletes the plain-text calendar address that Windows test builds kept in
/// settings.json. Windows moves it into secure key storage when no address is saved there yet (the calendar stays
/// connected), and turns the calendar off if that fails. Mac builds never wrote it: it's just dropped.
pub fn remove_legacy_address(store: &mut crate::storage::Storage) {
    #[cfg(target_os = "windows")]
    let keep = |address: &str| {
        crate::keys::get(crate::keys::CALENDAR_ICS).is_some()
            || crate::calendar::fetch::normalize_url(address).is_ok_and(|url| crate::keys::set(crate::keys::CALENDAR_ICS, &url).is_ok())
    };
    #[cfg(not(target_os = "windows"))]
    let keep = |_: &str| true;
    if let Err(e) = store.remove_legacy_calendar_address(keep) {
        eprintln!("talaan: couldn't remove the old calendar address from settings: {e}");
    }
}

/// Windows: hands the saved calendar address (secure key storage) to the platform, at startup and whenever
/// settings change.
#[cfg(target_os = "windows")]
pub fn sync_source(_app: &AppHandle) {
    system::set_source(crate::keys::get(crate::keys::CALENDAR_ICS));
}

/// Windows: downloads a calendar address and, if it works, saves it in secure key storage (never in settings) and
/// starts using it. Returns the number of events in the next 7 days. The frontend then sets `calendarConnected`.
#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn calendar_connect(url: String) -> Result<usize, String> {
    let url = crate::calendar::fetch::normalize_url(&url)?;
    let checked = url.clone();
    let n = tauri::async_runtime::spawn_blocking(move || system::test_address(&checked)).await.map_err(|e| e.to_string())??;
    crate::keys::set(crate::keys::CALENDAR_ICS, &url)?;
    system::set_source(Some(url));
    Ok(n)
}

/// Windows: forgets the calendar address. The frontend then clears `calendarConnected`.
#[cfg(target_os = "windows")]
#[tauri::command]
pub fn calendar_disconnect() -> Result<(), String> {
    crate::keys::clear(crate::keys::CALENDAR_ICS)?;
    system::set_source(None);
    Ok(())
}

fn enabled(app: &AppHandle) -> bool {
    let s = app.state::<AppState>().lock().settings_get().ok().flatten();
    s.as_ref().and_then(|s| s["calendarConnected"].as_bool()).unwrap_or(false) && system::access() == Access::Allowed
}

/// Looks up the event for a meeting, in the background, and saves it on the meeting. The event title replaces
/// the placeholder title. Does nothing when the calendar is off, not allowed, or the meeting already has one.
/// Called when a recording starts. If the recording stops before this lookup is done (it can take a minute when
/// the download fails), this thread does the after-stop tries itself (`Linking`).
pub fn link_soon(app: &AppHandle, meeting_id: &str) {
    if !enabled(app) {
        return;
    }
    let (app, id) = (app.clone(), meeting_id.to_string());
    linking().start(&id, Instant::now());
    std::thread::spawn(move || {
        let outcome = link(&app, &id);
        let offline = outcome == (Outcome::NotFound { offline: true });
        if let AfterStart::Stopped = linking().start_done(&id, offline, Instant::now()) {
            if outcome != Outcome::Linked {
                after_stop(&app, &id, offline);
            }
        }
    });
}

/// When a recording stops: a call started before its event was on the calendar (or a few minutes early) gets a
/// second chance. Windows, when the download had failed at the start (B3): tries again now, then 5 and 30 minutes
/// after the stop, while the meeting has no event and still has its placeholder title (a summary title ends it).
pub fn link_after_stop(app: &AppHandle, meeting_id: &str) {
    if !enabled(app) {
        return;
    }
    let (app, id) = (app.clone(), meeting_id.to_string());
    let offline = match linking().stop(&id, Instant::now()) {
        AtStop::StartStillRunning => return, // its thread carries on (see link_soon)
        AtStop::Tried { offline } => offline,
    };
    std::thread::spawn(move || after_stop(&app, &id, offline));
}

/// One try now; after an offline start (Windows only: the Mac reads its calendar locally), more later.
fn after_stop(app: &AppHandle, id: &str, offline_at_start: bool) {
    if link(app, id) == Outcome::Linked || !(cfg!(target_os = "windows") && offline_at_start) {
        return;
    }
    for wait in RELINK_WAITS {
        std::thread::sleep(wait);
        let wanted = app.state::<AppState>().lock().read_meeting_file(id).is_ok_and(|f| wants_link(&f));
        if !wanted || !enabled(app) || link(app, id) == Outcome::Linked {
            return;
        }
    }
}

/// Waits between the tries after a stop: 5 minutes after it, then 30 (B3).
pub const RELINK_WAITS: [std::time::Duration; 2] = [std::time::Duration::from_secs(5 * 60), std::time::Duration::from_secs(25 * 60)];

/// Where each recording's start lookup is, so Stop knows whether it found the calendar offline, or is still
/// running. Entries go at Stop (or when the start lookup ends after it), and any older than a day are dropped
/// (a recording discarded instead of stopped).
#[derive(Default)]
struct Linking {
    meetings: std::collections::HashMap<String, (Track, Instant)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Track {
    StartRunning,
    StopWaiting,
    Offline,
}

#[derive(Debug, PartialEq)]
enum AfterStart {
    /// Still recording, or nothing to remember.
    Recording,
    /// The recording stopped meanwhile: the start thread does the after-stop tries.
    Stopped,
}

#[derive(Debug, PartialEq)]
enum AtStop {
    StartStillRunning,
    Tried { offline: bool },
}

const FORGET_AFTER: Duration = Duration::from_secs(24 * 3600);

impl Linking {
    fn prune(&mut self, now: Instant) {
        self.meetings.retain(|_, (_, at)| now.duration_since(*at) < FORGET_AFTER);
    }

    fn start(&mut self, id: &str, now: Instant) {
        self.prune(now);
        self.meetings.insert(id.to_string(), (Track::StartRunning, now));
    }

    fn start_done(&mut self, id: &str, offline: bool, now: Instant) -> AfterStart {
        match self.meetings.get(id).map(|(t, _)| *t) {
            Some(Track::StopWaiting) => {
                self.meetings.remove(id);
                AfterStart::Stopped
            }
            Some(_) if offline => {
                self.meetings.insert(id.to_string(), (Track::Offline, now));
                AfterStart::Recording
            }
            _ => {
                self.meetings.remove(id);
                AfterStart::Recording
            }
        }
    }

    fn stop(&mut self, id: &str, now: Instant) -> AtStop {
        self.prune(now);
        match self.meetings.get(id).map(|(t, _)| *t) {
            Some(Track::StartRunning) => {
                self.meetings.insert(id.to_string(), (Track::StopWaiting, now));
                AtStop::StartStillRunning
            }
            Some(Track::Offline) => {
                self.meetings.remove(id);
                AtStop::Tried { offline: true }
            }
            _ => AtStop::Tried { offline: false },
        }
    }
}

fn linking() -> std::sync::MutexGuard<'static, Linking> {
    static STATE: std::sync::LazyLock<std::sync::Mutex<Linking>> = std::sync::LazyLock::new(Default::default);
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// A meeting still worth linking: no event yet, and the title is still the placeholder.
pub fn wants_link(f: &crate::storage::model::MeetingFile) -> bool {
    f.calendar_event.is_none() && f.title_is_placeholder
}

#[derive(Debug, PartialEq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))] // `offline` is only read on Windows
enum Outcome {
    Linked,
    /// Nothing to do: already linked, or the meeting can't be read.
    Skipped,
    /// No event at that time; `offline`: because the calendar couldn't be downloaded (Windows).
    NotFound { offline: bool },
}

/// One try: finds the event at the meeting's start and saves it (blocking; call off the main thread).
fn link(app: &AppHandle, id: &str) -> Outcome {
    let storage = app.state::<AppState>();
    let Ok(file) = storage.lock().read_meeting_file(id) else { return Outcome::Skipped };
    if file.calendar_event.is_some() {
        return Outcome::Skipped;
    }
    let Ok(start) = chrono::DateTime::parse_from_rfc3339(&file.started_at) else { return Outcome::Skipped };
    let at = start.timestamp_millis();
    let events = system::events(at - EARLY_MS, at + EARLY_MS);
    // Windows: when the calendar download just failed (a flaky network at the start of a call), try once more
    // after the retry interval, on this same background thread.
    #[cfg(target_os = "windows")]
    let events = if pick(&events, at).is_none() && system::last_download_failed() {
        std::thread::sleep(std::time::Duration::from_millis(crate::calendar::fetch::RETRY_MS as u64));
        system::events(at - EARLY_MS, at + EARLY_MS)
    } else {
        events
    };
    #[cfg(target_os = "windows")]
    let offline = system::last_download_failed();
    #[cfg(not(target_os = "windows"))]
    let offline = false;
    let Some(e) = pick(&events, at) else { return Outcome::NotFound { offline } };
    let (title, attendees, emails) = (e.title.trim().to_string(), e.attendees.clone(), e.emails.clone());
    let saved = storage.lock().update_meeting_file(id, |f| {
        f.calendar_event = Some(CalendarEvent { title: title.clone(), attendees: attendees.clone(), emails: emails.clone() });
        if f.title_is_placeholder {
            f.title = title.clone();
            f.title_is_placeholder = false;
        }
    });
    if saved.is_ok() {
        notify(app, MEETINGS_CHANGED);
        Outcome::Linked
    } else {
        Outcome::Skipped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_lookup_still_running_at_stop_finishes_the_job() {
        let t0 = Instant::now();
        let mut l = Linking::default();
        l.start("m", t0);
        assert_eq!(l.stop("m", t0), AtStop::StartStillRunning);
        assert_eq!(l.start_done("m", true, t0), AfterStart::Stopped, "the start thread does the after-stop tries");
        assert!(l.meetings.is_empty());
    }

    #[test]
    fn an_offline_start_is_remembered_until_stop() {
        let t0 = Instant::now();
        let mut l = Linking::default();
        l.start("m", t0);
        assert_eq!(l.start_done("m", true, t0), AfterStart::Recording);
        assert_eq!(l.stop("m", t0), AtStop::Tried { offline: true });
        assert!(l.meetings.is_empty());
        // Online start: nothing kept.
        l.start("n", t0);
        l.start_done("n", false, t0);
        assert!(l.meetings.is_empty());
        assert_eq!(l.stop("n", t0), AtStop::Tried { offline: false });
    }

    #[test]
    fn meetings_never_stopped_are_forgotten_after_a_day() {
        let t0 = Instant::now();
        let mut l = Linking::default();
        l.start("discarded", t0);
        l.start_done("discarded", true, t0);
        l.start("next", t0 + FORGET_AFTER);
        assert!(!l.meetings.contains_key("discarded"));
    }

    #[test]
    fn retries_come_5_and_30_minutes_after_the_stop() {
        let mut at = std::time::Duration::ZERO;
        let times: Vec<u64> = RELINK_WAITS.iter().map(|w| { at += *w; at.as_secs() / 60 }).collect();
        assert_eq!(times, vec![5, 30]);
    }

    #[test]
    fn only_a_placeholder_meeting_without_an_event_is_linked() {
        let json = r#"{"schemaVersion":1,"id":"x","title":"Call, Zoom, Oct 14, 2:30 PM","app":"zoom","appLabel":"Zoom",
            "startedAt":"2026-10-14T14:30:00-07:00","language":"en","status":"transcribing"}"#;
        let mut f: crate::storage::model::MeetingFile = serde_json::from_str(json).unwrap();
        f.calendar_event = None;
        f.title_is_placeholder = true;
        assert!(wants_link(&f));
        f.title_is_placeholder = false; // the summary (or the user) gave it a title
        assert!(!wants_link(&f));
        f.title_is_placeholder = true;
        f.calendar_event = Some(CalendarEvent { title: "Standup".into(), attendees: vec![], emails: vec![] });
        assert!(!wants_link(&f));
    }
}
