//! Microphone permission (AVFoundation), and links to the right System Settings panes.
//!
//! System audio recording has no public status call. Talaan learns it is blocked when the tap delivers
//! nothing but digital silence while the output device is playing (see `recorder` silence watch).

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
use std::sync::mpsc::channel;
use std::time::Duration;

pub fn mic_status() -> &'static str {
    let Some(media) = (unsafe { AVMediaTypeAudio }) else { return "unknown" };
    match unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) } {
        AVAuthorizationStatus::Authorized => "granted",
        AVAuthorizationStatus::Denied => "denied",
        AVAuthorizationStatus::Restricted => "restricted",
        AVAuthorizationStatus::NotDetermined => "undetermined",
        _ => "unknown",
    }
}

/// Shows the macOS prompt if it hasn't been answered yet. Returns true if allowed.
pub fn request_mic() -> bool {
    let Some(media) = (unsafe { AVMediaTypeAudio }) else { return false };
    let (tx, rx) = channel();
    let block = RcBlock::new(move |granted: Bool| {
        let _ = tx.send(granted.as_bool());
    });
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &block) };
    rx.recv_timeout(Duration::from_secs(120)).unwrap_or(false)
}

/// System Settings › Privacy & Security panes.
pub fn settings_url(pane: &str) -> Option<&'static str> {
    match pane {
        "microphone" => Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"),
        "system-audio" => Some("x-apple.systempreferences:com.apple.preference.security?Privacy_AudioCapture"),
        "calendar" => Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars"),
        "automation" => Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Automation"),
        _ => None,
    }
}

pub fn open_settings(pane: &str) -> Result<(), String> {
    let url = settings_url(pane).ok_or("Unknown settings pane")?;
    std::process::Command::new("open").arg(url).status().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_is_one_of_the_known_values() {
        assert!(["granted", "denied", "restricted", "undetermined", "unknown"].contains(&mic_status()));
    }

    #[test]
    fn known_panes_have_urls() {
        assert!(settings_url("microphone").unwrap().contains("Privacy_Microphone"));
        assert!(settings_url("system-audio").unwrap().contains("Privacy_AudioCapture"));
        assert!(settings_url("nope").is_none());
    }
}
