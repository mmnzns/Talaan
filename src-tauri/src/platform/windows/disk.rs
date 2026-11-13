//! Free space on a disk, to warn before a recording fills it.

use ::windows::core::HSTRING;
use ::windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use std::path::Path;

/// Bytes available to the user (their quota, not the whole disk's free space) on the disk holding `path`; None if it
/// can't be told.
pub fn free_space(path: &Path) -> Option<u64> {
    let mut available = 0u64;
    unsafe { GetDiskFreeSpaceExW(&HSTRING::from(path.as_os_str()), Some(&mut available), None, None) }.ok()?;
    Some(available)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_folder_has_free_space() {
        let dir = tempfile::tempdir().unwrap();
        assert!(free_space(dir.path()).is_some_and(|b| b > 0));
    }
}
