//! Free space on a disk, to warn before a recording fills it.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Bytes available to the user (not the root-only reserve) on the disk holding `path`; None if it can't be told.
pub fn free_space(path: &Path) -> Option<u64> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `c_path` is a valid C string and `stats` is a statvfs the call fills in.
    if unsafe { libc::statvfs(c_path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return None;
    }
    let stats = unsafe { stats.assume_init() };
    Some(stats.f_bavail as u64 * stats.f_frsize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_folder_has_free_space() {
        let dir = tempfile::tempdir().unwrap();
        assert!(free_space(dir.path()).is_some_and(|b| b > 0));
    }

    #[test]
    fn a_missing_folder_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(free_space(&dir.path().join("nope")), None);
    }
}
