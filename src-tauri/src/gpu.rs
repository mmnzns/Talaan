//! The graphics card: the default for the setting, and what fell back to the processor this run.
//! The engine (voice-engine) picks the card, runs the helpers and remembers failures; this file holds only what
//! Talaan decides: whether the card is on for a new install, and the note "the card failed" for Settings.

use serde_json::Value;
use std::sync::{Mutex, OnceLock};

/// The PC's discrete graphics card, by the name the driver gives it. Windows only (a cheap DXGI listing; no helper
/// starts). None on the Mac, with built-in graphics only, or when the check fails.
/// The listing runs once per process; every later call gets the same answer.
pub fn discrete_card() -> Option<String> {
    static CARD: OnceLock<Option<String>> = OnceLock::new();
    CARD.get_or_init(|| {
        #[cfg(windows)]
        {
            voice_engine_stt::discrete_card().map(|d| d.description)
        }
        #[cfg(not(windows))]
        {
            None
        }
    })
    .clone()
}

/// Whether the card is on by default: always on the Mac; on a PC only with a discrete card.
pub fn default_on(mac: bool, card: Option<&str>) -> bool {
    mac || card.is_some()
}

/// Sets the card default once (`gpuDefaultSet` missing). On the Mac it only marks it done and leaves `useGpu`
/// (default on). On Windows a PC without a discrete card gets the card off; a PC with one keeps `useGpu` as it is
/// stored (on when absent), so nobody who had the card off finds it switched on by an update. Returns whether the
/// settings changed.
pub fn apply_default(settings: &mut Value, mac: bool, card: Option<&str>) -> bool {
    if settings.get("gpuDefaultSet").is_some() {
        return false;
    }
    if !settings.is_object() {
        *settings = Value::Object(Default::default());
    }
    if !mac && !default_on(mac, card) {
        settings["useGpu"] = false.into();
    }
    settings["gpuDefaultSet"] = true.into();
    true
}

/// The default for a settings save from the screens (`value` is what is about to be written). A first launch has no
/// settings file, so the start-up default never ran and the first save would carry the screens' `useGpu: true` even
/// on a PC without a discrete card. A save that already has `gpuDefaultSet`, or whose stored settings do, is left
/// alone (the person's own choice stays). Returns whether `value` changed.
pub fn apply_default_on_save(value: &mut Value, stored: Option<&Value>, mac: bool, card: Option<&str>) -> bool {
    if value.get("gpuDefaultSet").is_some() {
        return false;
    }
    if let Some(done) = stored.and_then(|s| s.get("gpuDefaultSet")) {
        if value.is_object() {
            value["gpuDefaultSet"] = done.clone();
        }
        return false;
    }
    apply_default(value, mac, card)
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
    fn a_pc_with_a_card_keeps_what_was_stored_and_is_never_switched_on() {
        const RTX: Option<&str> = Some("NVIDIA GeForce RTX 5070 Ti");
        // Updating from a version where the person had switched the card off: it stays off.
        let mut off = serde_json::json!({ "useGpu": false });
        assert!(apply_default(&mut off, false, RTX));
        assert_eq!((off["useGpu"].clone(), off["gpuDefaultSet"].clone()), (false.into(), true.into()));
        // Stored on: stays on. Nothing stored: left absent, which reads as on.
        let mut on = serde_json::json!({ "useGpu": true });
        assert!(apply_default(&mut on, false, RTX));
        assert_eq!(on["useGpu"], true);
        let mut absent = serde_json::json!({});
        assert!(apply_default(&mut absent, false, RTX));
        assert!(absent.get("useGpu").is_none() && absent["gpuDefaultSet"] == true);
        assert!(crate::transcriber::worker::Config::from_settings(Some(&absent)).use_gpu);
        // Without a card it is switched off, whatever was stored.
        let mut stored_on = serde_json::json!({ "useGpu": true });
        assert!(apply_default(&mut stored_on, false, None));
        assert_eq!(stored_on["useGpu"], false);
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
    fn the_first_save_carries_the_card_default() {
        // First launch on a PC with no discrete card: the screens send useGpu true, the save turns it off.
        let mut first = serde_json::json!({ "useGpu": true, "yourName": "A" });
        assert!(apply_default_on_save(&mut first, None, false, None));
        assert_eq!((first["useGpu"].clone(), first["gpuDefaultSet"].clone()), (false.into(), true.into()));
        // With a discrete card it stays on.
        let mut card = serde_json::json!({ "useGpu": true });
        assert!(apply_default_on_save(&mut card, None, false, Some("RTX")));
        assert_eq!(card["useGpu"], true);
        // The Mac keeps useGpu and only marks it done.
        let mut mac = serde_json::json!({ "useGpu": true });
        assert!(apply_default_on_save(&mut mac, None, true, None));
        assert_eq!((mac["useGpu"].clone(), mac["gpuDefaultSet"].clone()), (true.into(), true.into()));
    }

    #[test]
    fn later_saves_keep_the_persons_choice() {
        // The value already says it was done: untouched.
        let mut done = serde_json::json!({ "useGpu": true, "gpuDefaultSet": true });
        assert!(!apply_default_on_save(&mut done, None, false, None));
        assert_eq!(done["useGpu"], true);
        // The screens dropped the marker but the stored file has it: carried over, useGpu untouched.
        let stored = serde_json::json!({ "useGpu": false, "gpuDefaultSet": true });
        let mut dropped = serde_json::json!({ "useGpu": true });
        assert!(!apply_default_on_save(&mut dropped, Some(&stored), false, None));
        assert_eq!((dropped["useGpu"].clone(), dropped["gpuDefaultSet"].clone()), (true.into(), true.into()));
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
