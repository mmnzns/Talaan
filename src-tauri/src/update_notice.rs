//! When to look for a new version and when to say so (pure: no Tauri). The update check runs 20 s after launch,
//! then every 6 hours while Talaan runs (an hour after a failed check, so a laptop that was offline at launch
//! hears about it sooner). Times are wall-clock seconds, so a check that fell due while the computer slept runs
//! soon after it wakes.
//!
//! A new version gets one system notification, ever: the version that was announced is kept in
//! `update-notice.json` in the data folder, so a restart doesn't announce it again. While a recording runs, the
//! notification waits and is posted when the recording ends.

use crate::storage::io::{read_json_opt, write_json_atomic};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const FIRST_CHECK_SECS: u64 = 20;
pub const CHECK_EVERY_SECS: u64 = 6 * 3600;
pub const RETRY_SECS: u64 = 3600;
/// How long a check may take before it counts as failed (and is retried after RETRY_SECS).
pub const CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// How long downloading the update may take after Update now.
pub const DOWNLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20 * 60);

/// The last check: when it ran and whether it reached GitHub.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Last {
    pub at: u64,
    pub ok: bool,
}

/// When the next check is due, in the same seconds as `started` and `last.at`.
pub fn next_check_at(started: u64, last: Option<Last>) -> u64 {
    match last {
        None => started + FIRST_CHECK_SECS,
        Some(l) => l.at + if l.ok { CHECK_EVERY_SECS } else { RETRY_SECS },
    }
}

/// Whether to check now. A clock set back past the last check counts as due (better one extra check than none).
pub fn check_due(started: u64, last: Option<Last>, now: u64) -> bool {
    if last.is_some_and(|l| now < l.at) {
        return true;
    }
    now >= next_check_at(started, last)
}

/// Which version to announce, and when. Every decision is one call on one lock: `found` (or
/// `recording_stopped`) both decides and reserves the version, so a Check now in Settings (`posted`) and the
/// background check can't both announce it.
#[derive(Default, Debug)]
pub struct Notice {
    /// The version already announced (from disk, so it survives restarts).
    notified: Option<String>,
    /// A version found while recording, announced when the recording ends.
    waiting: Option<String>,
    /// The version being posted right now (decided, not yet confirmed by `posted` or undone by `failed`).
    posting: Option<String>,
}

impl Notice {
    pub fn new(notified: Option<String>) -> Self {
        Notice { notified, waiting: None, posting: None }
    }

    fn settled(&self, version: &str) -> bool {
        self.notified.as_deref() == Some(version) || self.posting.as_deref() == Some(version)
    }

    /// A check found `version`. Returns it (reserved) when the notification should be posted now; while
    /// `recording`, it waits for `recording_stopped` instead. Nothing for a version already announced or being
    /// announced.
    pub fn found(&mut self, version: &str, recording: bool) -> Option<String> {
        if self.settled(version) {
            self.waiting = None;
            return None;
        }
        if recording {
            self.waiting = Some(version.to_string());
            return None;
        }
        self.waiting = None;
        self.posting = Some(version.to_string());
        Some(version.to_string())
    }

    /// The recording ended: the version that waited, if any and still not announced (reserved, like `found`).
    pub fn recording_stopped(&mut self) -> Option<String> {
        let v = self.waiting.take().filter(|v| !self.settled(v))?;
        self.posting = Some(v.clone());
        Some(v)
    }

    /// `version` was announced (or the user saw it in Settings): never again. True when that's news, so the
    /// caller saves it once.
    pub fn posted(&mut self, version: &str) -> bool {
        let news = self.notified.as_deref() != Some(version);
        self.notified = Some(version.to_string());
        if self.waiting.as_deref() == Some(version) {
            self.waiting = None;
        }
        if self.posting.as_deref() == Some(version) {
            self.posting = None;
        }
        news
    }

    /// The notification for `version` didn't go out (turned off, refused by the system): the next check tries
    /// again.
    pub fn failed(&mut self, version: &str) {
        if self.posting.as_deref() == Some(version) {
            self.posting = None;
        }
    }

    #[cfg(test)]
    pub fn notified(&self) -> Option<&str> {
        self.notified.as_deref()
    }
}

/// Set while an update downloads and installs. No recording may start then: the installer quits Talaan, which
/// would cut the call short. `begin` sets it (once: a second install gets None); the guard clears it when the
/// install fails or gives up.
#[derive(Default)]
pub struct InstallFlag(AtomicBool);

