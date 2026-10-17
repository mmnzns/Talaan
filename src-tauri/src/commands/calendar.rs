//! Calendar link: access for Settings, and giving a recording its calendar event (title and attendees).

use super::{notify, AppState, MEETINGS_CHANGED};
use crate::calendar::{pick, EARLY_MS};
use crate::platform::calendar::{self as system, Access};
use crate::storage::model::CalendarEvent;
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
/// Called when a recording starts. Windows: if the calendar couldn't be downloaded then (offline), the meeting is
/// remembered so `link_after_stop` keeps trying.
pub fn link_soon(app: &AppHandle, meeting_id: &str) {
    if !enabled(app) {
        return;
    }
    let (app, id) = (app.clone(), meeting_id.to_string());
    std::thread::spawn(move || {
        let outcome = link(&app, &id);
        #[cfg(target_os = "windows")]
        if outcome == (Outcome::NotFound { offline: true }) {
            offline_at_start().push(id);
        }
        #[cfg(not(target_os = "windows"))]
        let _ = outcome;
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
    std::thread::spawn(move || {
        #[cfg(target_os = "windows")]
        let offline = {
            let mut list = offline_at_start();
            let was = list.contains(&id);
            list.retain(|m| *m != id);
            was
        };
        #[cfg(not(target_os = "windows"))]
        let offline = false;
        if link(&app, &id) == Outcome::Linked || !offline {
            return;
        }
        for wait in RELINK_WAITS {
            std::thread::sleep(wait);
            let wanted = app.state::<AppState>().lock().read_meeting_file(&id).is_ok_and(|f| wants_link(&f));
            if !wanted || !enabled(&app) || link(&app, &id) == Outcome::Linked {
                return;
            }
        }
    });
}

/// Waits between the tries after a stop: 5 minutes after it, then 30 (B3).
pub const RELINK_WAITS: [std::time::Duration; 2] = [std::time::Duration::from_secs(5 * 60), std::time::Duration::from_secs(25 * 60)];

/// Meetings whose start found no event because the calendar couldn't be downloaded.
#[cfg(target_os = "windows")]
fn offline_at_start() -> std::sync::MutexGuard<'static, Vec<String>> {
    static LIST: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    LIST.lock().unwrap_or_else(|e| e.into_inner())
}

/// A meeting still worth linking: no event yet, and the title is still the placeholder.
pub fn wants_link(f: &crate::storage::model::MeetingFile) -> bool {
    f.calendar_event.is_none() && f.title_is_placeholder
}

#[derive(Debug, PartialEq)]
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
    let (title, attendees) = (e.title.trim().to_string(), e.attendees.clone());
    let saved = storage.lock().update_meeting_file(id, |f| {
        f.calendar_event = Some(CalendarEvent { title: title.clone(), attendees: attendees.clone() });
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
        f.calendar_event = Some(CalendarEvent { title: "Standup".into(), attendees: vec![] });
        assert!(!wants_link(&f));
    }
}
