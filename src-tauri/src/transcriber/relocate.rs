//! Moving Talaan's models folder (Windows, v1.1): from `%APPDATA%\Talaan\models` (Roaming, copied at every
//! sign-in on a roaming profile) to `%LOCALAPPDATA%\com.craftconceptsdigital.talaan\models`.
//!
//! Two steps, so a model is never half in each place and nothing is pulled from under a worker or a helper:
//!
//! 1. `prepare`, at launch before any worker starts: renames the whole folder, or each model folder, into place
//!    (instant on one volume), and deletes old model folders a previous launch already copied in full.
//! 2. `copy_rest`, in the background, for what couldn't be renamed (another volume, a file in use): each model
//!    folder is copied whole into a temp folder in the new place, then renamed into place in one step. The old
//!    folder stays until the next launch's `prepare`: something may have found the model there already (a
//!    summary helper is given the path and opens it later), and `ModelStore::locate` keeps using it until the new
//!    copy is complete.
//!
//! Nothing in the new folder is ever replaced (a download may have put a copy there). A copy that fails or is
//! cancelled is removed, and a model is copied only when the new volume has room for it plus a margin; otherwise
//! it waits for the next launch. Delete model and Delete everything coordinate with the copy through `forget` and
//! `stop`. Pure std plus one Win32 call for free space; tested with temp folders.
#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::Duration;

/// Prefix of a model folder still being copied; never a model id, so `locate` never sees it.
const PARTIAL: &str = ".talaan-move-";
/// Room left on the new volume after a copy.
const SPARE_BYTES: u64 = 1 << 30;

/// What a step did.
#[derive(Debug, Default, PartialEq)]
pub struct Moved {
    /// The whole folder was renamed in one step.
    pub renamed: bool,
    /// Model folders renamed or copied into place.
    pub moved: usize,
    /// Old model folders removed because the new place already had them in full.
    pub cleaned: usize,
    /// Model folders still to copy (`prepare`), or left for the next launch (`copy_rest`).
    pub left: Vec<String>,
}

/// Step 1, synchronous, before the workers start (see the module doc).
pub fn prepare(old: &Path, new: &Path) -> io::Result<Moved> {
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
    for entry in fs::read_dir(old)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let (src, dst) = (entry.path(), new.join(&name));
        if !src.is_dir() {
            continue; // models are folders; anything else stays where it is
        }
        if dst.exists() {
            if holds_all_of(&dst, &src) && fs::remove_dir_all(&src).is_ok() {
                report.cleaned += 1;
            } else {
                // A partial download or a different copy is there: the old one stays (locate falls back to it).
                report.left.push(name);
            }
        } else if fs::rename(&src, &dst).is_ok() {
            report.moved += 1;
        } else {
            report.left.push(name);
        }
    }
    // Temp folders a cut-short copy left behind.
    for entry in fs::read_dir(new)?.flatten() {
        if entry.file_name().to_string_lossy().starts_with(PARTIAL) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
    let _ = fs::remove_dir(old); // only when empty
    Ok(report)
}

/// Step 2, in the background: copies each model in `names` whole (see the module doc). `free` says how many
/// bytes the new volume has (None: unknown, then copying is skipped).
pub fn copy_rest(old: &Path, new: &Path, names: &[String], free: impl Fn(&Path) -> Option<u64>) -> Moved {
    let mut report = Moved::default();
    let _running = Running::start();
    for name in names {
        let (src, dst) = (old.join(name), new.join(name));
        if state().all {
            break;
        }
        if stopped(name) {
            continue;
        }
        if dst.exists() || !src.is_dir() {
            continue;
        }
        let size = crate::storage::io::dir_size(&src);
        if free(new).is_none_or(|f| f < size.saturating_add(SPARE_BYTES)) {
            report.left.push(name.clone()); // not enough room now; next launch
            continue;
        }
        let tmp = new.join(temp_name(name));
        let _ = fs::remove_dir_all(&tmp);
        let copied = copy_dir(&src, &tmp, &|| stopped(name));
        // The final step under the lock Delete model and Delete everything take, so a model deleted meanwhile
        // isn't put back.
        let state = state();
        let placed = copied.is_ok() && !state.is_stopped(name) && !dst.exists() && fs::rename(&tmp, &dst).is_ok();
        drop(state);
        if placed {
            report.moved += 1;
        } else {
            let _ = fs::remove_dir_all(&tmp);
            report.left.push(name.clone());
        }
    }
    report
}

