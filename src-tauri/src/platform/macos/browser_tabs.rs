//! A browser's open tab URLs, through Apple Events (`osascript`). macOS asks once per browser ("Talaan wants to
//! control Chrome"); a "no" is remembered by the system and reported here as `Denied`. Only called for a browser
//! that is already running and holding the mic, so it never launches one.

use crate::calls::tabs::{Family, Tabs};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// errAEEventNotPermitted: Automation was refused.
const NOT_PERMITTED: &str = "-1743";

pub fn tab_urls(app_bundle: &str, family: Family) -> Tabs {
    let property = match family {
        Family::Chromium | Family::Safari => "URL of every tab of every window",
    };
    let script = format!("tell application id \"{app_bundle}\" to get {property}");
    let Ok(mut child) = Command::new("/usr/bin/osascript").args(["-e", &script]).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() else {
        return Tabs::Unknown;
    };
    // While macOS shows the permission prompt the script waits; don't hold the calls loop for it.
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if started.elapsed() < Duration::from_secs(3) => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Tabs::Unknown;
            }
        }
    };
    let mut out = String::new();
    let mut err = String::new();
    let _ = child.stdout.take().map(|mut o| o.read_to_string(&mut out));
    let _ = child.stderr.take().map(|mut e| e.read_to_string(&mut err));
    if !status.success() {
        return if err.contains(NOT_PERMITTED) { Tabs::Denied } else { Tabs::Unknown };
    }
    // osascript prints a flat, comma-separated list. URLs don't contain ", " (spaces are encoded).
    Tabs::Urls(out.trim().split(", ").map(str::trim).filter(|u| !u.is_empty() && *u != "missing value").map(String::from).collect())
}
