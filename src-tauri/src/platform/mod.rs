//! OS-specific code. Each platform gets its own folder and exposes the same
//! items, so adding Windows means adding `windows/` and one `cfg` line here.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(target_os = "macos"))]
pub const NAME: &str = "unsupported";

#[cfg(test)]
mod tests {
    #[test]
    fn reports_a_platform_name() {
        assert!(!super::NAME.is_empty());
    }
}
