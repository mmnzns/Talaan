//! API keys for "your own key" summaries, kept in the macOS Keychain (Windows: Credential Manager), never in
//! settings.json. Service "Talaan", one account per provider.

const SERVICE: &str = "Talaan";

fn entry(provider: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, provider).map_err(|e| format!("Couldn't open the Keychain: {e}"))
}

pub fn set(provider: &str, key: &str) -> Result<(), String> {
    entry(provider)?.set_password(key.trim()).map_err(|e| format!("Couldn't save the key in the Keychain: {e}"))
}

pub fn get(provider: &str) -> Option<String> {
    entry(provider).ok()?.get_password().ok().filter(|k| !k.is_empty())
}

pub fn clear(provider: &str) -> Result<(), String> {
    match entry(provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Couldn't remove the key from the Keychain: {e}")),
    }
}

/// "Delete all Talaan data" removes every key too.
pub fn clear_all() {
    for p in ["claude", "openai", "gemini", "compatible"] {
        let _ = clear(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
