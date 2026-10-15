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
        Event { title: title.into(), start_ms: start_min * MIN, end_ms: end_min * MIN, all_day: false, attendees: vec![] }
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
