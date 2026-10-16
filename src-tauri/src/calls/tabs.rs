//! Browser tab detection (pure): which browser app a mic-using helper belongs to, and whether a tab is a call.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Chrome, Arc, Edge, Brave, Comet, Vivaldi, Opera: `URL of every tab of every window`.
    Chromium,
    Safari,
}

/// Mic-using bundle id (often a helper) → the browser app to ask, and how. Firefox has no scriptable tab URLs.
pub const BROWSERS: &[(&str, &str, &str, Family)] = &[
    // (bundle-id prefix of the mic user, app bundle id, name, family)
    ("com.google.Chrome", "com.google.Chrome", "Chrome", Family::Chromium),
    ("company.thebrowser.Browser", "company.thebrowser.Browser", "Arc", Family::Chromium),
    ("com.microsoft.edgemac", "com.microsoft.edgemac", "Edge", Family::Chromium),
    ("com.brave.Browser", "com.brave.Browser", "Brave", Family::Chromium),
    ("ai.perplexity.comet", "ai.perplexity.comet", "Comet", Family::Chromium),
    ("com.vivaldi.Vivaldi", "com.vivaldi.Vivaldi", "Vivaldi", Family::Chromium),
    ("com.operasoftware.Opera", "com.operasoftware.Opera", "Opera", Family::Chromium),
    // Safari's media runs in WebKit's GPU process.
    ("com.apple.WebKit.GPU", "com.apple.Safari", "Safari", Family::Safari),
    ("com.apple.Safari", "com.apple.Safari", "Safari", Family::Safari),
    // Windows: the exe file name (lower case), or the packaged app's family-name prefix. Firefox is left out.
    ("chrome.exe", "chrome.exe", "Chrome", Family::Chromium),
    ("msedge.exe", "msedge.exe", "Edge", Family::Chromium),
    ("brave.exe", "brave.exe", "Brave", Family::Chromium),
    ("opera.exe", "opera.exe", "Opera", Family::Chromium),
    ("vivaldi.exe", "vivaldi.exe", "Vivaldi", Family::Chromium),
    ("comet.exe", "comet.exe", "Comet", Family::Chromium),
    ("TheBrowserCompany.Arc_", "TheBrowserCompany.Arc_", "Arc", Family::Chromium),
];

/// What asking a browser for its tabs gave.
#[derive(Debug, Clone, PartialEq)]
pub enum Tabs {
    Urls(Vec<String>),
    /// Tab titles from the tab strip, at least one of them a call's (`title_call`). Windows only: a call in a tab
    /// that isn't in front has no address to read there, only its title.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    Titles(Vec<String>),
    /// The user said no to Automation for this browser (macOS; Windows has no such permission).
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    Denied,
    /// No answer in time (the permission prompt is still up) or another failure.
    Unknown,
}

/// What a browser on the mic is, with tab detection on.
#[derive(Debug, Clone, PartialEq)]
pub enum BrowserCall {
    /// A call tab is open: treat it as that call ("Google Meet"), Always allowed.
    Call(&'static str),
    /// Every browser answered and none has a call tab: YouTube, a voice recorder site. Don't trigger.
    NotACall,
    /// Couldn't tell (permission refused or pending, or a browser that can't be asked): ask, as without detection.
    Unsure,
}

pub fn classify(lookups: &[Tabs]) -> BrowserCall {
    if let Some(label) = lookups.iter().filter_map(|t| if let Tabs::Urls(u) = t { u.iter().find_map(|u| web_call(u)) } else { None }).next() {
        return BrowserCall::Call(label);
    }
    if let Some(label) = lookups.iter().filter_map(|t| if let Tabs::Titles(v) = t { v.iter().find_map(|t| title_call(t)) } else { None }).next() {
        return BrowserCall::Call(label);
    }
    if !lookups.is_empty() && lookups.iter().all(|t| matches!(t, Tabs::Urls(_))) {
        return BrowserCall::NotACall;
    }
    BrowserCall::Unsure
}

pub fn browser_for(mic_user: &str) -> Option<(&'static str, &'static str, Family)> {
    BROWSERS.iter().find(|(p, ..)| mic_user == *p || mic_user.starts_with(&format!("{p}.")) || (p.ends_with('_') && mic_user.starts_with(p))).map(|(_, app, name, f)| (*app, *name, *f))
}

/// The call a tab is in, by its URL: "Google Meet", "Zoom" or "Microsoft Teams". Lobby and home pages aren't calls.
pub fn web_call(url: &str) -> Option<&'static str> {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_lowercase();
    let path = path.split(['?', '#']).next().unwrap_or("");
    let seg: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let is = |h: &str| host == h || host.ends_with(&format!(".{h}"));
    if host == "meet.google.com" {
        // Meeting codes look like abc-defg-hij; the landing page and /new aren't calls.
        let code = seg.first().copied().unwrap_or("");
        let parts: Vec<&str> = code.split('-').collect();
        let looks_like_code = parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase()));
        return looks_like_code.then_some("Google Meet");
    }
    if is("zoom.us") || is("zoom.com") {
        // The web client: /wc/<id>/join, /wc/join/<id>, /wc/<id>/start.
        return (seg.first() == Some(&"wc") && seg.len() >= 2).then_some("Zoom");
    }
    if host == "teams.microsoft.com" || host == "teams.live.com" || host == "teams.cloud.microsoft" {
        // Meetings run inside the app shell, so any Teams tab holding the mic is the call.
        return Some("Microsoft Teams");
    }
    None
}

