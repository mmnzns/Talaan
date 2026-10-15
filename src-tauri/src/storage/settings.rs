//! `settings.json` holds the frontend's settings as opaque JSON. Rust only looks at the retention values.

use super::io::{read_json_opt, write_json_atomic};
use super::paths::{meetings_dir, SETTINGS};
use super::retention::Policy;
use super::{Result, Storage, StorageError};
use serde_json::Value;
use std::fs;

fn policy_of(v: &Value) -> Policy {
    Policy::from_settings(v["retention"].as_str().unwrap_or(""), v["transcriptRetention"].as_str().unwrap_or(""))
}

/// Where Windows test builds 3f3a9d8..8325bac kept the calendar's secret iCal address, in plain text. It now lives in
/// secure key storage (`keys::CALENDAR_ICS`), so this key is never read back, never written, and removed at startup.
pub const LEGACY_CALENDAR_ICS: &str = "calendarIcsUrl";

fn strip_legacy(v: &mut Value) -> Option<Value> {
    v.as_object_mut()?.remove(LEGACY_CALENDAR_ICS)
}

impl Storage {
    pub fn settings_get(&self) -> Result<Option<Value>> {
        let mut v: Option<Value> = read_json_opt(&self.root.join(SETTINGS))?;
        if let Some(v) = v.as_mut() {
            strip_legacy(v);
        }
        Ok(v)
    }

    pub fn settings_set(&mut self, mut value: Value) -> Result<()> {
        if !value.is_object() {
            return Err(StorageError::InvalidId("settings must be an object".into()));
        }
        strip_legacy(&mut value);
        let name_changed = self.settings_get()?.map(|old| old["yourName"].clone()).unwrap_or(Value::Null) != value["yourName"];
        write_json_atomic(&self.root.join(SETTINGS), &value)?;
        self.policy = policy_of(&value);
        if name_changed {
            // transcript.md and summary.md show Me by the user's name.
            for item in self.list_meetings(&Default::default())? {
                let _ = self.regenerate_markdown(&item.id);
            }
        }
        Ok(())
    }

    /// Loads the retention policy from `settings.json` if there is one. Called when storage opens.
    pub(crate) fn load_policy(&mut self) -> Result<()> {
        if let Some(v) = self.settings_get()? {
            self.policy = policy_of(&v);
        }
        Ok(())
    }

    /// Startup: deletes the plain-text calendar address (`calendarIcsUrl`) from settings.json, if it's there.
    /// `keep_connected` gets the address and says whether the calendar stays connected (Windows moves the address
    /// into secure key storage); when it says no, or the address is empty, `calendarConnected` is turned off.
    pub fn remove_legacy_calendar_address(&mut self, keep_connected: impl FnOnce(&str) -> bool) -> Result<()> {
        let path = self.root.join(SETTINGS);
        let Some(mut v) = read_json_opt::<Value>(&path)? else { return Ok(()) };
        let Some(old) = strip_legacy(&mut v) else { return Ok(()) };
        let address = old.as_str().map(str::trim).unwrap_or("");
        if address.is_empty() || !keep_connected(address) {
            v["calendarConnected"] = Value::Bool(false);
        }
        write_json_atomic(&path, &v)
    }

