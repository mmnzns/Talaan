//! The graphics card: the default for the setting, and what fell back to the processor this run.
//! The engine (voice-engine) picks the card, runs the helpers and remembers failures; this file holds only what
//! Talaan decides: whether the card is on for a new install, and the note "the card failed" for Settings.

use serde_json::Value;
use std::sync::Mutex;

/// The PC's discrete graphics card, by the name the driver gives it. Windows only (a cheap DXGI listing; no helper
/// starts). None on the Mac, with built-in graphics only, or when the check fails.
pub fn discrete_card() -> Option<String> {
    #[cfg(windows)]
    {
        voice_engine_stt::discrete_card().map(|d| d.description)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Whether the card is on by default: always on the Mac; on a PC only with a discrete card.
pub fn default_on(mac: bool, card: Option<&str>) -> bool {
    mac || card.is_some()
}

/// Sets the card default once (`gpuDefaultSet` missing). On the Mac it only marks it done and leaves `useGpu`
/// (default on). Returns whether the settings changed.
pub fn apply_default(settings: &mut Value, mac: bool, card: Option<&str>) -> bool {
    if settings.get("gpuDefaultSet").is_some() {
        return false;
    }
    if !settings.is_object() {
        *settings = Value::Object(Default::default());
    }
    if !mac {
        settings["useGpu"] = default_on(mac, card).into();
    }
    settings["gpuDefaultSet"] = true.into();
    true
}

/// Which jobs fell back to the processor since the last "Try again", with the backend that did the work.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FellBack {
    pub stt: Option<String>,
    pub llm: Option<String>,
}

static FELL_BACK: Mutex<FellBack> = Mutex::new(FellBack { stt: None, llm: None });

fn fell_back() -> std::sync::MutexGuard<'static, FellBack> {
    FELL_BACK.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn note_stt_fallback(backend: &str) {
    fell_back().stt = Some(backend.to_string());
}

pub fn note_llm_fallback(backend: &str) {
    fell_back().llm = Some(backend.to_string());
}

pub fn fallbacks() -> FellBack {
    fell_back().clone()
}

pub fn clear_fallbacks() {
    *fell_back() = FellBack::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_default_is_on_on_the_mac_and_on_a_pc_only_with_a_discrete_card() {
        assert!(default_on(true, None)); // Mac
        assert!(default_on(false, Some("NVIDIA GeForce RTX 5070 Ti")));
        assert!(!default_on(false, None)); // built-in graphics only, or the check failed
    }

    #[test]
    fn the_windows_default_is_set_once_and_never_again() {
        let mut s = serde_json::json!({});
        assert!(apply_default(&mut s, false, None));
        assert_eq!(s["useGpu"], false);
        assert_eq!(s["gpuDefaultSet"], true);
        s["useGpu"] = true.into(); // the person switched it on
        assert!(!apply_default(&mut s, false, None));
        assert_eq!(s["useGpu"], true);
    }

    #[test]
    fn the_mac_only_marks_the_default_done() {
        let mut s = serde_json::json!({ "useGpu": false });
        assert!(apply_default(&mut s, true, None));
        assert_eq!(s["useGpu"], false);
        assert_eq!(s["gpuDefaultSet"], true);
        let mut card = serde_json::json!({ "useGpu": true });
        assert!(apply_default(&mut card, false, Some("RTX")));
        assert_eq!(card["useGpu"], true);
    }

    #[test]
    fn fallbacks_are_noted_and_cleared() {
        clear_fallbacks();
        assert_eq!(fallbacks(), FellBack::default());
        note_stt_fallback("cpu");
        note_llm_fallback("cpu");
        assert_eq!(fallbacks(), FellBack { stt: Some("cpu".into()), llm: Some("cpu".into()) });
        clear_fallbacks();
        assert_eq!(fallbacks(), FellBack::default());
    }
}
