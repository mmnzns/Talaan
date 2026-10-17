//! Moving Talaan's models folder (Windows, v1.1): from `%APPDATA%\Talaan\models` (Roaming, copied at every
//! sign-in on a roaming profile) to `%LOCALAPPDATA%\com.craftconceptsdigital.talaan\models`. Runs once per launch
//! in the background until nothing is left behind; `ModelStore` reads the old folder until then.
//!
//! Crash-safe and never loses a model: the whole folder is renamed when possible (same volume, nothing open);
//! otherwise each model folder is renamed, or copied file by file (to `<name>.talaan-move`, then renamed into
//! place), and an old file is deleted only when the new folder holds a file of the same name and size. Nothing in
//! the new folder is ever overwritten (a download may have put a newer copy there). What can't be moved now (a
//! file in use) stays and is tried again at the next launch. Pure std, tested with temp folders.
#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

use std::fs;
use std::io;
use std::path::Path;

/// Suffix of a file still being copied; never a model file name, so `complete()` ignores it.
const PARTIAL: &str = ".talaan-move";

/// What one pass did.
#[derive(Debug, Default, PartialEq)]
pub struct Moved {
    /// The whole folder was renamed in one step.
    pub renamed: bool,
    /// Files copied across.
    pub copied: usize,
    /// Files or folders left in the old place (in use, or a copy failed); tried again next time.
    pub left: usize,
}

/// Moves everything in `old` into `new` (see the module doc). `old` missing means nothing to do.
pub fn move_models(old: &Path, new: &Path) -> io::Result<Moved> {
    let mut report = Moved::default();
    if !old.is_dir() {
        return Ok(report);
    }
    if let Some(parent) = new.parent() {
        fs::create_dir_all(parent)?;
    }
    if !new.exists() && fs::rename(old, new).is_ok() {
        report.renamed = true;
        return Ok(report);
    }
    fs::create_dir_all(new)?;
    merge(old, new, &mut report);
    // Gone only when empty: anything left is tried again at the next launch.
    let _ = fs::remove_dir(old);
    Ok(report)
}

/// Moves each entry of `from` into `to`, recursively, never overwriting.
fn merge(from: &Path, to: &Path, report: &mut Moved) {
    let Ok(entries) = fs::read_dir(from) else {
        report.left += 1;
        return;
    };
    for entry in entries.flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            if !dst.exists() && fs::rename(&src, &dst).is_ok() {
                continue;
            }
            if fs::create_dir_all(&dst).is_err() {
                report.left += 1;
                continue;
            }
            merge(&src, &dst, report);
            let _ = fs::remove_dir(&src);
        } else if move_file(&src, &dst, report).is_err() {
            report.left += 1;
        }
    }
}

