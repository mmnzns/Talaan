//! Calendar link: access for Settings, and giving a recording its calendar event (title and attendees).

use super::{notify, AppState, MEETINGS_CHANGED};
use crate::calendar::{pick, EARLY_MS};
use crate::platform::calendar::{self as system, Access};
use crate::storage::model::CalendarEvent;
use std::time::{Duration, Instant};
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
        unreadable();
        return None;
    }
    tauri::async_runtime::spawn_blocking(move || {
        relink_if_recovered(&app); // starts a background sweep at most; never waits for it
        let now = chrono::Utc::now().timestamp_millis();
        // Recordings already linked to an event (the last day is plenty), so their event isn't offered again.
        let recorded = recorded_events(&app.state::<AppState>().lock(), now);
        let recorded = as_recorded(&recorded);
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
/// Called when a recording starts. If the recording stops before this lookup is done (it can take a minute when
/// the download fails), this thread does the after-stop tries itself (`Linking`).
pub fn link_soon(app: &AppHandle, meeting_id: &str) {
    if !enabled(app) {
        unreadable(); // linked later, when the calendar can be read again
        return;
    }
    let (app, id) = (app.clone(), meeting_id.to_string());
    // Tracked first, so a sweep started below leaves this meeting to its own thread.
    linking().start(&id, Instant::now());
    relink_if_recovered(&app);
    std::thread::spawn(move || {
        let outcome = link(&app, &id, false);
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
        unreadable();
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
    if link(app, id, false) == Outcome::Linked || !(cfg!(target_os = "windows") && offline_at_start) {
        return;
    }
    for wait in RELINK_WAITS {
        std::thread::sleep(wait);
        let wanted = app.state::<AppState>().lock().read_meeting_file(id).is_ok_and(|f| wants_link(&f));
        if !wanted || !enabled(app) || link(app, id, false) == Outcome::Linked {
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

    /// Whether a start lookup (or its after-stop tries) is looking after this meeting.
    fn is_tracked(&self, id: &str) -> bool {
        self.meetings.contains_key(id)
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

/// A meeting still worth a title from its event: no event yet, and the title is still the placeholder (the user
/// hasn't renamed it and no summary title replaced it). The after-stop retries end when this stops being true.
pub fn wants_link(f: &crate::storage::model::MeetingFile) -> bool {
    f.calendar_event.is_none() && f.title_is_placeholder
}

/// How far back a meeting is still re-linked when the calendar becomes readable again.
const RELINK_WINDOW_MS: i64 = 24 * 3600 * 1000;

/// Whether a meeting is worth a late re-link at `now_ms`: it has no event and started within the last day.
pub fn relink_due(f: &crate::storage::model::MeetingFile, now_ms: i64) -> bool {
    let started = chrono::DateTime::parse_from_rfc3339(&f.started_at).map(|t| t.timestamp_millis()).ok();
    f.calendar_event.is_none() && started.is_some_and(|t| t <= now_ms && now_ms - t < RELINK_WINDOW_MS)
}

/// Gives a meeting its calendar event: the event and its attendees whenever it has none, and the title only while
/// it is still the placeholder (a rename or a summary title stays). Named speakers are never touched. `recorded`
/// is the recordings already linked to events: when one covers this event nothing is linked (pass `&[]` to skip
/// that check). False when nothing changed.
pub fn apply_event(f: &mut crate::storage::model::MeetingFile, e: &crate::calendar::Event, recorded: &[crate::calendar::Recorded]) -> bool {
    if f.calendar_event.is_some() || crate::calendar::already_recorded(e, recorded) {
        return false;
    }
    let title = e.title.trim().to_string();
    f.calendar_event = Some(CalendarEvent { title: title.clone(), attendees: e.attendees.clone(), emails: e.emails.clone() });
    if f.title_is_placeholder {
        f.title = title;
        f.title_is_placeholder = false;
    }
    true
}

/// Recordings of the last day that are linked to an event: (start ms, event title).
fn recorded_events(storage: &crate::storage::Storage, now: i64) -> Vec<(i64, String)> {
    let recent = storage.list_meetings(&Default::default()).unwrap_or_default().into_iter().filter_map(|m| {
        let started = chrono::DateTime::parse_from_rfc3339(&m.started_at).ok()?.timestamp_millis();
        (now - started < 86_400_000).then_some((m.id, started))
    });
    recent.filter_map(|(id, started)| Some((started, storage.read_meeting_file(&id).ok()?.calendar_event?.title))).collect()
}

fn as_recorded(list: &[(i64, String)]) -> Vec<crate::calendar::Recorded<'_>> {
    list.iter().map(|(started_ms, title)| crate::calendar::Recorded { started_ms: *started_ms, event_title: title }).collect()
}

/// Whether the calendar was readable the last time we looked (Mac: access allowed; Windows: address saved).
static WAS_READABLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn unreadable() {
    WAS_READABLE.store(false, std::sync::atomic::Ordering::SeqCst);
}

/// True when the calendar has just become readable: access came back (Mac: the platform saw it off and then on,
/// even between our own checks), or (Windows) a download succeeded after a failure or for the first time.
fn became_readable(app: &AppHandle) -> bool {
    let now = enabled(app);
    let was = WAS_READABLE.swap(now, std::sync::atomic::Ordering::SeqCst);
    let recovered = system::take_recovered();
    now && (!was || recovered)
}

/// At most one sweep at a time. A request that arrives while one runs makes it go round once more.
#[derive(Default)]
struct SweepGate {
    running: bool,
    again: bool,
}

impl SweepGate {
    /// True when the caller should run a sweep now; false when one is running (it will repeat).
    fn request(&mut self) -> bool {
        if self.running {
            self.again = true;
            false
        } else {
            self.running = true;
            true
        }
    }

    /// A sweep ended: true when another request came in meanwhile (run again); otherwise the gate opens.
    fn done(&mut self) -> bool {
        if std::mem::take(&mut self.again) {
            true
        } else {
            self.running = false;
            false
        }
    }
}

fn sweep_gate() -> std::sync::MutexGuard<'static, SweepGate> {
    static GATE: std::sync::LazyLock<std::sync::Mutex<SweepGate>> = std::sync::LazyLock::new(Default::default);
    GATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The calendar is readable again: links the meetings of the last day that started while it wasn't (blocking; only
/// the sweep thread calls it). Meetings whose own start lookup is still running are left to it, and an event that
/// another recording already has isn't given twice.
fn sweep(app: &AppHandle) {
    let now = chrono::Utc::now().timestamp_millis();
    // Windows: the first download happens here, so the "recovered" signal it sets is this sweep's own: dropped.
    system::events(now, now);
    system::take_recovered();
    let ids: Vec<String> = {
        let state = app.state::<AppState>();
        let storage = state.lock();
        let all = storage.list_meetings(&Default::default()).unwrap_or_default();
        all.into_iter().filter(|m| storage.read_meeting_file(&m.id).is_ok_and(|f| relink_due(&f, now))).map(|m| m.id).collect()
    };
    for id in ids {
        if !linking().is_tracked(&id) {
            link(app, &id, true);
        }
    }
}

/// When the calendar has become readable again, links the meetings that missed their event, on a background thread
/// (a Windows download can take a while). Never blocks the caller; cheap when nothing changed.
fn relink_if_recovered(app: &AppHandle) {
    if !became_readable(app) || !sweep_gate().request() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || loop {
        sweep(&app);
        if !sweep_gate().done() {
            break;
        }
    });
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

/// One try: finds the event at the meeting's start and saves it (blocking; call off the main thread). `dedupe`
/// (the sweep): leaves an event that another recording already has.
fn link(app: &AppHandle, id: &str, dedupe: bool) -> Outcome {
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
    let Some(e) = pick(&events, at) else {
        log::info!("calendar: no matching event for meeting {id} (offline: {offline})");
        return Outcome::NotFound { offline };
    };
    let taken = if dedupe { recorded_events(&storage.lock(), chrono::Utc::now().timestamp_millis()) } else { vec![] };
    let linked = std::cell::Cell::new(false);
    // Checked again inside the update, so an edit or a link made meanwhile is never overwritten.
    let saved = storage.lock().update_meeting_file(id, |f| linked.set(apply_event(f, e, &as_recorded(&taken))));
    if saved.is_ok() && linked.get() {
        log::info!("calendar: meeting {id} linked to its event");
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

    fn file(title_is_placeholder: bool) -> crate::storage::model::MeetingFile {
        let json = r#"{"schemaVersion":1,"id":"x","title":"Call, Zoom, Oct 14, 2:30 PM","app":"zoom","appLabel":"Zoom",
            "startedAt":"2026-10-14T14:30:00+00:00","language":"en","status":"ready"}"#;
        let mut f: crate::storage::model::MeetingFile = serde_json::from_str(json).unwrap();
        f.title_is_placeholder = title_is_placeholder;
        f
    }

    fn event() -> crate::calendar::Event {
        crate::calendar::Event {
            title: " Roadmap sync ".into(),
            start_ms: 0,
            end_ms: 1,
            all_day: false,
            attendees: vec!["Ana Reyes".into()],
            emails: vec!["ana@example.com".into()],
            join_url: None,
            cancelled: false,
            declined: false,
        }
    }

    #[test]
    fn a_late_link_fills_title_and_attendees() {
        let mut f = file(true);
        assert!(apply_event(&mut f, &event(), &[]));
        assert_eq!((f.title.as_str(), f.title_is_placeholder), ("Roadmap sync", false));
        let e = f.calendar_event.unwrap();
        assert_eq!((e.attendees, e.emails), (vec!["Ana Reyes".to_string()], vec!["ana@example.com".to_string()]));
    }

    #[test]
    fn a_renamed_meeting_still_gets_its_event_but_keeps_its_title() {
        let mut f = file(false); // the user (or the summary) already set the title
        f.title = "My own name".into();
        assert!(apply_event(&mut f, &event(), &[]));
        assert_eq!(f.title, "My own name");
        let e = f.calendar_event.unwrap();
        assert_eq!((e.title.as_str(), e.attendees), ("Roadmap sync", vec!["Ana Reyes".to_string()]));
    }

    #[test]
    fn named_speakers_get_the_event_and_stay_untouched() {
        let mut f = file(true);
        f.speakers.insert("them:0".into(), "Dana".into());
        assert!(apply_event(&mut f, &event(), &[]));
        assert!(f.calendar_event.is_some());
        assert_eq!(f.speakers.get("them:0").map(String::as_str), Some("Dana"));
    }

    #[test]
    fn a_meeting_with_an_event_is_left_alone() {
        let mut f = file(true);
        f.calendar_event = Some(CalendarEvent { title: "Other".into(), attendees: vec![], emails: vec![] });
        assert!(!apply_event(&mut f, &event(), &[]));
        assert_eq!((f.title.as_str(), f.title_is_placeholder), ("Call, Zoom, Oct 14, 2:30 PM", true));
    }

    #[test]
    fn an_event_another_recording_has_is_not_linked_twice() {
        let ev = event(); // starts at 0
        let taken = [crate::calendar::Recorded { started_ms: 0, event_title: "Roadmap sync" }];
        let mut f = file(true);
        assert!(!apply_event(&mut f, &ev, &taken));
        assert!(f.calendar_event.is_none() && f.title_is_placeholder);
        let elsewhere = [crate::calendar::Recorded { started_ms: 10 * 3600_000, event_title: "Roadmap sync" }];
        assert!(apply_event(&mut f, &ev, &elsewhere), "a recording of another day doesn't count");
    }

    #[test]
    fn at_most_one_sweep_runs_and_a_request_meanwhile_repeats_it() {
        let mut g = SweepGate::default();
        assert!(g.request());
        assert!(!g.request(), "a second sweep doesn't start");
        assert!(!g.request());
        assert!(g.done(), "the running one goes round once more");
        assert!(!g.done(), "then the gate opens");
        assert!(g.request());
    }

    #[test]
    fn a_sweep_leaves_meetings_with_a_start_lookup_running() {
        let t0 = Instant::now();
        let mut l = Linking::default();
        l.start("live", t0);
        assert!(l.is_tracked("live"));
        assert!(!l.is_tracked("other"));
        l.stop("live", t0);
        l.start_done("live", false, t0);
        assert!(!l.is_tracked("live"));
    }

    #[test]
    fn only_the_last_day_is_relinked() {
        let f = file(true);
        let start = chrono::DateTime::parse_from_rfc3339(&f.started_at).unwrap().timestamp_millis();
        assert!(relink_due(&f, start + 3600_000));
        assert!(!relink_due(&f, start + RELINK_WINDOW_MS + 1));
        assert!(!relink_due(&f, start - 1), "not before it started");
        assert!(relink_due(&file(false), start + 3600_000), "a renamed meeting still gets its event");
        let mut linked = file(true);
        linked.calendar_event = Some(CalendarEvent { title: "x".into(), attendees: vec![], emails: vec![] });
        assert!(!relink_due(&linked, start + 3600_000));
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
