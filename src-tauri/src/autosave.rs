//! "Also save each meeting to a folder": one Markdown file per meeting in a folder the user picked (an Obsidian
//! vault, Documents, a synced folder). Pure file work, no Tauri types; `commands/autosave.rs` wires it up.
//!
//! The only file Talaan ever replaces or removes there is the one it wrote for that meeting, and only while it is
//! exactly as Talaan wrote it: its path, SHA-256 and size are kept in meeting.json (`exportedCopy`). A copy the
//! person has edited (in Obsidian, say) is left alone and no longer managed; the next copy is a new file beside it.
//! A file with the same name that Talaan didn't write is never touched either: the new one gets " (2)" instead.
//! When Talaan does retire its own unchanged copy (the title changed), it goes to the Recycle Bin or Trash.

use crate::storage::model::{ExportedCopy, MeetingStatus, PendingCopy};
use crate::storage::Storage;
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A copy as Talaan wrote it: where, and what it held.
#[derive(Debug, Clone, PartialEq)]
pub struct Copy {
    pub path: PathBuf,
    /// SHA-256 of the bytes written, lowercase hex.
    pub sha256: String,
    pub size: u64,
}

impl Copy {
    fn of(path: PathBuf, contents: &str) -> Copy {
        Copy { path, sha256: sha256_hex(contents.as_bytes()), size: contents.len() as u64 }
    }

    /// Still there and exactly as Talaan wrote it (size first, so a big edited file isn't read for nothing).
    fn untouched(&self) -> bool {
        fs::metadata(&self.path).is_ok_and(|m| m.is_file() && m.len() == self.size) && fs::read(&self.path).is_ok_and(|b| sha256_hex(&b) == self.sha256)
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// What happened to a meeting's copy.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Written (or rewritten) here.
    Written(Copy),
    /// The folder isn't there right now (a disconnected drive, a deleted folder): nothing was written.
    FolderMissing,
}

/// Longest title kept in a file name, in characters, so paths stay well inside Windows' limits.
const MAX_TITLE: usize = 120;

/// "2026-10-14 Q4 Roadmap Sync.md": the meeting's local date (from its RFC 3339 start) and its title, without the
/// characters Windows and macOS don't allow in file names.
pub fn file_name(started_at: &str, title: &str) -> String {
    let date = started_at.get(..10).filter(|d| d.bytes().enumerate().all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() }));
    let clean: String = title.chars().map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { ' ' } else { c }).collect();
    let mut clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() > MAX_TITLE {
        clean = clean.chars().take(MAX_TITLE).collect::<String>().trim_end().to_string();
    }
    // Windows drops trailing dots and spaces from names, which would make two names collide.
    let clean = clean.trim_end_matches(['.', ' ']);
    let title = if clean.is_empty() { "Meeting" } else { clean };
    match date {
        Some(d) => format!("{d} {title}.md"),
        None => format!("{title}.md"),
    }
}

fn same_name(a: &Path, b: &Path) -> bool {
    match (a.file_name().and_then(|n| n.to_str()), b.file_name().and_then(|n| n.to_str())) {
        (Some(x), Some(y)) => x.to_lowercase() == y.to_lowercase(),
        _ => false,
    }
}

/// Writes a meeting's copy into `folder` as `name`. `previous` is the copy Talaan wrote for this meeting before
/// (from meeting.json): rewritten in place, or renamed when the name changed, but only while nobody has changed it.
/// `trash` retires Talaan's own old copy after a rename (`platform::trash` in the app). Other files are never touched.
pub fn write(folder: &Path, name: &str, previous: Option<&Copy>, contents: &str, trash: &dyn Fn(&Path) -> io::Result<()>) -> io::Result<Outcome> {
    if !folder.is_dir() {
        return Ok(Outcome::FolderMissing);
    }
    let target = folder.join(name);
    // Only a previous copy that is still in this folder, unchanged, is ours to replace (the folder may have changed
    // since, and an edited copy belongs to the person now).
    let previous = previous.filter(|c| c.path.parent() == Some(folder) && c.untouched()).map(|c| c.path.as_path());
    let target = match previous {
        // Same name, or the same name in another case (macOS and Windows treat those as one file): move it to the
        // new spelling, then rewrite it.
        Some(p) if same_name(p, &target) => {
            if p != target {
                fs::rename(p, &target)?;
            }
            target
        }
        _ => free_name(folder, name, previous),
    };
    write_atomic(&target, contents)?;
    if let Some(p) = previous.filter(|p| !same_name(p, &target)) {
        // The title changed: the old copy (ours, unchanged) goes to the bin, so the folder keeps one file per meeting.
        if let Err(e) = trash(p) {
            eprintln!("talaan: couldn't move the old copy to the bin, left it: {e}");
        }
    }
    Ok(Outcome::Written(Copy::of(target, contents)))
}

