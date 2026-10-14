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

impl Storage {
    pub fn settings_get(&self) -> Result<Option<Value>> {
        read_json_opt(&self.root.join(SETTINGS))
    }

    pub fn settings_set(&mut self, value: Value) -> Result<()> {
        if !value.is_object() {
            return Err(StorageError::InvalidId("settings must be an object".into()));
        }
        write_json_atomic(&self.root.join(SETTINGS), &value)?;
        self.policy = policy_of(&value);
        Ok(())
    }

    /// Loads the retention policy from `settings.json` if there is one. Called when storage opens.
    pub(crate) fn load_policy(&mut self) -> Result<()> {
        if let Some(v) = self.settings_get()? {
            self.policy = policy_of(&v);
        }
        Ok(())
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
