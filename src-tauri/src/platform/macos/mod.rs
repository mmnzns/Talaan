//! macOS-only code: audio capture, permissions and call detection.

pub mod clock;
pub mod coreaudio;
pub mod system_tap;

pub use clock::host_now_ns;

pub const NAME: &str = "macos";