/// The call a tab is in, by its title in the tab strip (Windows reads titles for the tabs that aren't in front).
/// Only the shapes the call pages themselves use: "Meet - abc-defg-hij" or "Meet – Weekly sync" (Google Meet),
/// "… | Microsoft Teams" (any Teams page, as with the address), "Zoom Meeting" / "Zoom Webinar" (the Zoom web
/// client). Chromium adds its own notes after the title (" - Memory usage - 22.4 MB", " - Audio playing"), so a
/// match may be followed by " - …". The caller counts a title only for a browser that is using the mic.
pub fn title_call(title: &str) -> Option<&'static str> {
    let t = title.trim();
    // Where a page's own title may end: the end, or the start of a note Chromium appended.
    let ends_here = |rest: &str| rest.is_empty() || rest.starts_with(" - ");
    for dash in ["Meet - ", "Meet \u{2013} ", "Meet \u{2014} "] {
        if let Some(rest) = t.strip_prefix(dash) {
            // The meeting code or name, which Chromium's notes never are.
            let name = rest.split(" - ").next().unwrap_or("").trim();
            if !name.is_empty() && !CHROMIUM_NOTES.iter().any(|n| name.starts_with(n)) {
                return Some("Google Meet");
            }
        }
    }
    const TEAMS: &str = "| Microsoft Teams";
    if let Some(i) = t.find(TEAMS) {
        if ends_here(&t[i + TEAMS.len()..]) {
            return Some("Microsoft Teams");
        }
    }
    for zoom in ["Zoom Meeting", "Zoom Webinar"] {
        if t.strip_prefix(zoom).is_some_and(ends_here) {
            return Some("Zoom");
        }
    }
    None
}

