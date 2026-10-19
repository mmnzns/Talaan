//! Updates (GitHub Releases, signed with Talaan's own key) and Open at login.
//!
//! The newest version found is kept here (`UpdateState`), so every window shows the same thing: the main
//! window's banner, the menu bar popover's line and Settings › General ask with `updates_pending`, and hear
//! `update-available` when a check finds one. The background check and its one notification per version are
//! in `crate::update_notice` (when) and below (how).

use super::{AppState, CmdResult};
use crate::i18n::{tr, tr_err, tr_with};
use crate::storage::StorageError;
use crate::update_notice::{check_due, notice_path, read_notified, write_notified, Last, Notice};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_updater::UpdaterExt;

pub const UPDATE_AVAILABLE: &str = "update-available";
/// Download progress while installing: `{ percent }` (None when the size isn't known yet), then `{ restarting }`.
pub const UPDATE_PROGRESS: &str = "update-progress";
/// A click on the update notification: the main window shows the banner again (even after Later).
pub const SHOW_UPDATE: &str = "show-update";

fn refused(m: impl Into<String>) -> StorageError {
    StorageError::Refused(m.into())
}

#[derive(Serialize, Clone)]
pub struct Available {
    version: String,
    notes: Option<String>,
}

/// The shared update state: the newest version found, and which one was announced.
pub struct UpdateState {
    pending: Mutex<Option<Available>>,
    notice: Mutex<Notice>,
    notice_file: PathBuf,
    /// Check at the next tick, not on the schedule (Talaan was started by a click on the update notification).
    soon: AtomicBool,
    installing: AtomicBool,
}