pub struct InstallGuard<'a>(&'a AtomicBool);

impl InstallFlag {
    pub fn begin(&self) -> Option<InstallGuard<'_>> {
        (!self.0.swap(true, Ordering::SeqCst)).then_some(InstallGuard(&self.0))
    }

    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

impl Drop for InstallGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[derive(Serialize, Deserialize)]
struct NoticeFile {
    notified: String,
}

pub fn notice_path(data_dir: &Path) -> PathBuf {
    data_dir.join("update-notice.json")
}

/// The version announced last, if the file is there and looks like a version number.
pub fn read_notified(path: &Path) -> Option<String> {
    let f: NoticeFile = read_json_opt(path).ok().flatten()?;
    looks_like_version(&f.notified).then_some(f.notified)
}

pub fn write_notified(path: &Path, version: &str) -> Result<(), crate::storage::StorageError> {
    write_json_atomic(path, &NoticeFile { notified: version.to_string() })
}

/// Where the announced version is kept: `update-notice.json` in the data folder, and a copy in the temp folder
/// for when the data folder can't be written (so a broken data folder doesn't mean the same notification at
/// every launch).
pub struct NoticeFiles {
    pub main: PathBuf,
    pub spare: PathBuf,
}

impl NoticeFiles {
    pub fn new(data_dir: &Path, temp_dir: &Path) -> Self {
        NoticeFiles { main: notice_path(data_dir), spare: temp_dir.join("talaan-update-notice.json") }
    }

    /// The version announced last: the data folder's file, else the spare.
    pub fn read(&self) -> Option<String> {
        read_notified(&self.main).or_else(|| read_notified(&self.spare))
    }

    /// Saves `version`; Err only when neither file could be written (it's still remembered until Talaan quits).
    pub fn write(&self, version: &str) -> Result<(), crate::storage::StorageError> {
        match write_notified(&self.main, version) {
            Ok(()) => {
                // A spare from an earlier failure would otherwise win over nothing; it's no longer needed.
                let _ = std::fs::remove_file(&self.spare);
                Ok(())
            }
            Err(_) => write_notified(&self.spare, version),
        }
    }
}

