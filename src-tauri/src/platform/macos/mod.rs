//! macOS-only code: audio capture, permissions and call detection land here.

pub mod clock;

pub use clock::host_now_ns;

pub const NAME: &str = "macos";
