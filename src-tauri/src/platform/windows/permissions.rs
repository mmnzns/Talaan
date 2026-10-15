//! Microphone permission and settings links (Windows has no per-app prompt for desktop apps yet).

/// Windows never asks for loopback capture.
pub const SYSTEM_AUDIO_PROMPT: bool = false;

pub fn mic_status() -> &'static str {
    "granted"
}

pub fn request_mic() -> bool {
    true
}

pub fn settings_url(_pane: &str) -> Option<&'static str> {
    None
}

pub fn open_settings(_pane: &str) -> Result<(), String> {
    Err("Not on this platform yet".into())
}
