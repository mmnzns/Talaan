//! Sleep and wake. Recording pauses itself while the Mac sleeps, so closing the lid never leaves an hour of silence.

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{NSWorkspace, NSWorkspaceDidWakeNotification, NSWorkspaceWillSleepNotification};
use objc2_foundation::NSNotification;
use std::ptr::NonNull;

pub struct SleepWatcher {
    observers: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

unsafe impl Send for SleepWatcher {}

impl SleepWatcher {
    /// `on_change(true)` before sleep, `on_change(false)` after wake.
    pub fn new(on_change: impl Fn(bool) + Send + Sync + 'static) -> Self {
        let on_change = std::sync::Arc::new(on_change);
        let center = unsafe { NSWorkspace::sharedWorkspace().notificationCenter() };
        let mut observers = vec![];
        for (name, sleeping) in [(unsafe { NSWorkspaceWillSleepNotification }, true), (unsafe { NSWorkspaceDidWakeNotification }, false)] {
            let cb = on_change.clone();
            let block = RcBlock::new(move |_n: NonNull<NSNotification>| cb(sleeping));
            let token = unsafe { center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block) };
            observers.push(token);
        }
        SleepWatcher { observers }
    }
}

impl Drop for SleepWatcher {
    fn drop(&mut self) {
        let center = unsafe { NSWorkspace::sharedWorkspace().notificationCenter() };
        for o in &self.observers {
            unsafe { center.removeObserver(AsRef::<objc2::runtime::AnyObject>::as_ref(&**o)) };
        }
    }
}
