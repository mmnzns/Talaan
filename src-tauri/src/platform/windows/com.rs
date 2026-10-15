//! COM on the calling thread. Every Windows audio call needs it; initializing twice is fine.
//!
//! The crate is named `::windows::` because the sibling `platform::windows` module shadows it. Threads never
//! call CoUninitialize: they live as long as the app or end with it.

use ::windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

/// Joins the multithreaded apartment (idempotent; an already-initialized thread keeps its apartment).
pub fn init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}
