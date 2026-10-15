//! Microphone access (the three Privacy & security switches) and links to the right Settings pages.
//! Windows never prompts a desktop app, so "request" just reports what the switches say.

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

/// Windows never asks for loopback capture.
pub const SYSTEM_AUDIO_PROMPT: bool = false;

const STORE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

fn value(root: RegKey, path: &str) -> Option<String> {
    root.open_subkey(path).ok()?.get_value::<String, _>("Value").ok()
}

/// Device-wide, all apps, desktop apps. A missing key means Windows' default: allowed.
pub fn status_from(device: Option<&str>, apps: Option<&str>, desktop: Option<&str>) -> &'static str {
    if [device, apps, desktop].iter().any(|v| v.is_some_and(|v| v.eq_ignore_ascii_case("Deny"))) {
        "denied"
    } else {
        "granted"
    }
}

pub fn mic_status() -> &'static str {
    let device = value(RegKey::predef(HKEY_LOCAL_MACHINE), STORE);
    let apps = value(RegKey::predef(HKEY_CURRENT_USER), STORE);
    let desktop = value(RegKey::predef(HKEY_CURRENT_USER), &format!(r"{STORE}\NonPackaged"));
    status_from(device.as_deref(), apps.as_deref(), desktop.as_deref())
}

pub fn request_mic() -> bool {
    mic_status() == "granted"
}

/// Settings pages. System audio needs no permission on Windows; its link opens Sound (output device).
pub fn settings_url(pane: &str) -> Option<&'static str> {
    match pane {
        "microphone" => Some("ms-settings:privacy-microphone"),
        "system-audio" => Some("ms-settings:sound"),
        "calendar" => Some("ms-settings:privacy-calendar"),
        _ => None,
    }
}

pub fn open_settings(pane: &str) -> Result<(), String> {
    let url = settings_url(pane).ok_or("Unknown settings pane")?;
    std::process::Command::new("explorer").arg(url).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_switch_off_means_denied() {
        assert_eq!(status_from(Some("Allow"), Some("Allow"), Some("Allow")), "granted");
        assert_eq!(status_from(Some("Deny"), Some("Allow"), Some("Allow")), "denied");
        assert_eq!(status_from(Some("Allow"), Some("Deny"), Some("Allow")), "denied");
        assert_eq!(status_from(Some("Allow"), Some("Allow"), Some("Deny")), "denied");
    }

    #[test]
    fn missing_keys_are_the_windows_default_allow() {
        assert_eq!(status_from(None, None, None), "granted");
    }

    #[test]
    fn panes_open_windows_settings() {
        assert_eq!(settings_url("microphone"), Some("ms-settings:privacy-microphone"));
        assert_eq!(settings_url("system-audio"), Some("ms-settings:sound"));
        assert_eq!(settings_url("calendar"), Some("ms-settings:privacy-calendar"));
        assert_eq!(settings_url("nope"), None);
    }

    #[test]
    fn reading_the_real_registry_does_not_fail() {
        assert!(["granted", "denied"].contains(&mic_status()));
    }
}
