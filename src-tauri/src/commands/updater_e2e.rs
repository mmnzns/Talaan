//! Test builds only: the updater end-to-end test (.github/workflows/updater-test.yml, docs/TESTING.md).
//!
//! Compiled only with the `updater-e2e` Cargo feature, which only that workflow turns on (through its throwaway
//! config overlay); release builds never have this module, and `scripts/release-windows.ps1` fails if a release
//! `talaan.exe` contains [`ENV`]. In a test build, every launch writes `updater-e2e-<version>.json` to the log folder
//! (what the test checks after the update), and with `TALAAN_UPDATER_E2E=install` it also runs the same
//! `updates_check` and `updates_install` the Settings buttons run, without a click.

use super::updates::{updates_check, updates_install};
use super::AppState;
use serde_json::{json, Value};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt as _;

pub const ENV: &str = "TALAAN_UPDATER_E2E";

/// Let the app settle (login item applied, windows loading) before reporting and updating.
const SETTLE: Duration = Duration::from_secs(8);

pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SETTLE).await;
        let mut report = snapshot(&app);
        write(&app, &report);
        if std::env::var(ENV).as_deref() != Ok("install") {
            return;
        }
        let check = match updates_check(app.clone()).await {
            Ok(found) => json!({ "ok": found.is_some(), "found": found }),
            Err(e) => json!({ "ok": false, "error": e.to_string() }),
        };
        let newer = check["ok"].as_bool().unwrap_or(false);
        log::info!("updater-e2e: newer version found: {newer}");
        report["check"] = check;
        if !newer {
            write(&app, &report);
            return;
        }
        report["install"] = json!("started");
        write(&app, &report);
        // On success this never returns: Windows exits for the installer, the Mac restarts.
        if let Err(e) = updates_install(app.clone()).await {
            log::error!("updater-e2e: install failed: {}", crate::logging::text(&e));
            report["install"] = json!({ "error": e.to_string() });
            write(&app, &report);
        }
    });
}

fn snapshot(app: &AppHandle) -> Value {
    let (meetings, settings) = {
        let state = app.state::<AppState>();
        let store = state.lock();
        let meetings: Vec<String> = store.list_meetings(&Default::default()).map(|v| v.into_iter().map(|m| m.id).collect()).unwrap_or_default();
        (meetings, store.settings_get().ok().flatten().unwrap_or(Value::Null))
    };
    json!({
        "version": app.package_info().version.to_string(),
        "pid": std::process::id(),
        "exe": std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default(),
        "args": std::env::args().skip(1).collect::<Vec<_>>(),
        "env": std::env::var(ENV).unwrap_or_default(),
        "meetings": meetings,
        "settings": { "launchAtLogin": settings["launchAtLogin"], "yourName": settings["yourName"], "checkUpdates": settings["checkUpdates"] },
        "autostart": match app.autolaunch().is_enabled() { Ok(on) => json!(on), Err(e) => json!(e.to_string()) },
        "at": chrono::Local::now().to_rfc3339(),
    })
}

fn write(app: &AppHandle, report: &Value) {
    let Some(dir) = crate::logging::dir() else { return };
    let path = dir.join(format!("updater-e2e-{}.json", app.package_info().version));
    let _ = std::fs::write(path, serde_json::to_vec_pretty(report).unwrap_or_default());
}
