//! Folders (`library.json`) and tags (derived from meetings).

use super::io::{read_json_opt, write_json_atomic};
use super::paths::LIBRARY;
use super::{Filter, Result, Storage, StorageError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct FolderDef {
    id: String,
    name: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct LibraryFile {
    schema_version: u32,
    folders: Vec<FolderDef>,
}

/// A folder as the frontend sees it (matches `Folder`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub count: usize,
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "folder".into() } else { out }
}

impl Storage {
    fn read_library(&self) -> Result<Vec<FolderDef>> {
        Ok(read_json_opt::<LibraryFile>(&self.root.join(LIBRARY))?.map(|l| l.folders).unwrap_or_default())
    }

    fn write_library(&self, folders: Vec<FolderDef>) -> Result<()> {
        write_json_atomic(&self.root.join(LIBRARY), &LibraryFile { schema_version: 1, folders })
    }

    pub fn list_folders(&self) -> Result<Vec<Folder>> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for m in self.list_meetings(&Filter::default())? {
            if let Some(f) = m.folder_id {
                *counts.entry(f).or_default() += 1;
            }
        }
        Ok(self.read_library()?.into_iter().map(|f| Folder { count: counts.get(&f.id).copied().unwrap_or(0), id: f.id, name: f.name }).collect())
    }

    pub fn create_folder(&self, name: &str) -> Result<Folder> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StorageError::InvalidId("folder name".into()));
        }
        let mut folders = self.read_library()?;
        let base = slug(name);
        let mut id = base.clone();
        let mut n = 2;
        while folders.iter().any(|f| f.id == id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        folders.push(FolderDef { id: id.clone(), name: name.to_string() });
        self.write_library(folders)?;
        Ok(Folder { id, name: name.to_string(), count: 0 })
    }

    pub fn rename_folder(&self, id: &str, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StorageError::InvalidId("folder name".into()));
        }
        let mut folders = self.read_library()?;
        let f = folders.iter_mut().find(|f| f.id == id).ok_or_else(|| StorageError::NotFound(format!("folder {id}")))?;
        // The id stays: meetings point at it.
        f.name = name.to_string();
        self.write_library(folders)
    }

    /// Removes the folder. Its meetings stay, outside any folder.
    pub fn delete_folder(&self, id: &str) -> Result<()> {
        let mut folders = self.read_library()?;
        let before = folders.len();
        folders.retain(|f| f.id != id);
        if folders.len() == before {
            return Err(StorageError::NotFound(format!("folder {id}")));
        }
        for m in self.list_meetings(&Filter { folder_id: Some(id.to_string()), ..Default::default() })? {
            self.move_to_folder(&m.id, None)?;
        }
        self.write_library(folders)
    }

    pub fn list_tags(&self) -> Result<Vec<String>> {
        let tags: BTreeSet<String> = self.list_meetings(&Filter::default())?.into_iter().flat_map(|m| m.tags).collect();
        Ok(tags.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::tests::{at, storage};
    use super::*;

    #[test]
    fn a_folder_can_be_renamed_and_deleted_without_losing_meetings() {
        let (_d, s) = storage();
        let f = s.create_folder("Clients").unwrap();
        let m = s.create_meeting("zoom", "Zoom", at("2026-10-14T14:30:00-07:00")).unwrap().id;
        s.move_to_folder(&m, Some(&f.id)).unwrap();
        s.rename_folder(&f.id, "  Client calls ").unwrap();
        assert_eq!(s.list_folders().unwrap(), vec![Folder { id: f.id.clone(), name: "Client calls".into(), count: 1 }]);
        assert!(s.rename_folder(&f.id, " ").is_err());
        s.delete_folder(&f.id).unwrap();
        assert!(s.list_folders().unwrap().is_empty());
        assert_eq!(s.get_meeting(&m).unwrap().item.folder_id, None);
        assert!(s.delete_folder(&f.id).is_err());
    }

    #[test]
    fn folders_get_unique_slug_ids() {
        let (_d, s) = storage();
        let a = s.create_folder("Q4 Planning").unwrap();
        let b = s.create_folder("Q4 Planning").unwrap();
        assert_eq!(a.id, "q4-planning");
        assert_eq!(b.id, "q4-planning-2");
        assert_eq!(s.create_folder("???").unwrap().id, "folder");
        assert!(s.create_folder("   ").is_err());
    }

    #[test]
    fn folders_persist_and_count_meetings() {
        let (d, s) = storage();
        let work = s.create_folder("Work").unwrap();
        s.create_folder("Clients").unwrap();
        let m = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap();
        s.move_to_folder(&m.id, Some(&work.id)).unwrap();

        let reopened = Storage::new(d.path().to_path_buf()).unwrap();
        let folders = reopened.list_folders().unwrap();
        assert_eq!(folders.iter().map(|f| (f.id.as_str(), f.count)).collect::<Vec<_>>(), vec![("work", 1), ("clients", 0)]);
    }

    #[test]
    fn tags_are_derived_sorted_and_unique() {
        let (_d, s) = storage();
        let a = s.create_meeting("zoom", "Zoom", at("2026-10-14T09:00:00-07:00")).unwrap();
        let b = s.create_meeting("zoom", "Zoom", at("2026-10-13T09:00:00-07:00")).unwrap();
        s.set_tags(&a.id, &["zoom".into(), "roadmap".into()]).unwrap();
        s.set_tags(&b.id, &["zoom".into(), "1:1".into()]).unwrap();
        assert_eq!(s.list_tags().unwrap(), vec!["1:1", "roadmap", "zoom"]);
    }

    #[test]
    fn no_folders_on_a_fresh_library() {
        let (_d, s) = storage();
        assert!(s.list_folders().unwrap().is_empty());
        assert!(s.list_tags().unwrap().is_empty());
    }
}