fn looks_like_version(v: &str) -> bool {
    !v.is_empty() && v.len() <= 32 && v.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_000_000;

    #[test]
    fn the_first_check_is_20_seconds_after_launch() {
        assert!(!check_due(T0, None, T0));
        assert!(!check_due(T0, None, T0 + 19));
        assert!(check_due(T0, None, T0 + 20));
    }

    #[test]
    fn then_every_6_hours() {
        let last = Some(Last { at: T0 + 20, ok: true });
        assert!(!check_due(T0, last, T0 + 20 + 3600));
        assert!(!check_due(T0, last, T0 + 20 + 6 * 3600 - 1));
        assert!(check_due(T0, last, T0 + 20 + 6 * 3600));
        // Asleep overnight: due as soon as it wakes.
        assert!(check_due(T0, last, T0 + 20 + 14 * 3600));
    }

    #[test]
    fn a_failed_check_retries_after_an_hour() {
        let last = Some(Last { at: T0 + 20, ok: false });
        assert!(!check_due(T0, last, T0 + 20 + 3599));
        assert!(check_due(T0, last, T0 + 20 + 3600));
    }

    #[test]
    fn a_hung_check_gives_up_long_before_the_next_one() {
        assert!(CHECK_TIMEOUT.as_secs() <= 30);
        assert!(CHECK_TIMEOUT.as_secs() < RETRY_SECS && DOWNLOAD_TIMEOUT > CHECK_TIMEOUT);
    }

    #[test]
    fn a_clock_set_back_checks_again() {
        assert!(check_due(T0, Some(Last { at: T0 + 5000, ok: true }), T0 + 100));
    }

    #[test]
    fn a_new_version_is_announced_once() {
        let mut n = Notice::new(None);
        assert_eq!(n.found("1.2.0", false).as_deref(), Some("1.2.0"));
        n.posted("1.2.0");
        assert_eq!(n.found("1.2.0", false), None, "the next check finds the same version");
        assert_eq!(n.found("1.3.0", false).as_deref(), Some("1.3.0"), "a newer one is announced");
    }

    #[test]
    fn the_announced_version_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = notice_path(dir.path());
        assert_eq!(read_notified(&path), None);
        write_notified(&path, "1.2.0").unwrap();
        let mut n = Notice::new(read_notified(&path));
        assert_eq!(n.notified(), Some("1.2.0"));
        assert_eq!(n.found("1.2.0", false), None);
    }

    #[test]
    fn when_the_data_folder_cant_be_written_the_spare_remembers() {
        let dir = tempfile::tempdir().unwrap();
        // A data "folder" that is a file: nothing can be written inside it.
        let data = dir.path().join("data");
        std::fs::write(&data, "not a folder").unwrap();
        let temp = dir.path().join("temp");
        std::fs::create_dir(&temp).unwrap();
        let files = NoticeFiles::new(&data, &temp);
        files.write("1.2.0").expect("the spare is written");
        assert_eq!(NoticeFiles::new(&data, &temp).read().as_deref(), Some("1.2.0"), "the next launch doesn't announce it again");

        // Neither writable: an error for the caller to log, nothing else.
        let gone = NoticeFiles::new(&data, &data);
        assert!(gone.write("1.3.0").is_err());
    }

    #[test]
    fn the_data_folder_wins_and_clears_the_spare() {
        let dir = tempfile::tempdir().unwrap();
        let files = NoticeFiles::new(dir.path(), dir.path());
        write_notified(&files.spare, "1.1.0").unwrap();
        assert_eq!(files.read().as_deref(), Some("1.1.0"));
        files.write("1.2.0").unwrap();
        assert!(!files.spare.exists());
        assert_eq!(files.read().as_deref(), Some("1.2.0"));
    }

    #[test]
    fn a_strange_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = notice_path(dir.path());
        std::fs::write(&path, r#"{"notified":"../../x y"}"#).unwrap();
        assert_eq!(read_notified(&path), None);
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(read_notified(&path), None);
    }

    #[test]
    fn while_recording_it_waits_for_the_recording_to_end() {
        let mut n = Notice::new(None);
        assert_eq!(n.found("1.2.0", true), None);
        assert_eq!(n.found("1.2.0", true), None, "a second check during the same call still waits");
        assert_eq!(n.recording_stopped().as_deref(), Some("1.2.0"));
        n.posted("1.2.0");
        assert_eq!(n.recording_stopped(), None, "only once");
    }

    #[test]
    fn a_recording_that_ends_with_nothing_waiting_posts_nothing() {
        let mut n = Notice::new(Some("1.1.0".into()));
        assert_eq!(n.recording_stopped(), None);
        assert_eq!(n.found("1.1.0", true), None, "already announced: nothing waits");
        assert_eq!(n.recording_stopped(), None);
    }

    #[test]
    fn a_version_being_posted_is_not_posted_twice() {
        let mut n = Notice::new(None);
        assert_eq!(n.found("1.2.0", false).as_deref(), Some("1.2.0"));
        // Before the post is confirmed: another check, or a recording that ends, decides nothing.
        assert_eq!(n.found("1.2.0", false), None);
        assert_eq!(n.found("1.2.0", true), None);
        assert_eq!(n.recording_stopped(), None);
        // Check now saw it meanwhile: saved once, the background post's confirmation is no news.
        assert!(n.posted("1.2.0"));
        assert!(!n.posted("1.2.0"));
    }

    #[test]
    fn a_post_that_didnt_go_out_is_tried_at_the_next_check() {
        let mut n = Notice::new(None);
        assert_eq!(n.found("1.2.0", false).as_deref(), Some("1.2.0"));
        n.failed("1.2.0");
        assert_eq!(n.notified(), None);
        assert_eq!(n.found("1.2.0", false).as_deref(), Some("1.2.0"));
    }

    #[test]
    fn one_install_at_a_time_and_recording_is_blocked_until_it_ends() {
        let flag = InstallFlag::default();
        assert!(!flag.is_set(), "recordings may start");
        let guard = flag.begin().expect("the first install starts");
        assert!(flag.is_set(), "no recording may start while it downloads");
        assert!(flag.begin().is_none(), "a second install is refused");
        drop(guard); // it failed or was given up
        assert!(!flag.is_set(), "recordings may start again");
        assert!(flag.begin().is_some(), "and Update now can be tried again");
    }

    #[test]
    fn seen_in_settings_while_waiting_means_no_notification_after_the_call() {
        let mut n = Notice::new(None);
        assert_eq!(n.found("1.2.0", true), None);
        n.posted("1.2.0");
        assert_eq!(n.recording_stopped(), None);
    }
}
