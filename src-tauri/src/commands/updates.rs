//! Updates (GitHub Releases, signed with Talaan's own key) and Open at login.

use super::{AppState, CmdResult};
use crate::i18n::{tr, tr_err};
use crate::storage::StorageError;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_updater::UpdaterExt;

pub const UPDATE_AVAILABLE: &str = "update-available";

fn refused(m: impl Into<String>) -> StorageError {
    StorageError::Refused(m.into())
}

#[derive(Serialize, Clone)]
pub struct Available {
    version: String,
    notes: Option<String>,
}

/// The newer version on GitHub, if there is one. Nothing is downloaded.
#[tauri::command]
pub async fn updates_check(app: AppHandle) -> CmdResult<Option<Available>> {
    let current = app.package_info().version.to_string();
    let updater = app.updater().inspect_err(|e| log::warn!("update check failed: {e}")).map_err(|e| refused(tr_err("update.check_failed", e)))?;
    let update = updater.check().await.inspect_err(|e| log::warn!("update check failed (offline?): {e}")).map_err(|e| refused(tr_err("update.offline", e)))?;
    match &update {
        Some(u) => log::info!("update check: {} is available (this is {current})", u.version),
        None => log::info!("update check: {current} is the newest"),
    }
    Ok(update.map(|u| Available { version: u.version, notes: u.body }))
}

/// Downloads, verifies (signature), installs and relaunches. Refused while recording.
#[tauri::command]
pub async fn updates_install(app: AppHandle) -> CmdResult<()> {
    if super::recorder::is_recording(&app) {
        return Err(refused(tr("update.stop_recording")));
    }
    let updater = app.updater().map_err(|e| refused(tr_err("update.check_failed", e)))?;
    let Some(update) = updater.check().await.map_err(|e| refused(tr_err("update.check_failed", e)))? else {
        return Err(refused(tr("update.up_to_date")));
    };
    log::info!("installing update {}", update.version);
    update.download_and_install(|_, _| {}, || {}).await.inspect_err(|e| log::error!("update install failed: {e}")).map_err(|e| refused(tr_err("update.install_failed", e)))?;
    log::info!("update installed, restarting");
    app.restart()
}

/// At launch, look once (quietly) and tell the windows when there's something newer.
pub fn check_at_launch(app: &AppHandle) {
    let app = app.clone();
    let on = app.state::<AppState>().lock().settings_get().ok().flatten().and_then(|s| s["checkUpdates"].as_bool()).unwrap_or(true);
    if !on || cfg!(debug_assertions) {
        log::info!("update check at launch: {}", if on { "skipped (debug build)" } else { "turned off" });
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio_sleep().await;
        if let Ok(Some(a)) = updates_check(app.clone()).await {
            let _ = app.emit(UPDATE_AVAILABLE, a);
        }
    });
}

async fn tokio_sleep() {
    // Let the app settle first; an update check is never urgent.
    tauri::async_runtime::spawn_blocking(|| std::thread::sleep(std::time::Duration::from_secs(20))).await.ok();
}

/// Makes the login item match the setting (default on). Called at launch and whenever settings change.
pub fn apply_login_item(app: &AppHandle) {
    let want = app.state::<AppState>().lock().settings_get().ok().flatten().and_then(|s| s["launchAtLogin"].as_bool()).unwrap_or(true);
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
