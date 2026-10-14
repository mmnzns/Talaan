//! "Summary ready" notifications (UserNotifications). Permission is asked the first time one is sent, not at
//! launch. Clicking one calls back with the meeting id. The notification center throws when the process isn't
//! a real .app bundle (e.g. `tauri dev`), so everything here is skipped outside a bundle.

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread};
use objc2_foundation::{NSBundle, NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNNotificationSound, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use std::sync::OnceLock;

type OnClick = Box<dyn Fn(String) + Send + Sync>;
static ON_CLICK: OnceLock<OnClick> = OnceLock::new();

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "TalaanNotificationDelegate"]
    #[ivars = ()]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(&self, _center: &UNUserNotificationCenter, response: &UNNotificationResponse, handler: &block2::DynBlock<dyn Fn()>) {
            let id = response.notification().request().identifier().to_string();
            if let Some(f) = ON_CLICK.get() {
                f(id);
            }
            handler.call(());
        }

        /// Show the banner even while Talaan is the frontmost app.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(&self, _center: &UNUserNotificationCenter, _n: &UNNotification, handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>) {
            handler.call((UNNotificationPresentationOptions::Banner | UNNotificationPresentationOptions::Sound,));
        }
    }
);

impl Delegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

/// True when running as a bundled .app (the only place the notification center works).
pub fn available() -> bool {
    let bundle = NSBundle::mainBundle();
    bundle.bundleIdentifier().is_some() && bundle.bundlePath().to_string().ends_with(".app")
}

/// Call once at launch, on the main thread, so clicks on notifications from earlier runs are handled too.
pub fn setup(on_click: impl Fn(String) + Send + Sync + 'static) {
    if !available() {
        return;
    }
    let _ = ON_CLICK.set(Box::new(on_click));
    let delegate = Delegate::new();
    UNUserNotificationCenter::currentNotificationCenter().setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    // The center keeps only a weak reference; the delegate lives as long as the app.
    std::mem::forget(delegate);
}

/// Posts a notification identified by `id` (clicking it calls back with `id`). Asks permission the first time.
pub fn post(id: &str, title: &str, body: &str) {
    if !available() {
        return;
    }
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let (id, title, body) = (id.to_string(), title.to_string(), body.to_string());
    let after_permission = RcBlock::new(move |granted: Bool, _err: *mut NSError| {
        if !granted.as_bool() {
            return;
        }
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&title));
        content.setBody(&NSString::from_str(&body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&NSString::from_str(&id), &content, None);
        UNUserNotificationCenter::currentNotificationCenter().addNotificationRequest_withCompletionHandler(&request, None);
    });
    center.requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound, &after_permission);
}

#[cfg(test)]
mod tests {
    #[test]
    fn outside_a_bundle_nothing_is_posted_and_nothing_crashes() {
        assert!(!super::available());
        super::post("m1", "Summary ready", "Q4 roadmap sync");
    }
}
