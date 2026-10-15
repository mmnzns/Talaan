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

/// Windows: hands the calendar address from settings to the platform (at startup and whenever settings change).
#[cfg(target_os = "windows")]
pub fn sync_source(app: &AppHandle) {
    let s = app.state::<AppState>().lock().settings_get().ok().flatten();
    let url = s.as_ref().and_then(|s| s["calendarIcsUrl"].as_str()).map(str::to_string);
    system::set_source(url);
}

/// Windows: downloads a calendar address and counts its events in the next 7 days, for Settings › Test.
#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn calendar_test_ics(url: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || system::test_address(&url)).await.map_err(|e| e.to_string())?
}

fn enabled(app: &AppHandle) -> bool {
    let s = app.state::<AppState>().lock().settings_get().ok().flatten();
    s.as_ref().and_then(|s| s["calendarConnected"].as_bool()).unwrap_or(false) && system::access() == Access::Allowed
}

/// Looks up the event for a meeting, in the background, and saves it on the meeting. The event title replaces
/// the placeholder title. Does nothing when the calendar is off, not allowed, or the meeting already has one.
pub fn link_soon(app: &AppHandle, meeting_id: &str) {
    if !enabled(app) {
        return;
    }
    let (app, id) = (app.clone(), meeting_id.to_string());
    std::thread::spawn(move || {
        let storage = app.state::<AppState>();
        let Ok(file) = storage.lock().read_meeting_file(&id) else { return };
        if file.calendar_event.is_some() {
            return;
        }
        let Ok(start) = chrono::DateTime::parse_from_rfc3339(&file.started_at) else { return };
        let at = start.timestamp_millis();
        let events = system::events(at - EARLY_MS, at + EARLY_MS);
        let Some(e) = pick(&events, at) else { return };
        let (title, attendees) = (e.title.trim().to_string(), e.attendees.clone());
        let saved = storage.lock().update_meeting_file(&id, |f| {
            f.calendar_event = Some(CalendarEvent { title: title.clone(), attendees: attendees.clone() });
            if f.title_is_placeholder {
                f.title = title.clone();
                f.title_is_placeholder = false;
            }
        });
        if saved.is_ok() {
            notify(&app, MEETINGS_CHANGED);
        }
    });
}

