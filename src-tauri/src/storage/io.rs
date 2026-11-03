//! Atomic file writes: write a temp file in the same folder, flush it, then rename over the target.

use super::StorageError;
use serde::{de::DeserializeOwned, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

/// Serializes first, so a value that fails to serialize never touches the existing file.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), StorageError> {
    let bytes = serde_json::to_vec_pretty(value)?;
    write_bytes_atomic(path, &bytes)
}

pub fn write_text_atomic(path: &Path, text: &str) -> Result<(), StorageError> {
    write_bytes_atomic(path, text.as_bytes())
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, StorageError> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn read_json_opt<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, StorageError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Total size in bytes of every file under `path`.
pub fn dir_size(path: &Path) -> u64 {
    dir_size_excluding(path, &[])
}

/// `dir_size` without the folders in `skip` (whole paths).
pub fn dir_size_excluding(path: &Path, skip: &[PathBuf]) -> u64 {
    let Ok(entries) = fs::read_dir(path) else { return 0 };
    entries
        .flatten()
        .filter(|e| !skip.contains(&e.path()))
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size_excluding(&e.path(), skip),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

pub fn remove_if_exists(path: &Path) -> Result<(), StorageError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::ser::Error as _;
    use serde_json::json;

    struct Unserializable;
    impl Serialize for Unserializable {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(S::Error::custom("nope"))
        }
    }

    #[test]
    fn writes_and_overwrites_without_leaving_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        write_json_atomic(&path, &json!({"v": 1})).unwrap();
        write_json_atomic(&path, &json!({"v": 2})).unwrap();
        let back: serde_json::Value = read_json(&path).unwrap();
        assert_eq!(back["v"], 2);
        let names: Vec<_> = fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, vec![std::ffi::OsString::from("a.json")]);
    }

    #[test]
    fn a_failed_serialization_keeps_the_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        write_json_atomic(&path, &json!({"v": 1})).unwrap();
        assert!(write_json_atomic(&path, &Unserializable).is_err());
        let back: serde_json::Value = read_json(&path).unwrap();
        assert_eq!(back["v"], 1);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn read_json_opt_treats_missing_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let v: Option<serde_json::Value> = read_json_opt(&dir.path().join("nope.json")).unwrap();
        assert!(v.is_none());
    }

    #[test]
    fn dir_size_adds_up_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("a")).unwrap();
        fs::write(dir.path().join("a").join("x"), [0u8; 100]).unwrap();
        fs::write(dir.path().join("y"), [0u8; 20]).unwrap();
        assert_eq!(dir_size(dir.path()), 120);
        assert_eq!(dir_size(&dir.path().join("missing")), 0);
    }

    #[test]
    fn remove_if_exists_is_quiet_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        remove_if_exists(&dir.path().join("nope")).unwrap();
    }
}
