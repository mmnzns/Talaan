//! Moving a file to the Trash (auto-save retiring its own old copy), so nothing Talaan removes is gone for good.

use objc2_foundation::{NSFileManager, NSString, NSURL};
use std::io;
use std::path::Path;

pub fn to_trash(path: &Path) -> io::Result<()> {
    let Some(p) = path.to_str() else { return Err(io::Error::other("path isn't valid UTF-8")) };
    let url = NSURL::fileURLWithPath(&NSString::from_str(p));
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| io::Error::other(e.localizedDescription().to_string()))
}
