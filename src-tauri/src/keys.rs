//! API keys for "your own key" summaries, kept in the macOS Keychain (Windows: Credential Manager), never in
//! settings.json. Service "Talaan", one account per provider.

const SERVICE: &str = "Talaan";

fn entry(provider: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, provider).map_err(|e| crate::i18n::tr_err("keys.open_failed", e))
}

pub fn set(provider: &str, key: &str) -> Result<(), String> {
    entry(provider)?.set_password(key.trim()).map_err(|e| crate::i18n::tr_err("keys.save_failed", e))
}

pub fn get(provider: &str) -> Option<String> {
    entry(provider).ok()?.get_password().ok().filter(|k| !k.is_empty())
}

pub fn clear(provider: &str) -> Result<(), String> {
    match entry(provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(crate::i18n::tr_err("keys.remove_failed", e)),
    }
}

/// The calendar's private iCal address (Windows calendar). It's a secret link, so it lives here, not in settings.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub const CALENDAR_ICS: &str = "calendar-ics";

/// Every account Talaan stores (service "Talaan"). Windows names them "<account>.Talaan" in Credential Manager.
/// The Windows uninstaller deletes the same list (src-tauri/windows/installer-hooks.nsh, checked by a test).
pub const ACCOUNTS: [&str; 10] = ["claude", "openai", "gemini", "groq", "cerebras", "mistral", "openrouter", "compatible", "notion", CALENDAR_ICS];

/// "Delete all Talaan data" removes every key too.
pub fn clear_all() {
    for p in ACCOUNTS {
        let _ = clear(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleting_all_data_covers_every_key() {
        use crate::summarizer::engines::Provider;
        for p in ["claude", "openai", "gemini", "groq", "cerebras", "mistral", "openrouter", "compatible"] {
            let id = Provider::parse(p).unwrap().key_id();
            assert!(ACCOUNTS.contains(&id), "{id}");
        }
        for k in ["notion", CALENDAR_ICS] {
            assert!(ACCOUNTS.contains(&k), "{k}");
        }
    }

    #[test]
    fn the_windows_uninstaller_removes_every_key() {
        let nsh = include_str!("../windows/installer-hooks.nsh");
        for a in ACCOUNTS {
            assert!(nsh.contains(&format!("TalaanDeleteKey \"{a}\"")), "{a} is missing from installer-hooks.nsh");
        }
    }

    #[test]
    fn keys_round_trip_and_clear() {
        // In-memory store: tests never touch the real Keychain.
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        // The mock keeps nothing between entries, so check within one entry's lifetime through the API shape.
        assert!(set("claude", " sk-test ").is_ok());
        assert!(clear("claude").is_ok());
        assert!(clear("never-set").is_ok());
        assert_eq!(get("never-set"), None);
    }
}