/// `name`, or "name (2).md", "name (3).md"... when a file Talaan didn't write already has it (`mine` is ours).
fn free_name(folder: &Path, name: &str, mine: Option<&Path>) -> PathBuf {
    let free = |p: &PathBuf| !p.exists() || mine == Some(p.as_path());
    let first = folder.join(name);
    if free(&first) {
        return first;
    }
    let stem = name.strip_suffix(".md").unwrap_or(name);
    (2..)
        .map(|n| folder.join(format!("{stem} ({n}).md")))
        .find(free)
        .expect("some name is free")
}

/// Temp file beside the target, then rename, so a sync app or Obsidian never sees half a file.
fn write_atomic(target: &Path, contents: &str) -> io::Result<()> {
    let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("meeting.md");
    let tmp = target.with_file_name(format!(".{name}.talaan-tmp"));
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, target).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

// ---- Bookkeeping in meeting.json (the commands hold the storage lock only around these, never during file work) ----

const WAITING: &str = "waiting";
const MISSING: &str = "missing";
const FAILED: &str = "failed";

/// A copy asked for: `finished` when the summary (or transcript-only result) was just saved, otherwise a change to a
/// meeting that has a copy (or one still waiting). Marks it pending, so it survives a quit before it's written.
/// Returns whether a copy is due.
pub fn mark_due(s: &Storage, id: &str, finished: bool) -> crate::storage::Result<bool> {
    s.update_meeting_file(id, |f| {
        let due = f.status == MeetingStatus::Ready && (finished || f.exported_copy.is_some() || f.autosave_pending.is_some());
        if due && f.autosave_pending.is_none() {
            f.autosave_pending = Some(PendingCopy { kind: WAITING.into(), error: None });
        }
        due
    })
}

/// What to write for a meeting, or `None` when nothing is due (not finished yet, or never saved and not asked for).
pub struct Plan {
    pub name: String,
    pub previous: Option<Copy>,
}

pub fn plan(s: &Storage, id: &str, finished: bool) -> crate::storage::Result<Option<Plan>> {
    let f = s.read_meeting_file(id)?;
    if f.status != MeetingStatus::Ready || !(finished || f.exported_copy.is_some() || f.autosave_pending.is_some()) {
        return Ok(None);
    }
    let previous = f.exported_copy.map(|c| Copy { path: PathBuf::from(c.path), sha256: c.sha256, size: c.size });
    Ok(Some(Plan { name: file_name(&f.started_at, &f.title), previous }))
}

/// Saves what happened: the new copy (and nothing pending), or why it's still pending. Answers "written", "missing"
/// or "failed".
pub fn record(s: &Storage, id: &str, result: &io::Result<Outcome>) -> crate::storage::Result<&'static str> {
    s.update_meeting_file(id, |f| match result {
        Ok(Outcome::Written(c)) => {
            f.exported_copy = Some(ExportedCopy { path: c.path.to_string_lossy().into_owned(), sha256: c.sha256.clone(), size: c.size });
            f.autosave_pending = None;
            "written"
        }
        Ok(Outcome::FolderMissing) => {
            f.autosave_pending = Some(PendingCopy { kind: MISSING.into(), error: None });
            MISSING
        }
        Err(e) => {
            f.autosave_pending = Some(PendingCopy { kind: FAILED.into(), error: Some(e.to_string()) });
            FAILED
        }
    })
}

