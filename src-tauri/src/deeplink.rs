//! `talaan://` links (Windows). A "Summary ready" toast opens `talaan://meeting/<id>`: Windows starts
//! `talaan.exe "talaan://meeting/<id>"` from the protocol the installer registers, even from Action Center after
//! the banner is gone, and the single-instance plugin hands that argument to the running Talaan. The "Talaan
//! {version} is ready" toast opens `talaan://update` the same way (the main window, with the update banner).
//! Pure: no Tauri.
#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

/// The URI scheme (registered under `HKCU\Software\Classes\talaan` by src-tauri/windows/installer-hooks.nsh).
pub const SCHEME: &str = "talaan";

/// The link that opens a meeting.
pub fn meeting_uri(id: &str) -> String {
    format!("{SCHEME}://meeting/{id}")
}

/// The notification id (and macOS request identifier) of the "Talaan {version} is ready" notification. Never a
/// meeting id: those start with a date.
pub const UPDATE_NOTICE: &str = "talaan-update";

/// The link that opens the main window on the update banner.
pub fn update_uri() -> String {
    format!("{SCHEME}://update")
}

/// The link a notification opens when clicked: the update banner for `UPDATE_NOTICE`, otherwise that meeting.
pub fn uri_for(notice_id: &str) -> String {
    if notice_id == UPDATE_NOTICE { update_uri() } else { meeting_uri(notice_id) }
}

/// What a `talaan://` link asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Link {
    Meeting(String),
    Update,
}

/// What `arg` opens, when it's one of Talaan's links: `talaan://meeting/<id>` or `talaan://update` (scheme and
/// host in any case, an optional trailing slash). Anything else is ignored.
pub fn link(arg: &str) -> Option<Link> {
    if let Some(id) = meeting_id(arg) {
        return Some(Link::Meeting(id));
    }
    let arg = arg.trim().trim_matches('"');
    let (scheme, rest) = arg.split_once("://")?;
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    (scheme.eq_ignore_ascii_case(SCHEME) && rest.eq_ignore_ascii_case("update")).then_some(Link::Update)
}

/// The first Talaan link among a process's arguments (the exe path comes first and is skipped by the check).
pub fn link_in_args<S: AsRef<str>>(args: &[S]) -> Option<Link> {
    args.iter().find_map(|a| link(a.as_ref()))
}

/// The meeting a link opens, when `arg` is one: `talaan://meeting/<id>` (scheme and host in any case, an optional
/// trailing slash). A meeting id is a folder name like `2026-10-14_1430_ab12`, so only letters, digits, `-` and `_`
/// are accepted: a link from anywhere else (a web page can open `talaan://` too) can't name a path.
pub fn meeting_id(arg: &str) -> Option<String> {
    let arg = arg.trim().trim_matches('"');
    let (scheme, rest) = arg.split_once("://")?;
    if !scheme.eq_ignore_ascii_case(SCHEME) {
        return None;
    }
    let (host, id) = rest.split_once('/')?;
    if !host.eq_ignore_ascii_case("meeting") {
        return None;
    }
    let id = id.strip_suffix('/').unwrap_or(id);
    let ok = !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    ok.then(|| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meeting_links_round_trip() {
        let id = "2026-10-14_1430_ab12";
        assert_eq!(meeting_uri(id), "talaan://meeting/2026-10-14_1430_ab12");
        assert_eq!(meeting_id(&meeting_uri(id)).as_deref(), Some(id));
        assert_eq!(meeting_id("TALAAN://Meeting/abc/").as_deref(), Some("abc"));
        assert_eq!(meeting_id("\"talaan://meeting/abc\"").as_deref(), Some("abc"));
    }

    #[test]
    fn anything_else_is_ignored() {
        for a in [
            "",
            "talaan.exe",
            r"C:\Users\a\AppData\Local\Talaan\talaan.exe",
            "talaan://",
            "talaan://meeting/",
            "talaan://meeting",
            "talaan://settings/general",
            "https://meeting/abc",
            "talaan://meeting/../../Windows",
            "talaan://meeting/a/b",
            "talaan://meeting/abc?x=1",
            "talaan://meeting/a%2Fb",
        ] {
            assert_eq!(meeting_id(a), None, "{a}");
        }
        assert_eq!(meeting_id(&format!("talaan://meeting/{}", "a".repeat(65))), None);
    }

    #[test]
    fn the_link_is_found_among_the_arguments() {
        assert_eq!(link_in_args(&[r"C:\Talaan\talaan.exe", "talaan://meeting/abc"]), Some(Link::Meeting("abc".into())));
        assert_eq!(link_in_args(&[r"C:\Talaan\talaan.exe"]), None);
        assert_eq!(link_in_args::<&str>(&[]), None);
    }

    #[test]
    fn the_update_link_opens_the_banner() {
        assert_eq!(update_uri(), "talaan://update");
        assert_eq!(uri_for(UPDATE_NOTICE), "talaan://update");
        assert_eq!(uri_for("2026-10-14_1430_ab12"), "talaan://meeting/2026-10-14_1430_ab12");
        assert_eq!(link("talaan://update"), Some(Link::Update));
        assert_eq!(link("\"TALAAN://Update/\""), Some(Link::Update));
        assert_eq!(link("talaan://meeting/abc"), Some(Link::Meeting("abc".into())));
        for a in ["talaan://update/x", "talaan://updates", "talaan://update?v=1", "https://update", "talaan://", "update"] {
            assert_eq!(link(a), None, "{a}");
        }
        assert_eq!(link_in_args(&[r"C:\Talaan\talaan.exe", "talaan://update"]), Some(Link::Update));
        assert_eq!(link_in_args(&[r"C:\Talaan\talaan.exe"]), None);
    }

    /// The installer registers the same scheme the app builds links with.
    #[test]
    fn the_installer_registers_this_scheme() {
        let nsh = include_str!("../windows/installer-hooks.nsh");
        assert!(nsh.contains(&format!(r#""Software\Classes\{SCHEME}\shell\open\command""#)), "register");
        assert!(nsh.contains(&format!(r#"DeleteRegKey HKCU "Software\Classes\{SCHEME}""#)), "unregister");
    }
}