    /// "Delete all Talaan data": every meeting, the library, the settings and downloaded models (all under the data
    /// folder). Keys arrive in M5. Models reused from another app are never touched.
    pub fn delete_all_data(&mut self) -> Result<()> {
        for entry in fs::read_dir(&self.root)? {
            let path = entry?.path();
            if path.is_dir() {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }
        fs::create_dir_all(meetings_dir(&self.root))?;
        self.policy = Policy::default();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::retention::{AudioPolicy, TranscriptPolicy};
    use super::super::store::tests::{at, storage};
    use super::*;
    use serde_json::json;

    #[test]
    fn settings_round_trip() {
        let (_d, mut s) = storage();
        assert!(s.settings_get().unwrap().is_none());
        let v = json!({"theme": "dark", "retention": "30d", "extra": {"x": [1, 2]}});
        s.settings_set(v.clone()).unwrap();
        assert_eq!(s.settings_get().unwrap().unwrap(), v);
    }

    #[test]
    fn setting_retention_updates_the_policy_immediately() {
        let (_d, mut s) = storage();
        s.settings_set(json!({"retention": "none", "transcriptRetention": "90d"})).unwrap();
        assert_eq!(s.policy().audio, AudioPolicy::Never);
        assert_eq!(s.policy().transcripts, TranscriptPolicy::Days(90));
    }

    #[test]
    fn policy_is_restored_when_storage_reopens() {
        let (d, mut s) = storage();
        s.settings_set(json!({"retention": "30d"})).unwrap();
        let mut reopened = Storage::new(d.path().to_path_buf()).unwrap();
        reopened.load_policy().unwrap();
        assert_eq!(reopened.policy().audio, AudioPolicy::Days(30));
    }

    #[test]
    fn a_new_name_rewrites_the_markdown_copies() {
        let (d, mut s) = storage();
        let m = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap();
        s.save_transcript(&m.id, &[crate::storage::store::tests::line("l0", 0, "Hello")]).unwrap();
        let md = || fs::read_to_string(d.path().join("meetings").join(&m.id).join("transcript.md")).unwrap();
        assert!(md().contains("Me:** Hello"));
        s.settings_set(json!({"yourName": "Miguel"})).unwrap();
        assert!(md().contains("Miguel:** Hello"));
    }

    fn raw(d: &tempfile::TempDir) -> Value {
        serde_json::from_str(&fs::read_to_string(d.path().join(SETTINGS)).unwrap()).unwrap()
    }

    #[test]
    fn the_legacy_calendar_address_is_never_read_or_written() {
        let (d, mut s) = storage();
        fs::write(d.path().join(SETTINGS), r#"{"theme":"dark","calendarIcsUrl":"https://secret/x.ics"}"#).unwrap();
        assert_eq!(s.settings_get().unwrap().unwrap(), json!({"theme": "dark"}));
        s.settings_set(json!({"theme": "light", "calendarIcsUrl": "https://secret/x.ics"})).unwrap();
        assert_eq!(raw(&d), json!({"theme": "light"}));
    }

    #[test]
    fn startup_moves_the_legacy_address_and_keeps_the_calendar_connected() {
        let (d, mut s) = storage();
        fs::write(d.path().join(SETTINGS), r#"{"calendarConnected":true,"calendarIcsUrl":" https://secret/x.ics "}"#).unwrap();
        let mut moved = None;
        s.remove_legacy_calendar_address(|a| {
            moved = Some(a.to_string());
            true
        })
        .unwrap();
        assert_eq!(moved.as_deref(), Some("https://secret/x.ics"));
        assert_eq!(raw(&d), json!({"calendarConnected": true}));
    }

    #[test]
    fn startup_turns_the_calendar_off_when_the_address_cant_be_kept() {
        for (file, keep) in [
            (r#"{"calendarConnected":true,"calendarIcsUrl":"https://secret/x.ics"}"#, false),
            (r#"{"calendarConnected":true,"calendarIcsUrl":""}"#, true),
        ] {
            let (d, mut s) = storage();
            fs::write(d.path().join(SETTINGS), file).unwrap();
            s.remove_legacy_calendar_address(|_| keep).unwrap();
            assert_eq!(raw(&d), json!({"calendarConnected": false}), "{file}");
        }
    }

    #[test]
    fn startup_leaves_clean_settings_untouched() {
        let (d, mut s) = storage();
        s.remove_legacy_calendar_address(|_| panic!("no settings file")).unwrap();
        assert!(!d.path().join(SETTINGS).exists());
        fs::write(d.path().join(SETTINGS), r#"{"calendarConnected":true}"#).unwrap();
        s.remove_legacy_calendar_address(|_| panic!("no legacy address")).unwrap();
        assert_eq!(fs::read_to_string(d.path().join(SETTINGS)).unwrap(), r#"{"calendarConnected":true}"#);
    }

    #[test]
    fn non_objects_are_rejected() {
        let (_d, mut s) = storage();
        assert!(s.settings_set(json!([1, 2])).is_err());
        assert!(s.settings_get().unwrap().is_none());
    }

    #[test]
    fn delete_all_removes_everything_and_storage_still_works() {
        let (d, mut s) = storage();
        let m = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap();
        s.create_folder("Work").unwrap();
        s.settings_set(json!({"retention": "none"})).unwrap();
        fs::write(d.path().join("meetings").join(&m.id).join("me.wav"), b"x").unwrap();

        s.delete_all_data().unwrap();
        assert!(s.list_meetings(&Default::default()).unwrap().is_empty());
        assert!(s.list_folders().unwrap().is_empty());
        assert!(s.settings_get().unwrap().is_none());
        assert_eq!(s.policy().audio, AudioPolicy::Days(7));
        let names: Vec<_> = fs::read_dir(d.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        assert_eq!(names, vec!["meetings"]);
        s.create_meeting("zoom", "Zoom", at("2026-10-15T09:00:00-07:00")).unwrap();
    }
}