/// Whether `have` has every file of `want` with the same size (a complete copy).
fn holds_all_of(have: &Path, want: &Path) -> bool {
    let Ok(entries) = fs::read_dir(want) else { return false };
    entries.flatten().all(|e| {
        let (w, h) = (e.path(), have.join(e.file_name()));
        if w.is_dir() {
            holds_all_of(&h, &w)
        } else {
            matches!((fs::metadata(&w), fs::metadata(&h)), (Ok(a), Ok(b)) if b.is_file() && a.len() == b.len())
        }
    })
}

/// Copies a folder tree, checking `cancel` between 8 MB blocks. On any error or cancel the caller removes `to`.
fn copy_dir(from: &Path, to: &Path, cancel: &dyn Fn() -> bool) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if entry.file_type()?.is_dir() {
            copy_dir(&src, &dst, cancel)?;
            continue;
        }
        // A plain read, so a file another program has open for reading can still be copied.
        let mut input = fs::File::open(&src)?;
        let mut output = fs::OpenOptions::new().write(true).create_new(true).open(&dst)?;
        let mut buf = vec![0u8; 8 << 20];
        loop {
            if cancel() {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            output.write_all(&buf[..n])?;
        }
        output.sync_all()?;
        if fs::metadata(&dst)?.len() != fs::metadata(&src)?.len() {
            return Err(io::Error::other("short copy"));
        }
    }
    Ok(())
}

/// Free bytes on the volume holding `path` (its nearest existing ancestor).
#[cfg(target_os = "windows")]
pub fn free_bytes(path: &Path) -> Option<u64> {
    use ::windows::core::HSTRING;
    use ::windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let dir = path.ancestors().find(|p| p.exists())?;
    let mut free = 0u64;
    unsafe { GetDiskFreeSpaceExW(&HSTRING::from(dir.as_os_str()), Some(&mut free), None, None) }.ok()?;
    Some(free)
}

#[cfg(not(target_os = "windows"))]
pub fn free_bytes(_path: &Path) -> Option<u64> {
    None
}

/// What Delete model and Delete everything told the copy.
#[derive(Default)]
struct State {
    all: bool,
    models: BTreeSet<String>,
    running: bool,
}

impl State {
    fn is_stopped(&self, name: &str) -> bool {
        self.all || self.models.contains(name)
    }
}

static STATE: Mutex<State> = Mutex::new(State { all: false, models: BTreeSet::new(), running: false });
static IDLE: Condvar = Condvar::new();

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn stopped(name: &str) -> bool {
    state().is_stopped(name)
}

/// Marks the copy as running for its lifetime, and wakes `stop` when it ends.
struct Running;

impl Running {
    fn start() -> Running {
        state().running = true;
        Running
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        state().running = false;
        IDLE.notify_all();
    }
}

/// Delete model: the copy leaves `id` alone from now on. `then` (the deletion) runs under the copy's lock, so the
/// copy can't put the model back afterwards.
pub fn forget<T>(id: &str, then: impl FnOnce() -> T) -> T {
    let mut s = state();
    s.models.insert(id.to_string());
    then()
}

/// Delete everything: stops the copy and waits (up to `timeout`) until it has let go of its files.
pub fn stop(timeout: Duration) {
    let mut s = state();
    s.all = true;
    let _ = IDLE.wait_timeout_while(s, timeout, |s| s.running);
}

/// The temp folder name a model is copied under.
fn temp_name(id: &str) -> PathBuf {
    PathBuf::from(format!("{PARTIAL}{id}"))
}