impl UpdateState {
    fn pending(&self) -> std::sync::MutexGuard<'_, Option<Available>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn notice(&self) -> std::sync::MutexGuard<'_, Notice> {
        self.notice.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Call once at launch, after `AppState`.
pub fn setup(app: &AppHandle) {
    let notice_file = notice_path(app.state::<AppState>().lock().root());
    let notice = Notice::new(read_notified(&notice_file));
    app.manage(UpdateState { pending: Mutex::new(None), notice: Mutex::new(notice), notice_file, soon: AtomicBool::new(false), installing: AtomicBool::new(false) });
}

fn setting(app: &AppHandle, key: &str) -> bool {
    app.state::<AppState>().lock().settings_get().ok().flatten().and_then(|s| s[key].as_bool()).unwrap_or(true)
}

/// Asks GitHub and records the answer in the shared state. Nothing is downloaded.
async fn check(app: &AppHandle) -> CmdResult<Option<Available>> {
    let current = app.package_info().version.to_string();
    let updater = app.updater().inspect_err(|e| log::warn!("update check failed: {}", crate::logging::text(e))).map_err(|e| refused(tr_err("update.check_failed", e)))?;
    let update = updater.check().await.inspect_err(|e| log::warn!("update check failed (offline?): {}", crate::logging::text(e))).map_err(|e| refused(tr_err("update.offline", e)))?;
    match &update {
        Some(u) => log::info!("update check: {} is available (this is {current})", u.version),
        None => log::info!("update check: {current} is the newest"),
    }
    let found = update.map(|u| Available { version: u.version, notes: u.body });
    if let Some(state) = app.try_state::<UpdateState>() {
        *state.pending() = found.clone();
    }
    if let Some(a) = &found {
        let _ = app.emit(UPDATE_AVAILABLE, a);
    }
    Ok(found)
}

/// Check now (Settings › General). What it finds is what the banner shows; seen here, it isn't announced again.
#[tauri::command]
pub async fn updates_check(app: AppHandle) -> CmdResult<Option<Available>> {
    let found = check(&app).await?;
    if let (Some(a), Some(state)) = (&found, app.try_state::<UpdateState>()) {
        let mut notice = state.notice();
        if notice.notified() != Some(a.version.as_str()) {
            notice.posted(&a.version);
            remember_notified(&state, &a.version);
        }
    }
    Ok(found)
}

/// The newer version found by the last check, if any, for a window that opens after it.
#[tauri::command]
pub fn updates_pending(app: AppHandle) -> Option<Available> {
    app.try_state::<UpdateState>().and_then(|s| s.pending().clone())
}

/// Downloads, verifies (signature), installs and relaunches. Refused while recording.
#[tauri::command]
pub async fn updates_install(app: AppHandle) -> CmdResult<()> {
    if super::recorder::is_recording(&app) {
        return Err(refused(tr("update.stop_recording")));
    }
    let state = app.try_state::<UpdateState>();
    // Two windows can ask at once; the second waits for the restart the first one causes.
    if state.as_ref().is_some_and(|s| s.installing.swap(true, Ordering::SeqCst)) {
        return Ok(());
    }
    let done = |r: CmdResult<()>| {
        if let Some(s) = &state {
            s.installing.store(false, Ordering::SeqCst);
        }
        r
    };
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => return done(Err(refused(tr_err("update.check_failed", e)))),
    };
    let update = match updater.check().await {
        Ok(Some(u)) => u,
        Ok(None) => return done(Err(refused(tr("update.up_to_date")))),
        Err(e) => return done(Err(refused(tr_err("update.check_failed", e)))),
    };
    log::info!("installing update {}", update.version);
    let (progress_app, finish_app) = (app.clone(), app.clone());
    let mut got: u64 = 0;
    let mut said: Option<Option<u8>> = None;
    let on_chunk = move |chunk: usize, total: Option<u64>| {
        got += chunk as u64;
        let percent = total.filter(|t| *t > 0).map(|t| (got.saturating_mul(100) / t).min(100) as u8);
        // Only when the whole percent changes (chunks are a few kilobytes).
        if said != Some(percent) {
            said = Some(percent);
            let _ = progress_app.emit(UPDATE_PROGRESS, serde_json::json!({ "percent": percent }));
        }
    };
    let on_finish = move || {
        let _ = finish_app.emit(UPDATE_PROGRESS, serde_json::json!({ "percent": 100, "restarting": true }));
    };
    if let Err(e) = update.download_and_install(on_chunk, on_finish).await {
        log::error!("update install failed: {}", crate::logging::text(&e));
        return done(Err(refused(tr_err("update.install_failed", e))));
    }
    log::info!("update installed, restarting");
    app.restart()
}

/// The background check: 20 s after launch, then every 6 hours (`update_notice`), while "Check for updates
/// automatically" is on. Not in debug builds (`tauri dev`), as before.
pub fn check_at_launch(app: &AppHandle) {
    if cfg!(debug_assertions) {
        log::info!("update checks: skipped (debug build)");
        return;
    }
    if !setting(app, "checkUpdates") {
        log::info!("update checks: turned off");
    }
    let app = app.clone();
    let started = now_secs();
    let _ = std::thread::Builder::new().name("talaan-updates".into()).spawn(move || {
        let mut last: Option<Last> = None;
        loop {
            std::thread::sleep(Duration::from_secs(5));
            let soon = app.try_state::<UpdateState>().is_some_and(|s| s.soon.swap(false, Ordering::SeqCst));
            if !soon && !check_due(started, last, now_secs()) {
                continue;
            }
            // Read when a check is due, so turning the setting on or off needs no restart. Off: look at it again
            // in an hour (the retry delay), not every tick.
            if !setting(&app, "checkUpdates") {
                last = Some(Last { at: now_secs(), ok: false });
                continue;
            }
            let result = tauri::async_runtime::block_on(check(&app));
            last = Some(Last { at: now_secs(), ok: result.is_ok() });
            if let Ok(Some(a)) = result {
                found_in_background(&app, &a.version);
            }
        }
    });
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A background check found `version`: announce it, unless it was announced before or a call is being recorded.
fn found_in_background(app: &AppHandle, version: &str) {
    let Some(state) = app.try_state::<UpdateState>() else { return };
    let recording = super::recorder::is_recording(app);
    let due = state.notice().found(version, recording);
    if recording {
        log::info!("update notification waits for the recording to end");
    }
    if let Some(v) = due {
        announce(app, &state, &v);
    }
}

/// A recording ended (stopped or discarded): post the update notification that waited for it.
pub fn recording_ended(app: &AppHandle) {
    let Some(state) = app.try_state::<UpdateState>() else { return };
    let due = state.notice().recording_stopped();
    if let Some(v) = due {
        announce(app, &state, &v);
    }
}

/// The "Talaan {version} is ready" notification, unless notifications are turned off in Settings › General (then
/// the banner alone says it, and the version isn't marked, so it's announced once they're back on). The system's
/// own switch for Talaan's notifications applies too (the platform code asks, as for Summary ready).
fn announce(app: &AppHandle, state: &UpdateState, version: &str) {
    if !setting(app, "notifySummaryReady") {
        log::info!("update notification: notifications are turned off");
        return;
    }
    crate::platform::notify::post(crate::deeplink::UPDATE_NOTICE, &tr_with("notify.update_ready", &[("version", version)]), &tr("notify.update_body"));
    log::info!("update notification posted for {version}");
    state.notice().posted(version);
    remember_notified(state, version);
}

fn remember_notified(state: &UpdateState, version: &str) {
    if let Err(e) = write_notified(&state.notice_file, version) {
        log::warn!("couldn't save which update was announced: {}", e.for_log());
    }
}

/// A click on the update notification: the main window, with the banner showing.
pub fn open_banner(app: &AppHandle) {
    super::tray::show_main(app);
    let _ = app.emit(SHOW_UPDATE, ());
}

/// Talaan was started by a click on the update notification (Windows): the main window, and a check right away
/// so the banner has something to show.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn open_banner_at_launch(app: &AppHandle) {
    if let Some(s) = app.try_state::<UpdateState>() {
        s.soon.store(true, Ordering::SeqCst);
    }
    super::tray::show_main(app);
}

/// Makes the login item match the setting (default on). Called at launch and whenever settings change.
pub fn apply_login_item(app: &AppHandle) {
    let want = setting(app, "launchAtLogin");
    let auto = app.autolaunch();
    let have = auto.is_enabled().unwrap_or(false);
    // Only a real app bundle should register itself (not `tauri dev`).
    if want != have && !cfg!(debug_assertions) {
        let _ = if want { auto.enable() } else { auto.disable() };
    }
}

/// "Delete all Talaan data" also removes the login item.
pub fn remove_login_item(app: &AppHandle) {
    let _ = app.autolaunch().disable();
}
