//! OS-specific code. Each platform gets its own folder and exposes the same items (see the contract table in
//! docs/plans/W0-W1-windows-build-and-recorder.md), so callers only ever say `crate::platform::…`.

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("Talaan runs on macOS and Windows");

#[cfg(test)]
mod tests {
    #[test]
    fn reports_a_platform_name() {
        assert!(!super::NAME.is_empty());
    }
}
