//! "Summary ready" notifications as Windows toasts. Clicking one opens the meeting. Windows shows toasts only for
//! an app id with a Start-menu shortcut: the Start Menu shortcut the installer creates carries the bundle id as its
//! AppUserModelID, so installed builds use that; dev builds borrow PowerShell's.
//!
//! How a click reaches Talaan: when the `talaan://` protocol is registered for this exe (the installer does it),
//! the toast is a protocol toast (`activationType="protocol"`, `launch="talaan://meeting/<id>"`). Windows then
//! starts `talaan.exe "talaan://meeting/<id>"` on a click, from the banner or later from Action Center, and the
//! single-instance plugin hands the link to the running Talaan (`crate::deeplink`). Without the protocol (a dev
//! build), the click is a foreground activation handled in this process, which works only while the banner is up:
//! from Action Center that needs a COM activator, which Talaan doesn't have.

use crate::deeplink;
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

/// Whether Windows will show Talaan's toasts: false when they're turned off for Talaan or for every app
/// (Settings › System › Notifications). None when Windows can't say.
pub fn allowed() -> Option<bool> {
    use windows::UI::Notifications::{NotificationSetting, ToastNotificationManager};
    super::com::init();
    let exe = std::env::current_exe().unwrap_or_default();
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&windows_core::HSTRING::from(app_id(&exe))).ok()?;
    Some(notifier.Setting().ok()? == NotificationSetting::Enabled)
}

/// Shows a toast; a click calls the `setup` callback with `id`. Never fails the caller.
pub fn post(id: &str, title: &str, body: &str) {
    if let Err(e) = show(id, title, body) {
        eprintln!("toast failed: {e}");
    }
}

fn show(id: &str, title: &str, body: &str) -> Result<(), String> {
    // WinRT on this thread (the summarizer worker), both toast paths.
    super::com::init();
    let exe = std::env::current_exe().unwrap_or_default();
    if protocol_is_ours(&exe) {
        return show_protocol(app_id(&exe), &toast_xml(title, body, &deeplink::meeting_uri(id))).map_err(|e| e.to_string());
    }
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

/// Whether `talaan://` opens this very exe (HKCU, where the per-user installer writes it). A dev build next to an
/// installed Talaan must not have its clicks start the installed one, so the command has to name `exe`.
fn protocol_is_ours(exe: &Path) -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    let key = format!(r"Software\Classes\{}\shell\open\command", deeplink::SCHEME);
    winreg::RegKey::predef(HKEY_CURRENT_USER).open_subkey(key).and_then(|k| k.get_value::<String, _>("")).is_ok_and(|cmd| command_opens(&cmd, exe))
}

/// Whether a protocol's open command (`"C:\…\talaan.exe" "%1"`) starts `exe` with the link.
fn command_opens(command: &str, exe: &Path) -> bool {
    let command = command.trim();
    let program = match command.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(""),
        None => command.split(' ').next().unwrap_or(""),
    };
    !program.is_empty() && command.contains("%1") && program.eq_ignore_ascii_case(&exe.to_string_lossy())
}

/// A protocol toast: a click (banner or Action Center) opens `launch` through the shell.
fn toast_xml(title: &str, body: &str, launch: &str) -> String {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;");
    format!(
        r#"<toast activationType="protocol" launch="{}"><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual></toast>"#,
        esc(launch),
        esc(title),
        esc(body)
    )
}

fn show_protocol(app_id: &str, xml: &str) -> ::windows::core::Result<()> {
    use ::windows::core::HSTRING;
    use ::windows::Data::Xml::Dom::XmlDocument;
    use ::windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    super::com::init();
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))?.Show(&toast)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn the_protocol_counts_only_when_it_opens_this_exe() {
        let exe = Path::new(r"C:\Users\a\AppData\Local\Talaan\talaan.exe");
        assert!(command_opens(r#""C:\Users\a\AppData\Local\Talaan\talaan.exe" "%1""#, exe));
        assert!(command_opens(r#""c:\users\a\appdata\local\talaan\TALAAN.EXE" "%1""#, exe));
        assert!(command_opens(r"C:\Users\a\AppData\Local\Talaan\talaan.exe %1", exe));
        assert!(!command_opens(r#""C:\Dev\Talaan\src-tauri\target\debug\talaan.exe" "%1""#, exe));
        assert!(!command_opens(r#""C:\Users\a\AppData\Local\Talaan\talaan.exe""#, exe), "no link argument");
        assert!(!command_opens("", exe));
    }

    #[test]
    fn protocol_toasts_carry_the_link_and_escape_text() {
        let xml = toast_xml("Summary ready", "Q4 <plan> & \"budget\"", "talaan://meeting/2026-10-14_1430_ab12");
        assert!(xml.starts_with(r#"<toast activationType="protocol" launch="talaan://meeting/2026-10-14_1430_ab12">"#), "{xml}");
        assert!(xml.contains("<text>Q4 &lt;plan&gt; &amp; &quot;budget&quot;</text>"), "{xml}");
    }

    /// Windows' XML parser reads the protocol toast (as `show_protocol` does), with COM set up as the app does
    /// it. No `ToastNotification` is built here: CI runners (Windows Server) may have no notification platform,
    /// and the old version of this test, which built one on a thread without COM, crashed the test binary there
    /// (STATUS_ACCESS_VIOLATION). Parsing happens on fresh threads that end, which used to tear the MTA down.
    #[test]
    fn windows_reads_the_protocol_toast() {
        use ::windows::core::HSTRING;
        use ::windows::Data::Xml::Dom::XmlDocument;
        for _ in 0..3 {
            std::thread::spawn(|| {
                super::super::com::init();
                let doc = XmlDocument::new().unwrap();
                doc.LoadXml(&HSTRING::from(toast_xml("Summary ready", "A & B", "talaan://meeting/abc"))).unwrap();
                let root = doc.DocumentElement().unwrap();
                assert_eq!(root.GetAttribute(&HSTRING::from("activationType")).unwrap(), "protocol");
                assert_eq!(root.GetAttribute(&HSTRING::from("launch")).unwrap(), "talaan://meeting/abc");
            })
            .join()
            .unwrap();
        }
    }

    /// Manual: posts one protocol toast (talaan://meeting/<id>, id from TALAAN_TOAST_MEETING or "smoke") under the
    /// dev app id. With the protocol registered for a dev build, clicking it, also from Action Center, opens it.
    #[test]
    #[ignore = "shows a real toast"]
    fn protocol_toast_smoke() {
        let id = std::env::var("TALAAN_TOAST_MEETING").unwrap_or_else(|_| "smoke".into());
        let r = show_protocol(Toast::POWERSHELL_APP_ID, &toast_xml("Talaan test", "Summary ready: protocol toast smoke test", &deeplink::meeting_uri(&id)));
        println!("protocol toast show() returned: {r:?}");
        assert!(r.is_ok());
    }

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
