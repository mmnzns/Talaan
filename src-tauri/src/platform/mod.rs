//! OS-specific code. Each platform gets its own folder and exposes the same
//! items, so adding Windows means adding `windows/` and one `cfg` line here.

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(target_os = "macos"))]
pub const NAME: &str = "unsupported";

/// Until the Windows port adds its own clock (QPC, matching WASAPI timestamps).
#[cfg(not(target_os = "macos"))]
pub fn host_now_ns() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}

/// Until the Windows port sets its own thread priority.
#[cfg(not(target_os = "macos"))]
pub fn background_priority() {}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_a_platform_name() {
        assert!(!super::NAME.is_empty());
    }
}
