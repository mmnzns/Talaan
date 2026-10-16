//! "Summary ready" notifications as Windows toasts. Clicking one opens the meeting (Talaan keeps running in the
//! tray, so the click reaches this process). Windows shows toasts only for an app id with a Start-menu shortcut:
//! the Start Menu shortcut the installer creates carries the bundle id as its AppUserModelID, so installed
//! builds use that; dev builds borrow PowerShell's.

use std::path::Path;
use std::sync::OnceLock;
use tauri_winrt_notification::Toast;

type OnClick = Box<dyn Fn(String) + Send + Sync>;
static ON_CLICK: OnceLock<OnClick> = OnceLock::new();

const APP_ID: &str = "com.craftconceptsdigital.talaan";

/// The app id toasts are posted under: Talaan's own when installed, PowerShell's for a dev build (an exe under
/// a `target` folder, or any debug build), which Windows already knows.
pub fn app_id(exe: &Path) -> &'static str {
    app_id_for(exe, cfg!(debug_assertions))
}

fn app_id_for(exe: &Path, debug_build: bool) -> &'static str {
    let dev = debug_build || exe.components().any(|c| c.as_os_str().eq_ignore_ascii_case("target"));
    if dev { Toast::POWERSHELL_APP_ID } else { APP_ID }
}

pub fn setup(on_click: impl Fn(String) + Send + Sync + 'static) {
    let _ = ON_CLICK.set(Box::new(on_click));
}

/// Shows a toast; a click calls the `setup` callback with `id`. Never fails the caller.
pub fn post(id: &str, title: &str, body: &str) {
    if let Err(e) = show(id, title, body) {
        log::warn!("notification failed: {e}");
    }
}

fn show(id: &str, title: &str, body: &str) -> Result<(), String> {
    let exe = std::env::current_exe().unwrap_or_default();
    let id = id.to_string();
    Toast::new(app_id(&exe))
        .title(title)
        .text1(body)
        .on_activated(move |_action| {
            if let Some(f) = ON_CLICK.get() {
                f(id.clone());
            }
            Ok(())
        })
        .show()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn installed_builds_use_talaan_s_own_app_id() {
        assert_eq!(app_id_for(Path::new(r"C:\Users\a\AppData\Local\Talaan\talaan.exe"), false), "com.craftconceptsdigital.talaan");
        assert_eq!(app_id_for(Path::new(r"C:\Program Files\Talaan\Talaan.exe"), false), "com.craftconceptsdigital.talaan");
    }

    #[test]
    fn dev_builds_borrow_an_app_id_windows_already_knows() {
        assert_eq!(app_id_for(Path::new(r"C:\Dev\Talaan\src-tauri\target\debug\talaan.exe"), false), Toast::POWERSHELL_APP_ID);
        assert_eq!(app_id_for(Path::new(r"C:\Anywhere\talaan.exe"), true), Toast::POWERSHELL_APP_ID);
    }

    /// The installer stamps the Start Menu shortcut with the bundle identifier as its AppUserModelID (Tauri's
    /// NSIS template, SetLnkAppUserModelId). Windows shows our toasts only under that id.
    #[test]
    fn the_app_id_is_the_identifier_the_installer_puts_on_the_shortcut() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../../../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], APP_ID);
    }

    /// Manual: posts one toast under the dev app id and reports whether Windows accepted it.
    #[test]
    #[ignore = "shows a real toast"]
    fn toast_smoke() {
        let r = show("smoke", "Talaan test", "Summary ready: toast smoke test");
        println!("toast show() returned: {r:?}");
        std::thread::sleep(std::time::Duration::from_secs(5));
        assert!(r.is_ok());
    }
}
