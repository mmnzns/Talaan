//! Sleep and wake. Recording pauses itself while the PC sleeps, so closing the lid never leaves an hour of silence.

use ::windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE};
use ::windows::Win32::System::Power::{PowerRegisterSuspendResumeNotification, PowerUnregisterSuspendResumeNotification, DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, HPOWERNOTIFY};
use ::windows::Win32::UI::WindowsAndMessaging::{DEVICE_NOTIFY_CALLBACK, PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND};
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Suspend pauses; the automatic resume (sent on every wake) resumes. The later RESUMESUSPEND only follows a
/// user-driven wake, so acting on it too would resume twice.
pub fn sleeping_for(event: u32) -> Option<bool> {
    match event {
        PBT_APMSUSPEND => Some(true),
        PBT_APMRESUMEAUTOMATIC => Some(false),
        _ => None,
    }
}

/// What Windows holds a pointer to while registered: the subscribe parameters and the callback they point back to.
struct Context {
    params: DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS,
    on_change: Box<dyn Fn(bool) + Send + Sync>,
}

unsafe extern "system" fn on_power(context: *const c_void, event: u32, _setting: *const c_void) -> u32 {
    let context = unsafe { &*(context as *const Context) };
    if let Some(sleeping) = sleeping_for(event) {
        // A panic must not unwind into Windows: that aborts the process, and the recording with it.
        let _ = catch_unwind(AssertUnwindSafe(|| (context.on_change)(sleeping)));
    }
    0
}

pub struct SleepWatcher {
    handle: Option<HPOWERNOTIFY>,
    context: *mut Context,
}

// The context is only touched by Windows' callback, and freed in drop only if Windows never had it.
unsafe impl Send for SleepWatcher {}

impl SleepWatcher {
    /// `on_change(true)` before sleep, `on_change(false)` after wake. If Windows refuses the registration the
    /// watcher still exists and simply never fires.
    pub fn new(on_change: impl Fn(bool) + Send + Sync + 'static) -> Self {
        let context = Box::into_raw(Box::new(Context { params: DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS::default(), on_change: Box::new(on_change) }));
        let mut handle = HPOWERNOTIFY::default();
        let status = unsafe {
            (*context).params = DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS { Callback: Some(on_power), Context: context as *mut c_void };
            let recipient = HANDLE(&raw mut (*context).params as *mut c_void);
            PowerRegisterSuspendResumeNotification(DEVICE_NOTIFY_CALLBACK, recipient, &mut handle.0 as *mut isize as *mut *mut c_void)
        };
        SleepWatcher { handle: (status == ERROR_SUCCESS).then_some(handle), context }
    }
}

impl Drop for SleepWatcher {
    fn drop(&mut self) {
        match self.handle.take() {
            Some(h) => {
                unsafe {
                    let _ = PowerUnregisterSuspendResumeNotification(h);
                }
                // Unregister isn't documented to wait for an `on_power` already running, so the context is never
                // freed once Windows has seen it: one small allocation per watcher, and sound whatever
                // Unregister does.
            }
            None => drop(unsafe { Box::from_raw(self.context) }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::windows::Win32::UI::WindowsAndMessaging::{PBT_APMRESUMESUSPEND, PBT_POWERSETTINGCHANGE};

    #[test]
    fn suspend_pauses_and_resume_resumes() {
        assert_eq!(sleeping_for(PBT_APMSUSPEND), Some(true));
        assert_eq!(sleeping_for(PBT_APMRESUMEAUTOMATIC), Some(false));
        assert_eq!(sleeping_for(PBT_APMRESUMESUSPEND), None); // follows RESUMEAUTOMATIC; don't resume twice
        assert_eq!(sleeping_for(PBT_POWERSETTINGCHANGE), None);
    }

    #[test]
    fn a_watcher_can_be_added_and_removed() {
        let w = SleepWatcher::new(|_| {});
        assert!(w.handle.is_some(), "Windows refused the suspend/resume registration");
        drop(w);
        drop(SleepWatcher::new(|_| {})); // a second one registers and frees cleanly too
    }
}
