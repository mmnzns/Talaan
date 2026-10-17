//! "Also save each meeting to a folder": one Markdown file per meeting in a folder the user picked (an Obsidian
//! vault, Documents, a synced folder). Pure file work, no Tauri types; `commands/autosave.rs` wires it up.
//!
//! The only file Talaan ever replaces or removes there is the one it wrote for that meeting (its path is kept in
//! meeting.json as `exportedPath`). A file with the same name that Talaan didn't write is never touched: the new
//! one gets " (2)" after its name instead.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// What happened to a meeting's copy.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Written (or rewritten) here.
    Written(PathBuf),
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
/// (from meeting.json): it is rewritten in place, or renamed when the name changed. Other files are never replaced.
pub fn write(folder: &Path, name: &str, previous: Option<&Path>, contents: &str) -> io::Result<Outcome> {
    if !folder.is_dir() {
        return Ok(Outcome::FolderMissing);
    }
    let target = folder.join(name);
    // Only a previous copy that is still in this folder is ours to replace (the folder may have changed since).
    let previous = previous.filter(|p| p.parent() == Some(folder) && p.is_file());
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
        // The title changed: the old copy (ours) goes, so the folder keeps one file per meeting.
        let _ = fs::remove_file(p);
    }
    Ok(Outcome::Written(target))
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(write(&gone, "a.md", None, "x").unwrap(), Outcome::FolderMissing);
        assert!(!gone.exists());
    }

    #[test]
    fn writes_then_rewrites_the_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let Outcome::Written(first) = write(dir.path(), "2026-10-14 Sync.md", None, "one").unwrap() else { panic!() };
        let Outcome::Written(again) = write(dir.path(), "2026-10-14 Sync.md", Some(&first), "two").unwrap() else { panic!() };
        assert_eq!(first, again);
        assert_eq!(read(&again), "two");
        assert_eq!(names(dir.path()), vec!["2026-10-14 Sync.md"], "no temp files left behind");
    }

    #[test]
    fn a_new_title_renames_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let Outcome::Written(old) = write(dir.path(), "2026-10-14 Call, Zoom.md", None, "one").unwrap() else { panic!() };
        let Outcome::Written(new) = write(dir.path(), "2026-10-14 Q4 Roadmap Sync.md", Some(&old), "two").unwrap() else { panic!() };
        assert_eq!(names(dir.path()), vec!["2026-10-14 Q4 Roadmap Sync.md"]);
        assert_eq!(read(&new), "two");
    }

    #[test]
    fn a_title_that_only_changes_case_keeps_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let Outcome::Written(old) = write(dir.path(), "2026-10-14 sync.md", None, "one").unwrap() else { panic!() };
        let Outcome::Written(new) = write(dir.path(), "2026-10-14 Sync.md", Some(&old), "two").unwrap() else { panic!() };
        assert_eq!(names(dir.path()), vec!["2026-10-14 Sync.md"]);
        assert_eq!(read(&new), "two");
    }

    #[test]
    fn files_talaan_did_not_write_are_never_replaced() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("2026-10-14 Sync.md"), "my own notes").unwrap();
        let Outcome::Written(ours) = write(dir.path(), "2026-10-14 Sync.md", None, "talaan").unwrap() else { panic!() };
        assert!(ours.ends_with("2026-10-14 Sync (2).md"));
        assert_eq!(read(&dir.path().join("2026-10-14 Sync.md")), "my own notes");
        // Renaming our copy onto the user's name again still leaves theirs alone.
        let Outcome::Written(back) = write(dir.path(), "2026-10-14 Sync.md", Some(&ours), "talaan 2").unwrap() else { panic!() };
        assert_eq!(back, ours, "our own copy keeps its name");
        assert_eq!(read(&dir.path().join("2026-10-14 Sync.md")), "my own notes");
    }

    #[test]
    fn a_previous_copy_in_another_folder_is_left_alone() {
        let old_dir = tempfile::tempdir().unwrap();
        let new_dir = tempfile::tempdir().unwrap();
        let Outcome::Written(old) = write(old_dir.path(), "a.md", None, "one").unwrap() else { panic!() };
        let Outcome::Written(new) = write(new_dir.path(), "b.md", Some(&old), "two").unwrap() else { panic!() };
        assert!(old.is_file(), "the old folder is no longer Talaan's to tidy");
        assert_eq!(read(&new), "two");
    }

    #[test]
    fn a_deleted_previous_copy_is_written_again() {
        let dir = tempfile::tempdir().unwrap();
        let Outcome::Written(old) = write(dir.path(), "a.md", None, "one").unwrap() else { panic!() };
        fs::remove_file(&old).unwrap();
        let Outcome::Written(again) = write(dir.path(), "a.md", Some(&old), "two").unwrap() else { panic!() };
        assert_eq!(read(&again), "two");
    }
}
