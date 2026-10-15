//! COM on the calling thread. Every Windows audio call needs it; initializing twice is fine.

use ::windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

/// Joins the multithreaded apartment (idempotent; an already-initialized thread keeps its apartment).
pub fn init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}