#[cfg(test)]
pub(crate) fn reset_for_tests() {
    let mut s = state();
    s.all = false;
    s.models.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The module's stop flags are global: tests that use them run one at a time.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn serial() -> MutexGuard<'static, ()> {
        let g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_tests();
        g
    }

    fn write(p: &Path, bytes: usize) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![7u8; bytes]).unwrap();
    }

    fn size(p: &Path) -> Option<u64> {
        fs::metadata(p).ok().map(|m| m.len())
    }

    const PLENTY: fn(&Path) -> Option<u64> = |_| Some(u64::MAX);

    #[test]
    fn nothing_to_move_is_fine() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(prepare(&d.path().join("old"), &d.path().join("new")).unwrap(), Moved::default());
        assert!(!d.path().join("new").exists());
    }

    #[test]
    fn the_whole_folder_is_renamed_when_it_can_be() {
        let d = tempfile::tempdir().unwrap();
        let (old, new) = (d.path().join("Roaming/Talaan/models"), d.path().join("Local/com.x/models"));
        write(&old.join("parakeet-v3/encoder.onnx"), 1000);
        write(&old.join("silero-vad/vad.onnx"), 10);
        let r = prepare(&old, &new).unwrap();
        assert!(r.renamed && r.left.is_empty());
        assert!(!old.exists());
        assert_eq!(size(&new.join("parakeet-v3/encoder.onnx")), Some(1000));
    }

    #[test]
    fn into_an_existing_folder_each_model_is_renamed_and_nothing_is_replaced() {
        let d = tempfile::tempdir().unwrap();
        let (old, new) = (d.path().join("old"), d.path().join("new"));
        write(&old.join("whisper-small/model.bin"), 500);
        write(&old.join("parakeet-v3/encoder.onnx"), 1000);
        write(&old.join("parakeet-v3/vocab.txt"), 20);
        // Downloaded again into the new place meanwhile (a different, partial copy).
        write(&new.join("parakeet-v3/encoder.onnx"), 400);
        let r = prepare(&old, &new).unwrap();
        assert_eq!(size(&new.join("whisper-small/model.bin")), Some(500), "moved in one step");
        assert_eq!(size(&new.join("parakeet-v3/encoder.onnx")), Some(400), "the new copy is kept");
        assert_eq!(size(&old.join("parakeet-v3/vocab.txt")), Some(20), "the old copy stays, not lost");
        assert_eq!(r.left, vec!["parakeet-v3".to_string()]);
        assert_eq!(r.moved, 1);
    }

    #[test]
    fn a_model_already_copied_in_full_is_removed_from_the_old_place_at_the_next_launch() {
        let d = tempfile::tempdir().unwrap();
        let (old, new) = (d.path().join("old"), d.path().join("new"));
        write(&old.join("m/a.bin"), 300);
        write(&new.join("m/a.bin"), 300);
        // A cut-short copy's temp folder.
        write(&new.join(temp_name("n")).join("b.bin"), 10);
        let r = prepare(&old, &new).unwrap();
        assert_eq!(r.cleaned, 1);
        assert!(!old.exists());
        assert!(!new.join(temp_name("n")).exists());
    }

    #[test]
    fn a_copy_appears_whole_or_not_at_all_and_the_old_folder_stays_until_next_launch() {
        let _g = serial();
        let d = tempfile::tempdir().unwrap();
        let (old, new) = (d.path().join("old"), d.path().join("new"));
        write(&old.join("m/a.bin"), 3000);
        write(&old.join("m/sub/b.bin"), 4000);
        fs::create_dir_all(&new).unwrap();
        let r = copy_rest(&old, &new, &["m".into()], PLENTY);
        assert_eq!(r.moved, 1);
        assert_eq!(size(&new.join("m/a.bin")), Some(3000));
        assert_eq!(size(&new.join("m/sub/b.bin")), Some(4000));
        assert!(old.join("m/a.bin").exists(), "the old copy stays for this session");
        assert!(!new.join(temp_name("m")).exists());
        // Next launch: the old one goes.
        assert_eq!(prepare(&old, &new).unwrap().cleaned, 1);
        assert!(!old.exists());
    }

    #[test]
    fn without_room_nothing_is_copied() {
        let _g = serial();
        let d = tempfile::tempdir().unwrap();
        let (old, new) = (d.path().join("old"), d.path().join("new"));
        write(&old.join("m/a.bin"), 3000);
        fs::create_dir_all(&new).unwrap();
        let r = copy_rest(&old, &new, &["m".into()], |_| Some(SPARE_BYTES));
        assert_eq!(r.left, vec!["m".to_string()]);
        assert!(fs::read_dir(&new).unwrap().next().is_none(), "no temp folder left");
        assert_eq!(copy_rest(&old, &new, &["m".into()], |_| None).left.len(), 1, "unknown free space: wait");
    }

    /// A copy that fails (here: a file nobody else may open) leaves no temp folder and waits for next time.
    #[cfg(windows)]
    #[test]
    fn a_failed_copy_leaves_no_temp_folder() {
        use std::os::windows::fs::OpenOptionsExt;
        let _g = serial();
        let d = tempfile::tempdir().unwrap();
        let (old, new) = (d.path().join("old"), d.path().join("new"));
        write(&old.join("m/a.bin"), 3000);
        write(&old.join("m/b.bin"), 3000);
        fs::create_dir_all(&new).unwrap();
        let locked = fs::OpenOptions::new().read(true).share_mode(0).open(old.join("m/b.bin")).unwrap();
        let r = copy_rest(&old, &new, &["m".into()], PLENTY);
        assert_eq!(r.left, vec!["m".to_string()]);
        assert!(fs::read_dir(&new).unwrap().next().is_none(), "no temp folder or half model left");
        drop(locked);
        assert_eq!(copy_rest(&old, &new, &["m".into()], PLENTY).moved, 1);
    }

    #[test]
    fn a_deleted_model_is_not_put_back_and_delete_everything_stops_the_copy() {
        let _g = serial();
        let d = tempfile::tempdir().unwrap();
        let (old, new) = (d.path().join("old"), d.path().join("new"));
        write(&old.join("m/a.bin"), 3000);
        write(&old.join("n/a.bin"), 3000);
        fs::create_dir_all(&new).unwrap();
        forget("m", || ());
        let r = copy_rest(&old, &new, &["m".into(), "n".into()], PLENTY);
        assert!(!new.join("m").exists() && !new.join(temp_name("m")).exists());
        assert!(new.join("n/a.bin").exists());
        assert_eq!(r.moved, 1);
        reset_for_tests();
        let _ = fs::remove_dir_all(new.join("n"));
        stop(Duration::from_millis(10));
        let r = copy_rest(&old, &new, &["n".into()], PLENTY);
        assert_eq!(r.moved, 0);
        assert!(!new.join("n").exists());
        reset_for_tests();
    }

    #[test]
    fn stop_waits_for_a_running_copy() {
        let _g = serial();
        let t = std::thread::spawn(|| {
            let _r = Running::start();
            std::thread::sleep(Duration::from_millis(200));
        });
        std::thread::sleep(Duration::from_millis(50));
        let started = std::time::Instant::now();
        stop(Duration::from_secs(5));
        assert!(started.elapsed() >= Duration::from_millis(100), "returned before the copy let go");
        t.join().unwrap();
        reset_for_tests();
    }

    /// On Windows a file open without delete sharing can't be renamed: `prepare` leaves its model for the copy,
    /// which reads it anyway.
    #[cfg(windows)]
    #[test]
    fn a_model_in_use_is_copied_instead_of_renamed() {
        use std::os::windows::fs::OpenOptionsExt;
        let _g = serial();
        let d = tempfile::tempdir().unwrap();
        let (old, new) = (d.path().join("old"), d.path().join("new"));
        write(&old.join("m/model.bin"), 2048);
        fs::create_dir_all(&new).unwrap();
        let held = fs::OpenOptions::new().read(true).share_mode(1).open(old.join("m/model.bin")).unwrap();
        let r = prepare(&old, &new).unwrap();
        assert_eq!(r.left, vec!["m".to_string()]);
        let r = copy_rest(&old, &new, &r.left, PLENTY);
        assert_eq!(r.moved, 1);
        assert_eq!(size(&new.join("m/model.bin")), Some(2048));
        drop(held);
        assert_eq!(prepare(&old, &new).unwrap().cleaned, 1);
        assert!(!old.exists());
    }

    #[cfg(windows)]
    #[test]
    fn free_space_is_read() {
        assert!(free_bytes(&std::env::temp_dir().join("does/not/exist")).is_some_and(|f| f > 0));
    }
}
