//! What the log and the diagnostics say about this Mac: the macOS version and the processor.

use std::process::Command;

fn run(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd).args(args).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

/// "15.1": the version number alone.
pub fn product_version() -> String {
    run("/usr/bin/sw_vers", &["-productVersion"])
}

/// "macOS 15.1 (24B83)".
pub fn os_version() -> String {
    let version = product_version();
    let build = run("/usr/bin/sw_vers", &["-buildVersion"]);
    match (version.is_empty(), build.is_empty()) {
        (true, _) => "macOS".into(),
        (false, true) => format!("macOS {version}"),
        (false, false) => format!("macOS {version} ({build})"),
    }
}

/// "Apple M2 Pro".
pub fn cpu_name() -> String {
    run("/usr/sbin/sysctl", &["-n", "machdep.cpu.brand_string"])
}

#[cfg(test)]
mod tests {
    #[test]
    fn this_mac_answers() {
        assert!(super::os_version().starts_with("macOS"));
        let _ = super::cpu_name();
    }
}