/// The notes Chromium appends to a tab's accessible name (English UI), so "Meet - Memory usage - 20 MB" (the
/// Meet home page) isn't read as a meeting called "Memory usage".
const CHROMIUM_NOTES: &[&str] = &["Memory usage", "Audio playing", "Audio muted", "Camera", "Microphone", "Network error", "Crashed", "Pinned", "Sharing"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_tabs_are_recognized_by_title() {
        assert_eq!(title_call("Meet - abc-defg-hij"), Some("Google Meet"));
        assert_eq!(title_call("Meet \u{2013} Weekly sync"), Some("Google Meet"));
        assert_eq!(title_call("Meet - abc-defg-hij - Memory usage - 19.2 MB"), Some("Google Meet"));
        assert_eq!(title_call("Meet - abc-defg-hij - Camera or microphone recording"), Some("Google Meet"));
        assert_eq!(title_call("Meeting with Ana | Microsoft Teams"), Some("Microsoft Teams"));
        assert_eq!(title_call("Chat | Microsoft Teams - Memory usage - 80 MB"), Some("Microsoft Teams"));
        assert_eq!(title_call("Zoom Meeting"), Some("Zoom"));
        assert_eq!(title_call("Zoom Webinar - Audio playing"), Some("Zoom"));
    }

    #[test]
    fn other_titles_are_not_calls() {
        for t in [
            "Meet",
            "Google Meet",
            "Meet - ",
            "Meet - Memory usage - 20 MB",
            "Meeting notes - Google Docs",
            "Meetup: Toronto Rust",
            "Microsoft Teams | Microsoft Learn",
            "News | Microsoft Teams Blog",
            "Zoom Workplace",
            "Zoom Meetings pricing",
            "Example Domain - Memory usage - 22.4 MB",
            "",
        ] {
            assert_eq!(title_call(t), None, "{t}");
        }
    }

    #[test]
    fn a_call_title_makes_it_a_call() {
        let titles = |t: &[&str]| Tabs::Titles(t.iter().map(|s| s.to_string()).collect());
        assert_eq!(classify(&[Tabs::Unknown, titles(&["Example Domain", "Meet - abc-defg-hij"])]), BrowserCall::Call("Google Meet"));
        // An address in front wins over a title behind it.
        assert_eq!(classify(&[titles(&["Zoom Meeting"]), Tabs::Urls(vec!["https://teams.microsoft.com/v2/".into()])]), BrowserCall::Call("Microsoft Teams"));
        assert_eq!(classify(&[titles(&["Example Domain"])]), BrowserCall::Unsure);
    }

    #[test]
    fn calls_are_recognized_by_url() {
        assert_eq!(web_call("https://meet.google.com/abc-defg-hij"), Some("Google Meet"));
        assert_eq!(web_call("https://meet.google.com/abc-defg-hij?authuser=1"), Some("Google Meet"));
        assert_eq!(web_call("https://us02web.zoom.us/wc/81234567890/join?pwd=x"), Some("Zoom"));
        assert_eq!(web_call("https://app.zoom.us/wc/join/81234567890"), Some("Zoom"));
        assert_eq!(web_call("https://teams.microsoft.com/v2/"), Some("Microsoft Teams"));
        assert_eq!(web_call("https://teams.live.com/light-meetings/launch?p=x"), Some("Microsoft Teams"));
    }

    #[test]
    fn everything_else_is_not_a_call() {
        for u in [
            "https://meet.google.com/",
            "https://meet.google.com/landing",
            "https://meet.google.com/new",
            "https://www.youtube.com/watch?v=abc",
            "https://zoom.us/signin",
            "https://us02web.zoom.us/j/81234567890",
            "https://notmeet.google.com.evil.example/abc-defg-hij",
            "https://evilzoom.us/wc/123/join",
            "about:blank",
        ] {
            assert_eq!(web_call(u), None, "{u}");
        }
    }

    #[test]
    fn a_browser_on_the_mic_is_a_call_only_with_a_call_tab() {
        let urls = |u: &[&str]| Tabs::Urls(u.iter().map(|s| s.to_string()).collect());
        assert_eq!(classify(&[urls(&["https://www.youtube.com/", "https://meet.google.com/abc-defg-hij"])]), BrowserCall::Call("Google Meet"));
        assert_eq!(classify(&[urls(&["https://www.youtube.com/watch?v=x"])]), BrowserCall::NotACall);
        // One browser refused, another has the call: it's the call.
        assert_eq!(classify(&[Tabs::Denied, urls(&["https://app.zoom.us/wc/join/1"])]), BrowserCall::Call("Zoom"));
        assert_eq!(classify(&[Tabs::Denied]), BrowserCall::Unsure);
        assert_eq!(classify(&[urls(&[]), Tabs::Unknown]), BrowserCall::Unsure);
        // Firefox (can't be asked): nothing to look at.
        assert_eq!(classify(&[]), BrowserCall::Unsure);
    }

    #[test]
    fn helpers_map_to_their_browser() {
        assert_eq!(browser_for("com.google.Chrome.helper"), Some(("com.google.Chrome", "Chrome", Family::Chromium)));
        assert_eq!(browser_for("company.thebrowser.Browser.helper"), Some(("company.thebrowser.Browser", "Arc", Family::Chromium)));
        assert_eq!(browser_for("com.apple.WebKit.GPU"), Some(("com.apple.Safari", "Safari", Family::Safari)));
        assert_eq!(browser_for("org.mozilla.firefox"), None);
        assert_eq!(browser_for("com.google.ChromeX"), None);
    }

    #[test]
    fn windows_browsers_map_to_themselves() {
        assert_eq!(browser_for("chrome.exe"), Some(("chrome.exe", "Chrome", Family::Chromium)));
        assert_eq!(browser_for("msedge.exe"), Some(("msedge.exe", "Edge", Family::Chromium)));
        assert_eq!(browser_for("TheBrowserCompany.Arc_ttt1ap7aakyb4"), Some(("TheBrowserCompany.Arc_", "Arc", Family::Chromium)));
        assert_eq!(browser_for("firefox.exe"), None);
    }
}
