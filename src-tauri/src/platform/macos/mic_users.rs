//! Which apps are using the microphone right now: Core Audio's per-process "running input" flag (macOS 14.2+).
//! Public API, no extra permission. Talaan itself is left out.

use super::coreaudio::*;
use objc2_core_audio::*;

/// A process capturing audio input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicUser {
    pub pid: i32,
    /// Empty for command-line tools.
    pub bundle_id: String,
    pub path: String,
}

unsafe extern "C" {
    fn proc_pidpath(pid: i32, buffer: *mut u8, size: u32) -> i32;
}

fn path_of(pid: i32) -> String {
    let mut buf = vec![0u8; 4096];
    let n = unsafe { proc_pidpath(pid, buf.as_mut_ptr(), buf.len() as u32) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf8_lossy(&buf[..n as usize]).into_owned()
}

/// Every process other than Talaan that has audio input running.
pub fn mic_users() -> Vec<MicUser> {
    let own = std::process::id() as i32;
    let Ok(processes) = get_ids(kAudioObjectSystemObject as u32, kAudioHardwarePropertyProcessObjectList) else { return vec![] };
    processes
        .into_iter()
        .filter(|p| get::<u32>(*p, kAudioProcessPropertyIsRunningInput, kAudioObjectPropertyScopeGlobal).unwrap_or(0) != 0)
        .filter_map(|p| {
            let pid: i32 = get(p, kAudioProcessPropertyPID, kAudioObjectPropertyScopeGlobal).ok()?;
            (pid != own).then(|| MicUser { pid, bundle_id: get_string(p, kAudioProcessPropertyBundleID, kAudioObjectPropertyScopeGlobal).unwrap_or_default(), path: path_of(pid) })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_does_not_fail_and_leaves_talaan_out() {
        let own = std::process::id() as i32;
        assert!(mic_users().iter().all(|u| u.pid != own));
    }

    /// Run while something captures the mic (e.g. the scratch `mic` probe): it must show up.
    /// `cargo test mic_users_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mic_users_smoke() {
        eprintln!("{:#?}", mic_users());
    }
}
