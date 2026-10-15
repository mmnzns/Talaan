//! Thread priority for long background work (transcription, indexing), so a call being recorded never stutters.
//! Below normal, not THREAD_MODE_BACKGROUND_BEGIN: background mode also throttles disk reads, which would make
//! loading a 1.5 GB model crawl.

use ::windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};

pub fn background_priority() {
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn setting_the_priority_does_not_crash() {
        std::thread::spawn(super::background_priority).join().unwrap();
    }
}
