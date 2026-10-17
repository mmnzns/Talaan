//! Calendar link: access for Settings, and giving a recording its calendar event (title and attendees).

use super::{notify, AppState, MEETINGS_CHANGED};
use crate::calendar::{pick, EARLY_MS};
use crate::platform::calendar::{self as system, Access};
use crate::storage::model::CalendarEvent;
use serde::Serialize;
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
        log::warn!("couldn't remove the old calendar address from settings: {}", e.for_log());
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
    let n = connect(url).await.inspect_err(|e| log::warn!("calendar connect failed: {}", crate::logging::text(e)))?;
    log::info!("calendar connected ({n} entries in the next 7 days)");
    Ok(n)
}

#[cfg(target_os = "windows")]
async fn connect(url: String) -> Result<usize, String> {
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
    log::info!("calendar disconnected");
    Ok(())
}

fn enabled(app: &AppHandle) -> bool {
    let s = app.state::<AppState>().lock().settings_get().ok().flatten();
    s.as_ref().and_then(|s| s["calendarConnected"].as_bool()).unwrap_or(false) && system::access() == Access::Allowed
}

/// The meeting the menu bar offers to record (in progress, or starting within the hour).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextMeeting {
    title: String,
    start_ms: i64,
    end_ms: i64,
    attendees: Vec<String>,
    emails: Vec<String>,
    join_url: Option<String>,
    /// The call app the link opens ("zoom", "teams", or "browser" for Google Meet) and its name.
    app_id: Option<&'static str>,
    app_label: Option<&'static str>,
}

/// For the menu bar popover. `None` when the calendar is off or nothing is coming up. Async: on Windows it may
/// download the calendar first.
#[tauri::command]
pub async fn calendar_next(app: AppHandle) -> Option<NextMeeting> {
    if !enabled(&app) {
        return None;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let now = chrono::Utc::now().timestamp_millis();
        // Recordings already linked to an event (the last day is plenty), so their event isn't offered again.
        let recorded: Vec<(i64, String)> = {
            let state = app.state::<AppState>();
            let storage = state.lock();
            let recent = storage.list_meetings(&Default::default()).unwrap_or_default().into_iter().filter_map(|m| {
                let started = chrono::DateTime::parse_from_rfc3339(&m.started_at).ok()?.timestamp_millis();
                (now - started < 86_400_000).then_some((m.id, started))
            });
            recent.filter_map(|(id, started)| Some((started, storage.read_meeting_file(&id).ok()?.calendar_event?.title))).collect()
        };
        let recorded: Vec<_> = recorded.iter().map(|(started_ms, title)| crate::calendar::Recorded { started_ms: *started_ms, event_title: title }).collect();
        let events = system::events(now, now + crate::calendar::NEXT_MS);
        let e = crate::calendar::upcoming(&events, now, &recorded)?;
        let app = e.join_url.as_deref().and_then(crate::calendar::meeting_app);
        Some(NextMeeting {
            title: e.title.trim().to_string(),
            start_ms: e.start_ms,
            end_ms: e.end_ms,
            attendees: e.attendees.clone(),
            emails: e.emails.clone(),
            join_url: e.join_url.clone(),
            app_id: app.map(|a| a.0),
            app_label: app.map(|a| a.1),
        })
    })
    .await
    .ok()
    .flatten()
}

/// Recording started from the menu bar's next meeting: the event is known, so it's saved right away.
pub fn link_now(app: &AppHandle, meeting_id: &str, event: CalendarEvent) {
    let title = event.title.trim().to_string();
    let saved = app.state::<AppState>().lock().update_meeting_file(meeting_id, |f| {
        if f.title_is_placeholder && !title.is_empty() {
            f.title = title.clone();
            f.title_is_placeholder = false;
        }
        f.calendar_event = Some(CalendarEvent { title, ..event });
    });
    if saved.is_ok() {
        notify(app, MEETINGS_CHANGED);
    }
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
        // Windows: when the calendar download just failed (a flaky network at the start of a call), try once more
        // after the retry interval, on this same background thread.
        #[cfg(target_os = "windows")]
        let events = if pick(&events, at).is_none() && system::last_download_failed() {
            std::thread::sleep(std::time::Duration::from_millis(crate::calendar::fetch::RETRY_MS as u64));
            system::events(at - EARLY_MS, at + EARLY_MS)
        } else {
            events
        };
        let Some(e) = pick(&events, at) else {
            log::info!("calendar: no matching event for meeting {id}");
            return;
        };
        let (title, attendees, emails) = (e.title.trim().to_string(), e.attendees.clone(), e.emails.clone());
        let saved = storage.lock().update_meeting_file(&id, |f| {
            f.calendar_event = Some(CalendarEvent { title: title.clone(), attendees: attendees.clone(), emails: emails.clone() });
            if f.title_is_placeholder {
                f.title = title.clone();
                f.title_is_placeholder = false;
            }
        });
        if saved.is_ok() {
            log::info!("calendar: meeting {id} linked to its event");
            notify(&app, MEETINGS_CHANGED);
        }
    });
}

