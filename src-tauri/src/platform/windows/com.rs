//! COM on the calling thread. Every Windows audio call needs it; initializing twice is fine.
//!
//! The crate is named `::windows::` because the sibling `platform::windows` module shadows it. Threads never
//! call CoUninitialize: they live as long as the app or end with it.

use ::windows::Win32::System::Com::{CoIncrementMTAUsage, CoInitializeEx, COINIT_MULTITHREADED};

/// Joins the multithreaded apartment (idempotent; an already-initialized thread keeps its apartment), and keeps
/// the MTA alive for the whole process from the first call on. WinRT objects and the activation factories the
/// `windows` crate caches belong to the MTA; without this, the MTA ends when the last thread that joined it exits
/// (a finished worker or test thread), and a later WinRT call (toasts, XML) through a cached factory can crash.
pub fn init() {
    static KEEP_MTA: std::sync::Once = std::sync::Once::new();
    KEEP_MTA.call_once(|| {
        // The cookie is never released: the MTA lives as long as the process.
        let _ = unsafe { CoIncrementMTAUsage() };
    });
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}