/// One file: rename, else copy then rename into place; the old copy goes only once the new one is there in full.
fn move_file(src: &Path, dst: &Path, report: &mut Moved) -> io::Result<()> {
    let size = fs::metadata(src)?.len();
    let mut tmp = dst.as_os_str().to_owned();
    tmp.push(PARTIAL);
    let tmp = std::path::PathBuf::from(tmp);
    let _ = fs::remove_file(&tmp); // left by a pass that was cut short
    if !dst.exists() && fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    if !dst.exists() {
        // A plain read, so a file another program has open for reading can still be copied.
        {
            let mut from = fs::File::open(src)?;
            let mut to = fs::File::create(&tmp)?;
            io::copy(&mut from, &mut to)?;
            to.sync_all()?;
        }
        if fs::metadata(&tmp)?.len() != size {
            let _ = fs::remove_file(&tmp);
            return Err(io::Error::other("short copy"));
        }
        fs::rename(&tmp, dst)?;
        report.copied += 1;
    }
    // Same name and size in the new folder (just copied, or already there): the old one isn't needed.
    if fs::metadata(dst)?.len() == size {
        fs::remove_file(src)?;
        Ok(())
    } else {
        Err(io::Error::other("a different file is already in the new folder"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, bytes: usize) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![7u8; bytes]).unwrap();
    }

    fn size(p: &Path) -> Option<u64> {
        fs::metadata(p).ok().map(|m| m.len())
    }

    #[test]
    fn nothing_to_move_is_fine() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(move_models(&d.path().join("old"), &d.path().join("new")).unwrap(), Moved::default());
        assert!(!d.path().join("new").exists());
    }

    #[test]
    fn the_whole_folder_is_renamed_when_it_can_be() {
        let d = tempfile::tempdir().unwrap();
        let (old, new) = (d.path().join("Roaming/Talaan/models"), d.path().join("Local/com.x/models"));
        write(&old.join("parakeet-v3/encoder.onnx"), 1000);
        write(&old.join("silero-vad/vad.onnx"), 10);
        let r = move_models(&old, &new).unwrap();
        assert!(r.renamed);
        assert!(!old.exists());
        assert_eq!(size(&new.join("parakeet-v3/encoder.onnx")), Some(1000));
        assert_eq!(size(&new.join("silero-vad/vad.onnx")), Some(10));
    }

    #[test]
    fn into_an_existing_folder_models_are_merged_and_nothing_is_overwritten() {
        let d = tempfile::tempdir().unwrap();
        let (old, new) = (d.path().join("old"), d.path().join("new"));
        write(&old.join("whisper-small/model.bin"), 500);
        write(&old.join("parakeet-v3/encoder.onnx"), 1000);
        write(&old.join("parakeet-v3/vocab.txt"), 20);
        // Downloaded again into the new place meanwhile: one file the same, one different.
        write(&new.join("parakeet-v3/encoder.onnx"), 1000);
        write(&new.join("parakeet-v3/vocab.txt"), 21);
        let r = move_models(&old, &new).unwrap();
        assert!(!r.renamed);
        assert_eq!(size(&new.join("whisper-small/model.bin")), Some(500), "a whole model folder moved");
        assert_eq!(size(&new.join("parakeet-v3/vocab.txt")), Some(21), "the new copy is kept");
        assert_eq!(size(&old.join("parakeet-v3/vocab.txt")), Some(20), "the different old file stays, not lost");
        assert!(!old.join("parakeet-v3/encoder.onnx").exists(), "an identical old file goes");
        assert_eq!(r.left, 1);
    }

    #[test]
    fn a_pass_cut_short_is_finished_by_the_next_one() {
        let d = tempfile::tempdir().unwrap();
        let (old, new) = (d.path().join("old"), d.path().join("new"));
        write(&old.join("m/a.bin"), 300);
        write(&old.join("m/b.bin"), 400);
        // A crash mid-copy: a's copy is complete, b only half there under its temp name.
        write(&new.join("m/a.bin"), 300);
        write(&new.join(format!("m/b.bin{PARTIAL}")), 200);
        move_models(&old, &new).unwrap();
        assert_eq!(size(&new.join("m/a.bin")), Some(300));
        assert_eq!(size(&new.join("m/b.bin")), Some(400));
        assert!(!new.join(format!("m/b.bin{PARTIAL}")).exists(), "the half copy is replaced");
        assert!(!old.exists(), "everything moved, the old folder is gone");
    }

    #[test]
    fn copying_keeps_the_original_until_the_copy_is_whole() {
        let d = tempfile::tempdir().unwrap();
        let (src, dst) = (d.path().join("a/model.bin"), d.path().join("b/model.bin"));
        write(&src, 123);
        fs::create_dir_all(dst.parent().unwrap()).unwrap();
        // Force the copy path: an existing different file means "keep both".
        write(&dst, 5);
        let mut r = Moved::default();
        assert!(move_file(&src, &dst, &mut r).is_err());
        assert_eq!(size(&src), Some(123));
        assert_eq!(size(&dst), Some(5));
    }

    /// The copy path itself (rename refused): on Windows a file open without delete sharing can't be renamed,
    /// but it can be copied; it stays in the old place until it's closed.
    #[cfg(windows)]
    #[test]
    fn a_file_in_use_is_copied_and_left_for_next_time() {
        use std::os::windows::fs::OpenOptionsExt;
        let d = tempfile::tempdir().unwrap();
        let (old, new) = (d.path().join("old"), d.path().join("new"));
        write(&old.join("m/model.bin"), 2048);
        write(&old.join("m/other.bin"), 10);
        // Open without FILE_SHARE_DELETE (share read only): rename and delete fail while it's open.
        let held = fs::OpenOptions::new().read(true).share_mode(1).open(old.join("m/model.bin")).unwrap();
        let r = move_models(&old, &new).unwrap();
        assert!(!r.renamed);
        assert_eq!(size(&new.join("m/model.bin")), Some(2048), "copied");
        assert_eq!(size(&old.join("m/model.bin")), Some(2048), "the original stays while in use");
        assert_eq!(size(&new.join("m/other.bin")), Some(10));
        drop(held);
        let r = move_models(&old, &new).unwrap();
        assert_eq!(r.left, 0);
        assert!(!old.exists());
        assert_eq!(size(&new.join("m/model.bin")), Some(2048));
    }
}