/// Every meeting whose copy is still to be written, with why.
pub fn pending(s: &Storage) -> crate::storage::Result<Vec<(String, PendingCopy)>> {
    let mut out = vec![];
    for item in s.list_meetings(&Default::default())? {
        if let Ok(Some(p)) = s.read_meeting_file(&item.id).map(|f| f.autosave_pending) {
            out.push((item.id, p));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests' bin: a plain delete (the real one is the Recycle Bin or Trash).
    fn bin(p: &Path) -> io::Result<()> {
        fs::remove_file(p)
    }

    fn read(p: &Path) -> String {
        fs::read_to_string(p).unwrap()
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    #[test]
    fn file_names_use_the_local_date_and_a_safe_title() {
        assert_eq!(file_name("2026-10-14T14:30:00-04:00", "Q4 Roadmap Sync"), "2026-10-14 Q4 Roadmap Sync.md");
        assert_eq!(file_name("2026-10-14T14:30:00-04:00", "Ana / Sam: 1:1?"), "2026-10-14 Ana Sam 1 1.md");
        assert_eq!(file_name("2026-10-14T14:30:00-04:00", "  \"*<>|  "), "2026-10-14 Meeting.md");
        assert_eq!(file_name("2026-10-14T14:30:00-04:00", "Ends with dots..."), "2026-10-14 Ends with dots.md");
        assert_eq!(file_name("garbage", "Sync"), "Sync.md");
        let long = "x".repeat(300);
        assert_eq!(file_name("2026-10-14", &long).chars().count(), "2026-10-14 ".len() + MAX_TITLE + ".md".len());
    }

    #[test]
    fn a_missing_folder_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("unplugged drive");
        assert_eq!(write(&gone, "a.md", None, "x", &bin).unwrap(), Outcome::FolderMissing);
        assert!(!gone.exists());
    }

    #[test]
    fn writes_then_rewrites_the_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let Outcome::Written(first) = write(dir.path(), "2026-10-14 Sync.md", None, "one", &bin).unwrap() else { panic!() };
        let Outcome::Written(again) = write(dir.path(), "2026-10-14 Sync.md", Some(&first), "two", &bin).unwrap() else { panic!() };
        assert_eq!(first.path, again.path);
        assert_eq!(read(&again.path), "two");
        assert_eq!(names(dir.path()), vec!["2026-10-14 Sync.md"], "no temp files left behind");
    }

    #[test]
    fn a_new_title_renames_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let Outcome::Written(old) = write(dir.path(), "2026-10-14 Call, Zoom.md", None, "one", &bin).unwrap() else { panic!() };
        let Outcome::Written(new) = write(dir.path(), "2026-10-14 Q4 Roadmap Sync.md", Some(&old), "two", &bin).unwrap() else { panic!() };
        assert_eq!(names(dir.path()), vec!["2026-10-14 Q4 Roadmap Sync.md"]);
        assert_eq!(read(&new.path), "two");
    }

    #[test]
    fn a_title_that_only_changes_case_keeps_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let Outcome::Written(old) = write(dir.path(), "2026-10-14 sync.md", None, "one", &bin).unwrap() else { panic!() };
        let Outcome::Written(new) = write(dir.path(), "2026-10-14 Sync.md", Some(&old), "two", &bin).unwrap() else { panic!() };
        assert_eq!(names(dir.path()), vec!["2026-10-14 Sync.md"]);
        assert_eq!(read(&new.path), "two");
    }

    #[test]
    fn files_talaan_did_not_write_are_never_replaced() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("2026-10-14 Sync.md"), "my own notes").unwrap();
        let Outcome::Written(ours) = write(dir.path(), "2026-10-14 Sync.md", None, "talaan", &bin).unwrap() else { panic!() };
        assert!(ours.path.ends_with("2026-10-14 Sync (2).md"));
        assert_eq!(read(&dir.path().join("2026-10-14 Sync.md")), "my own notes");
        // Renaming our copy onto the user's name again still leaves theirs alone.
        let Outcome::Written(back) = write(dir.path(), "2026-10-14 Sync.md", Some(&ours), "talaan 2", &bin).unwrap() else { panic!() };
        assert_eq!(back.path, ours.path, "our own copy keeps its name");
        assert_eq!(read(&dir.path().join("2026-10-14 Sync.md")), "my own notes");
    }

    #[test]
    fn a_previous_copy_in_another_folder_is_left_alone() {
        let old_dir = tempfile::tempdir().unwrap();
        let new_dir = tempfile::tempdir().unwrap();
        let Outcome::Written(old) = write(old_dir.path(), "a.md", None, "one", &bin).unwrap() else { panic!() };
        let Outcome::Written(new) = write(new_dir.path(), "b.md", Some(&old), "two", &bin).unwrap() else { panic!() };
        assert!(old.path.is_file(), "the old folder is no longer Talaan's to tidy");
        assert_eq!(read(&new.path), "two");
    }

    #[test]
    fn a_deleted_previous_copy_is_written_again() {
        let dir = tempfile::tempdir().unwrap();
        let Outcome::Written(old) = write(dir.path(), "a.md", None, "one", &bin).unwrap() else { panic!() };
        fs::remove_file(&old.path).unwrap();
        let Outcome::Written(again) = write(dir.path(), "a.md", Some(&old), "two", &bin).unwrap() else { panic!() };
        assert_eq!(read(&again.path), "two");
    }

    #[test]
    fn a_copy_the_person_edited_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let Outcome::Written(first) = write(dir.path(), "2026-10-14 Sync.md", None, "one", &bin).unwrap() else { panic!() };
        fs::write(&first.path, "one, plus my notes").unwrap();
        let Outcome::Written(next) = write(dir.path(), "2026-10-14 Sync.md", Some(&first), "two", &bin).unwrap() else { panic!() };
        assert_eq!(read(&first.path), "one, plus my notes");
        assert!(next.path.ends_with("2026-10-14 Sync (2).md"), "a new copy beside it: {:?}", next.path);
        assert_eq!(read(&next.path), "two");
    }

    #[test]
    fn an_edited_copy_is_not_removed_when_the_title_changes() {
        let dir = tempfile::tempdir().unwrap();
        let Outcome::Written(old) = write(dir.path(), "2026-10-14 Call.md", None, "one", &bin).unwrap() else { panic!() };
        fs::write(&old.path, "edited").unwrap();
        let never = |_: &Path| -> io::Result<()> { panic!("an edited copy must not go to the bin") };
        let Outcome::Written(new) = write(dir.path(), "2026-10-14 Roadmap.md", Some(&old), "two", &never).unwrap() else { panic!() };
        assert_eq!(names(dir.path()), vec!["2026-10-14 Call.md", "2026-10-14 Roadmap.md"]);
        assert_eq!(read(&old.path), "edited");
        assert_eq!(read(&new.path), "two");
    }

    #[test]
    fn a_renamed_unchanged_copy_goes_to_the_bin_and_the_hash_is_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let Outcome::Written(old) = write(dir.path(), "a.md", None, "one", &bin).unwrap() else { panic!() };
        assert_eq!((old.size, old.sha256.as_str()), (3, "7692c3ad3540bb803c020b3aee66cd8887123234ea0c6e7143c0add73ff431ed"));
        let binned = std::cell::RefCell::new(Vec::new());
        let to_bin = |p: &Path| {
            binned.borrow_mut().push(p.to_path_buf());
            fs::remove_file(p)
        };
        write(dir.path(), "b.md", Some(&old), "two", &to_bin).unwrap();
        assert_eq!(*binned.borrow(), vec![old.path.clone()]);
    }

    #[test]
    fn a_bin_that_fails_leaves_the_old_copy() {
        let dir = tempfile::tempdir().unwrap();
        let Outcome::Written(old) = write(dir.path(), "a.md", None, "one", &bin).unwrap() else { panic!() };
        let broken = |_: &Path| Err(io::Error::other("no bin here"));
        write(dir.path(), "b.md", Some(&old), "two", &broken).unwrap();
        assert_eq!(names(dir.path()), vec!["a.md", "b.md"]);
    }

    mod bookkeeping {
        use super::super::*;
        use crate::storage::store::tests::{at, storage};

        fn ready(s: &Storage) -> String {
            let id = s.create_meeting("zoom", "Zoom", at("2026-10-14T14:30:00-07:00")).unwrap().id;
            s.update_meeting_file(&id, |f| f.status = MeetingStatus::Ready).unwrap();
            id
        }

        #[test]
        fn a_finished_meeting_stays_pending_until_its_copy_is_written() {
            let (_d, s) = storage();
            let id = ready(&s);
            assert!(mark_due(&s, &id, true).unwrap());
            assert_eq!(pending(&s).unwrap()[0].1.kind, "waiting", "survives a quit before it's written");
            let dir = tempfile::tempdir().unwrap();
            let plan = plan(&s, &id, false).unwrap().expect("pending copies are due even for a change");
            let result = write(dir.path(), &plan.name, plan.previous.as_ref(), "one", &|_| Ok(()));
            assert_eq!(record(&s, &id, &result).unwrap(), "written");
            assert!(pending(&s).unwrap().is_empty());
            assert!(s.read_meeting_file(&id).unwrap().exported_copy.is_some());
        }

        #[test]
        fn a_change_to_a_meeting_without_a_copy_is_not_due() {
            let (_d, s) = storage();
            let id = ready(&s);
            assert!(!mark_due(&s, &id, false).unwrap());
            assert!(plan(&s, &id, false).unwrap().is_none());
            assert!(pending(&s).unwrap().is_empty());
        }

        #[test]
        fn failures_are_kept_with_their_reason() {
            let (_d, s) = storage();
            let id = ready(&s);
            mark_due(&s, &id, true).unwrap();
            assert_eq!(record(&s, &id, &Ok(Outcome::FolderMissing)).unwrap(), "missing");
            assert_eq!(pending(&s).unwrap()[0].1, PendingCopy { kind: "missing".into(), error: None });
            assert_eq!(record(&s, &id, &Err(io::Error::other("disk full"))).unwrap(), "failed");
            assert_eq!(pending(&s).unwrap()[0].1, PendingCopy { kind: "failed".into(), error: Some("disk full".into()) });
        }

        #[test]
        fn meetings_still_processing_are_not_due() {
            let (_d, s) = storage();
            let id = s.create_meeting("zoom", "Zoom", at("2026-10-14T14:30:00-07:00")).unwrap().id;
            assert!(!mark_due(&s, &id, true).unwrap());
            assert!(plan(&s, &id, true).unwrap().is_none());
        }
    }
}
