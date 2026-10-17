//! Moving a file to the Recycle Bin (auto-save retiring its own old copy), so nothing Talaan removes is gone for good.

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use windows::Win32::Storage::FileSystem::GetDriveTypeW;
const DRIVE_FIXED: u32 = 3;
use windows::Win32::UI::Shell::{SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW};

/// Only on a fixed drive: elsewhere (USB sticks, network shares) the shell deletes for good instead, so the file is
/// left where it is and the error says why.
pub fn to_trash(path: &Path) -> io::Result<()> {
    // The drive's root: C:\ here, or \\server\share\ for a network share.
    let root: PathBuf = path.components().take_while(|c| matches!(c, Component::Prefix(_) | Component::RootDir)).collect();
    let root: Vec<u16> = root.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: `root` is a NUL-terminated wide string that outlives the call.
    let kind = unsafe { GetDriveTypeW(windows_core::PCWSTR(root.as_ptr())) };
    if kind != DRIVE_FIXED {
        return Err(io::Error::other(format!("{} has no Recycle Bin (drive type {kind})", path.display())));
    }
    // The shell wants a list of paths, each ending in a NUL, the list ending in another.
    let mut from: Vec<u16> = path.as_os_str().encode_wide().collect();
    from.extend([0, 0]);
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: windows_core::PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO.0 | FOF_NOCONFIRMATION.0 | FOF_NOERRORUI.0 | FOF_SILENT.0) as u16,
        ..Default::default()
    };
    // SAFETY: `op` and the path buffer it points to live until the call returns.
    let code = unsafe { SHFileOperationW(&mut op) };
    if code != 0 || op.fAnyOperationsAborted.as_bool() {
        return Err(io::Error::other(format!("couldn't move {} to the Recycle Bin (shell error {code})", path.display())));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Puts a temp file in this PC's Recycle Bin, so it only runs when asked: `cargo test recycle_bin -- --ignored`.
    #[test]
    #[ignore]
    fn a_file_goes_to_the_recycle_bin() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("talaan trash test.md");
        std::fs::write(&file, "x").unwrap();
        super::to_trash(&file).unwrap();
        assert!(!file.exists());
    }
}
