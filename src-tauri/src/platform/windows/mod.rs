//! Windows-only code: audio capture, permissions and call detection. Mirrors `platform/macos/` item for item.

pub mod browser_tabs;
pub mod calendar;
pub mod clock;
pub mod com;
pub mod devices;
pub mod mic_users;
pub mod notify;
pub mod permissions;
pub mod power;
pub mod system;
pub mod system_tap;
pub mod thread;
pub mod trash;

pub use clock::host_now_ns;
pub use thread::background_priority;

pub const NAME: &str = "windows";
