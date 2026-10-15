//! "Summary ready" notifications as Windows toasts. Clicking one opens the meeting (Talaan keeps running in the
//! tray, so the click reaches this process). Windows shows toasts only for an app id with a Start-menu shortcut:
//! the installer's (Talaan's bundle id) when installed, PowerShell's for dev builds.

use std::path::Path;
use std::sync::OnceLock;
use tauri_winrt_notification::Toast;

type OnClick = Box<dyn Fn(String) + Send + Sync>;
static ON_CLICK: OnceLock<OnClick> = OnceLock::new();

const APP_ID: &str = "com.craftconceptsdigital.talaan";

/// The app id toasts are posted under: Talaan's own when installed, PowerShell's for a dev build (an exe under
/// a `target` folder), which Windows already knows.
pub fn app_id(exe: &Path) -> &'static str {
    let dev = exe.components().any(|c| c.as_os_str().eq_ignore_ascii_case("target"));
    if dev { Toast::POWERSHELL_APP_ID } else { APP_ID }
}

pub fn setup(on_click: impl Fn(String) + Send + Sync + 'static) {
    let _ = ON_CLICK.set(Box::new(on_click));
}

/// Shows a toast; a click calls the `setup` callback with `id`. Never fails the caller.
pub fn post(id: &str, title: &str, body: &str) {
    let _ = show(id, title, body);
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
        assert_eq!(app_id(Path::new(r"C:\Users\a\AppData\Local\Talaan\talaan.exe")), "com.craftconceptsdigital.talaan");
        assert_eq!(app_id(Path::new(r"C:\Program Files\Talaan\Talaan.exe")), "com.craftconceptsdigital.talaan");
    }

    #[test]
    fn dev_builds_borrow_an_app_id_windows_already_knows() {
        assert_eq!(app_id(Path::new(r"C:\Dev\Talaan\src-tauri\target\debug\talaan.exe")), Toast::POWERSHELL_APP_ID);
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
