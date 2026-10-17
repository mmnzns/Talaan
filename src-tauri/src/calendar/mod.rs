//! Calendar link: which calendar event a recording belongs to (pure; the system calendar is read in
//! `platform::<os>::calendar`).

// The Windows calendar downloads an iCal address; macOS uses EventKit, so these stay unused there.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub mod fetch;
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub mod ics;

/// An event as read from the system calendar. Times are Unix milliseconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub title: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub all_day: bool,
    /// Other people invited (the user's own entry is left out).
    pub attendees: Vec<String>,
    /// The email addresses of those people, when the calendar has them (for "Email this summary").
    pub emails: Vec<String>,
    /// The Zoom, Google Meet or Microsoft Teams link in the invite, if any.
    pub join_url: Option<String>,
}

/// How far ahead the menu bar shows the next meeting.
pub const NEXT_MS: i64 = 60 * 60_000;

/// The meeting the menu bar offers to record: the one a recording started now would belong to (`pick`), else the
/// next one starting within the hour.
pub fn upcoming(events: &[Event], now_ms: i64) -> Option<&Event> {
    pick(events, now_ms).or_else(|| {
        events
            .iter()
            .filter(|e| !e.all_day && !e.title.trim().is_empty() && e.end_ms > e.start_ms)
            .filter(|e| e.start_ms > now_ms && e.start_ms - now_ms <= NEXT_MS)
            .min_by_key(|e| e.start_ms)
    })
}

/// The call app a meeting link opens: (app id, name) as call detection names them. `None` for any other link.
pub fn meeting_app(url: &str) -> Option<(&'static str, &'static str)> {
    let url = url.trim();
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    let path = path.to_ascii_lowercase();
    let under = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    if under("zoom.us") || under("zoom.com") {
        return ["j/", "my/", "w/", "s/"].iter().any(|p| path.starts_with(p)).then_some(("zoom", "Zoom"));
    }
    if host == "meet.google.com" {
        let code = path.split(['?', '#']).next().unwrap_or("");
        let room = code.len() >= 10 && code.contains('-') && code.chars().all(|c| c.is_ascii_alphabetic() || c == '-');
        return room.then_some(("browser", "Google Meet"));
    }
    if host == "teams.microsoft.com" || host == "teams.live.com" || host == "teams.cloud.microsoft" {
        return (path.starts_with("l/meetup-join") || path.starts_with("meet/")).then_some(("teams", "Microsoft Teams"));
    }
    None
}

/// The first Zoom, Meet or Teams link in an invite's fields (its URL, location, notes), in the order given.
pub fn join_link(fields: &[&str]) -> Option<String> {
    for text in fields {
        let mut rest = *text;
        while let Some(at) = rest.find("http") {
            let tail = &rest[at..];
            let end = tail.find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\'' | ')' | ']' | ',' | ';')).unwrap_or(tail.len());
            let url = tail[..end].trim_end_matches(['.', ':']);
            if meeting_app(url).is_some() {
                return Some(url.to_string());
            }
            rest = &tail[4..];
        }
    }
    None
}

/// "mailto:ana@example.com" (any case, with `?` options) -> "ana@example.com". `None` when it isn't an address.
pub fn email_of(uri: &str) -> Option<String> {
    let v = uri.trim();
    let addr = v.get(..7).filter(|p| p.eq_ignore_ascii_case("mailto:")).map_or(v, |_| &v[7..]);
    let addr = addr.split('?').next().unwrap_or("").trim();
    let ok = !addr.contains(char::is_whitespace) && addr.split_once('@').is_some_and(|(user, domain)| !user.is_empty() && domain.contains('.'));
    ok.then(|| addr.to_string())
}

/// How early a call may start before its event and still belong to it.
pub const EARLY_MS: i64 = 10 * 60_000;

