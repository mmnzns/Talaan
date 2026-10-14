//! macOS-only code: audio capture, permissions and call detection.

pub mod clock;
pub mod coreaudio;
pub mod devices;
pub mod mic_users;
pub mod notify;
pub mod permissions;
pub mod power;
pub mod system_tap;
pub mod thread;

pub use clock::host_now_ns;
pub use thread::background_priority;

pub const NAME: &str = "macos";