/// The event a recording that started at `at_ms` belongs to: one in progress (the shortest, when several
/// overlap, so a 30-minute call wins over an afternoon block), else the next one starting within 10 minutes.
/// All-day events and untitled ones never count.
pub fn pick(events: &[Event], at_ms: i64) -> Option<&Event> {
    let usable = events.iter().filter(|e| !e.all_day && !e.title.trim().is_empty() && e.end_ms > e.start_ms);
    let current = usable.clone().filter(|e| e.start_ms <= at_ms && at_ms < e.end_ms).min_by_key(|e| e.end_ms - e.start_ms);
    current.or_else(|| usable.filter(|e| e.start_ms > at_ms && e.start_ms - at_ms <= EARLY_MS).min_by_key(|e| e.start_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;

    fn ev(title: &str, start_min: i64, end_min: i64) -> Event {
        Event { title: title.into(), start_ms: start_min * MIN, end_ms: end_min * MIN, all_day: false, attendees: vec![], emails: vec![], join_url: None }
    }

    #[test]
    fn the_menu_bar_shows_the_current_meeting_or_the_next_one_within_the_hour() {
        let e = [ev("Standup", 0, 15), ev("Roadmap", 60, 90), ev("Later", 200, 230)];
        assert_eq!(upcoming(&e, 5 * MIN).unwrap().title, "Standup");
        assert_eq!(upcoming(&e, 16 * MIN).unwrap().title, "Roadmap");
        assert_eq!(upcoming(&e, 75 * MIN).unwrap().title, "Roadmap");
        assert!(upcoming(&e, 95 * MIN).is_none(), "Later starts in 105 minutes");
        assert_eq!(upcoming(&e, 141 * MIN).unwrap().title, "Later");
        let mut all_day = ev("Holiday", 20, 50);
        all_day.all_day = true;
        assert!(upcoming(&[all_day, ev("  ", 20, 50)], 0).is_none());
    }

    #[test]
    fn meeting_links_name_their_app_and_other_links_do_not() {
        assert_eq!(meeting_app("https://us02web.zoom.us/j/81234567890?pwd=abc"), Some(("zoom", "Zoom")));
        assert_eq!(meeting_app("https://zoom.us/my/ana.reyes"), Some(("zoom", "Zoom")));
        assert_eq!(meeting_app("https://meet.google.com/abc-defg-hij"), Some(("browser", "Google Meet")));
        assert_eq!(meeting_app("https://teams.microsoft.com/l/meetup-join/19%3ameeting_x/0?context=y"), Some(("teams", "Microsoft Teams")));
        assert_eq!(meeting_app("https://teams.live.com/meet/9876543210"), Some(("teams", "Microsoft Teams")));
        for other in ["https://zoom.us/signin", "https://meet.google.com/", "https://evilzoom.us/j/1", "https://teams.microsoft.com/v2/", "https://example.com/j/1", "zoom.us/j/1"] {
            assert_eq!(meeting_app(other), None, "{other}");
        }
    }

    #[test]
    fn the_join_link_is_found_in_the_location_or_notes() {
        let notes = "Join Zoom Meeting\n<https://us02web.zoom.us/j/81234567890?pwd=x>.\nhttps://example.com/agenda";
        assert_eq!(join_link(&["", "Room 4", notes]).as_deref(), Some("https://us02web.zoom.us/j/81234567890?pwd=x"));
        assert_eq!(join_link(&["https://meet.google.com/abc-defg-hij", notes]).as_deref(), Some("https://meet.google.com/abc-defg-hij"));
        assert_eq!(join_link(&["Call me at http", "https://example.com"]), None);
    }

    #[test]
    fn emails_come_from_mailto_addresses() {
        assert_eq!(email_of("mailto:ana@example.com").as_deref(), Some("ana@example.com"));
        assert_eq!(email_of("MAILTO:Sam.Lee@Example.org?subject=x").as_deref(), Some("Sam.Lee@Example.org"));
        assert_eq!(email_of("sam@example.com").as_deref(), Some("sam@example.com"));
        for bad in ["tel:+15555550100", "mailto:", "mailto:nobody", "mailto:a b@example.com", "urn:uuid:123"] {
            assert_eq!(email_of(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_event_in_progress_wins() {
        let e = [ev("Standup", 0, 15), ev("Roadmap", 30, 60)];
        assert_eq!(pick(&e, 5 * MIN).unwrap().title, "Standup");
        assert_eq!(pick(&e, 45 * MIN).unwrap().title, "Roadmap");
    }

    #[test]
    fn a_call_started_a_few_minutes_early_gets_the_next_event() {
        let e = [ev("Roadmap", 30, 60)];
        assert_eq!(pick(&e, 22 * MIN).unwrap().title, "Roadmap");
        assert!(pick(&e, 15 * MIN).is_none());
        assert!(pick(&e, 61 * MIN).is_none());
    }

    #[test]
    fn the_shortest_overlapping_event_wins_and_all_day_is_ignored() {
        let mut holiday = ev("Holiday", 0, 24 * 60);
        holiday.all_day = true;
        let e = [holiday, ev("Focus block", 0, 240), ev("Client call", 60, 90), ev("", 60, 70)];
        assert_eq!(pick(&e, 65 * MIN).unwrap().title, "Client call");
        assert_eq!(pick(&e, 100 * MIN).unwrap().title, "Focus block");
    }
}
