//! Events from an iCalendar (.ics) file, such as a calendar's private iCal address (Google, Outlook,
//! iCloud). Pure: text in, events out, no network or OS calls. Used by the Windows calendar; compiles everywhere.

//!
//! Times: `Z` is UTC; `TZID=` is an IANA name (Google, iCloud) or a Windows zone name (Outlook, e.g. "Eastern
//! Standard Time", mapped through `WINDOWS_ZONES`), or a custom id whose VTIMEZONE names its IANA zone in
//! `X-LIC-LOCATION`; anything else, and floating times, are local. `VALUE=DATE` (all-day) starts at local midnight.

use super::Event;
use chrono::{DateTime, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use icalendar::parser::{read_calendar, unfold, Component, Property};
use rrule::{RRule, Tz, Unvalidated};
use std::collections::HashMap;

/// At most this many occurrences of one recurring event, so a pathological rule can't hang or flood.
pub const MAX_OCCURRENCES: usize = 1000;
/// At most this many steps through one rule (including those before the window), for the same reason. A daily
/// meeting started 30 years ago is about 11,000 steps.
const MAX_STEPS: usize = 100_000;

/// The events overlapping [`from_ms`, `to_ms`), with recurring ones expanded into occurrences. `me` holds the
/// user's own email addresses, which are left out of the attendees. A file that can't be read gives no events.
pub fn parse(ics: &str, from_ms: i64, to_ms: i64, me: &[String]) -> Vec<Event> {
    let text = unfold(ics);
    let Ok(cal) = read_calendar(&text) else { return vec![] };
    let zones = vtimezone_locations(&cal.components);
    let vevents: Vec<&Component> = cal.components.iter().filter(|c| c.name.as_str().eq_ignore_ascii_case("VEVENT")).collect();

    // Occurrences that an override (RECURRENCE-ID) replaces or cancels, by UID.
    let mut replaced: HashMap<String, Vec<DateTime<Tz>>> = HashMap::new();
    for ev in &vevents {
        if let Some(rid) = prop(ev, "RECURRENCE-ID").and_then(|p| when(p, &zones)) {
            replaced.entry(uid(ev)).or_default().push(rid.at);
        }
    }

    let mut out = Vec::new();
    for ev in vevents {
        if prop(ev, "STATUS").is_some_and(|p| p.val.as_str().trim().eq_ignore_ascii_case("CANCELLED")) {
            continue;
        }
        let Some(start) = prop(ev, "DTSTART").and_then(|p| when(p, &zones)) else { continue };
        let duration_ms = duration_ms(ev, &start, &zones);
        let title = prop(ev, "SUMMARY").map(|p| p.val.clone().unescape_text().as_str().trim().to_string()).unwrap_or_default();
        let attendees = attendees(ev, me);
        let is_override = prop(ev, "RECURRENCE-ID").is_some();

        let starts = match prop(ev, "RRULE").filter(|_| !is_override) {
            Some(rule) => {
                let mut skip = times(ev, "EXDATE", &zones);
                skip.extend(replaced.get(&uid(ev)).cloned().unwrap_or_default());
                expand(rule.val.as_str(), start.at, times(ev, "RDATE", &zones), skip, duration_ms, from_ms, to_ms)
            }
            None => vec![start.at.timestamp_millis()],
        };
        for s in starts.into_iter().filter(|&s| s < to_ms && s + duration_ms > from_ms) {
            out.push(Event { title: title.clone(), start_ms: s, end_ms: s + duration_ms, all_day: start.all_day, attendees: attendees.clone() });
        }
    }
    out.sort_by_key(|e| e.start_ms);
    out
}

/// A parsed DTSTART-like value.
struct When {
    at: DateTime<Tz>,
    all_day: bool,
}

fn prop<'a>(c: &'a Component, name: &str) -> Option<&'a Property<'a>> {
    c.properties.iter().find(|p| p.name.as_str().eq_ignore_ascii_case(name))
}

fn param<'a>(p: &'a Property, key: &str) -> Option<&'a str> {
    p.params.iter().find(|x| x.key.as_str().eq_ignore_ascii_case(key)).and_then(|x| x.val.as_ref()).map(|v| v.as_str().trim().trim_matches('"'))
}

fn uid(c: &Component) -> String {
    prop(c, "UID").map(|p| p.val.as_str().trim().to_string()).unwrap_or_default()
}

/// Custom TZIDs whose VTIMEZONE says which IANA zone they are (`X-LIC-LOCATION`, written by Thunderbird and others).
fn vtimezone_locations(components: &[Component]) -> HashMap<String, Tz> {
    components
        .iter()
        .filter(|c| c.name.as_str().eq_ignore_ascii_case("VTIMEZONE"))
        .filter_map(|c| {
            let id = prop(c, "TZID")?.val.as_str().trim().trim_matches('"').to_string();
            let tz = iana(prop(c, "X-LIC-LOCATION")?.val.as_str().trim())?;
            Some((id, tz))
        })
        .collect()
}

fn iana(name: &str) -> Option<Tz> {
    name.parse::<chrono_tz::Tz>().ok().map(Tz::Tz)
}

/// The zone a TZID names; local time when it can't be told.
fn zone(tzid: &str, vtimezones: &HashMap<String, Tz>) -> Tz {
    let id = tzid.trim().trim_matches('"');
    iana(id)
        .or_else(|| vtimezones.get(id).copied())
        .or_else(|| WINDOWS_ZONES.iter().find(|(w, _)| w.eq_ignore_ascii_case(id)).and_then(|(_, z)| iana(z)))
        // Prefixed ids such as "/mozilla.org/20050126_1/America/New_York".
        .or_else(|| id.match_indices('/').find_map(|(i, _)| iana(&id[i + 1..])))
        .unwrap_or(Tz::LOCAL)
}

/// The instant a wall-clock time names in `tz`: the earlier one when the clocks go back, and an hour later when it
/// falls in the gap when they go forward.
fn in_zone(tz: Tz, n: NaiveDateTime) -> Option<DateTime<Tz>> {
    match tz.from_local_datetime(&n) {
        LocalResult::Single(d) | LocalResult::Ambiguous(d, _) => Some(d),
        LocalResult::None => tz.from_local_datetime(&(n + chrono::Duration::hours(1))).earliest(),
    }
}

fn parse_time(value: &str, tzid: Option<&str>, date_param: bool, vtimezones: &HashMap<String, Tz>) -> Option<When> {
    let v = value.trim();
    if date_param || v.len() == 8 {
        let d = NaiveDate::parse_from_str(v, "%Y%m%d").ok()?;
        return Some(When { at: in_zone(Tz::LOCAL, d.and_hms_opt(0, 0, 0)?)?, all_day: true });
    }
    let (v, utc) = match v.strip_suffix(['Z', 'z']) {
        Some(rest) => (rest, true),
        None => (v, false),
    };
    let n = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()?;
    let tz = if utc { Tz::UTC } else { tzid.map(|t| zone(t, vtimezones)).unwrap_or(Tz::LOCAL) };
    Some(When { at: in_zone(tz, n)?, all_day: false })
}

fn when(p: &Property, vtimezones: &HashMap<String, Tz>) -> Option<When> {
    let date = param(p, "VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE"));
    parse_time(p.val.as_str(), param(p, "TZID"), date, vtimezones)
}

/// Every value of a list property that may repeat (EXDATE, RDATE), e.g. `EXDATE;TZID=…:20261019T093000,20261026T093000`.
fn times(c: &Component, name: &str, vtimezones: &HashMap<String, Tz>) -> Vec<DateTime<Tz>> {
    let date = |p: &Property| param(p, "VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE"));
    c.properties
        .iter()
        .filter(|p| p.name.as_str().eq_ignore_ascii_case(name))
        .flat_map(|p| p.val.as_str().split(',').filter_map(move |v| parse_time(v, param(p, "TZID"), date(p), vtimezones)).map(|w| w.at))
        .collect()
}

/// DTEND − DTSTART, else DURATION, else a day for all-day events and nothing for the rest (RFC 5545).
fn duration_ms(ev: &Component, start: &When, vtimezones: &HashMap<String, Tz>) -> i64 {
    if let Some(end) = prop(ev, "DTEND").and_then(|p| when(p, vtimezones)) {
        return (end.at.timestamp_millis() - start.at.timestamp_millis()).max(0);
    }
    if let Some(d) = prop(ev, "DURATION").and_then(|p| iso_duration_ms(p.val.as_str())) {
        return d.max(0);
    }
    if start.all_day { 86_400_000 } else { 0 }
}

/// An RFC 5545 duration such as `PT45M`, `P1D`, `P1W` or `-PT15M`.
fn iso_duration_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    let (sign, s) = match s.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, s.strip_prefix('+').unwrap_or(s)),
    };
    let s = s.strip_prefix(['P', 'p'])?;
    let (mut total, mut num, mut in_time) = (0i64, String::new(), false);
    for ch in s.chars() {
        match ch.to_ascii_uppercase() {
            'T' => in_time = true,
            c if c.is_ascii_digit() => num.push(c),
            unit => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += n * match (unit, in_time) {
                    ('W', false) => 7 * 86_400_000,
                    ('D', false) => 86_400_000,
                    ('H', true) => 3_600_000,
                    ('M', true) => 60_000,
                    ('S', true) => 1000,
                    _ => return None,
                };
            }
        }
    }
    num.is_empty().then_some(sign * total)
}

/// Rewrites an `UNTIL` given without `Z` into UTC: RFC 5545 requires UTC when DTSTART has a zone, but some
/// exporters write it in the event's zone, which the rrule crate then refuses. A date-only UNTIL includes that day.
fn until_in_utc(rule: &str, tz: Tz) -> String {
    rule.split(';')
        .map(|part| match part.split_once('=') {
            Some((k, v)) if k.trim().eq_ignore_ascii_case("UNTIL") && !v.trim().ends_with(['Z', 'z']) => {
                let v = v.trim();
                let naive = if v.len() == 8 {
                    NaiveDate::parse_from_str(v, "%Y%m%d").ok().and_then(|d| d.and_hms_opt(23, 59, 59))
                } else {
                    NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()
                };
                match naive.and_then(|n| in_zone(tz, n)) {
                    Some(d) => format!("UNTIL={}", d.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ")),
                    None => part.to_string(),
                }
            }
            _ => part.to_string(),
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// Start times (ms) of a recurring event's occurrences that can overlap the window. A rule that can't be read
/// leaves just the first occurrence.
fn expand(rule: &str, start: DateTime<Tz>, rdates: Vec<DateTime<Tz>>, exdates: Vec<DateTime<Tz>>, duration_ms: i64, from_ms: i64, to_ms: i64) -> Vec<i64> {
    let Some(set) = until_in_utc(rule, start.timezone()).parse::<RRule<Unvalidated>>().ok().and_then(|r| r.build(start).ok()) else {
        return if exdates.contains(&start) { vec![] } else { vec![start.timestamp_millis()] };
    };
    let set = set.set_rdates(rdates).set_exdates(exdates).limit();
    let mut out = Vec::new();
    for at in set.into_iter().take(MAX_STEPS) {
        let s = at.timestamp_millis();
        if s >= to_ms {
            break;
        }
        if s + duration_ms > from_ms {
            out.push(s);
            if out.len() >= MAX_OCCURRENCES {
                break;
            }
        }
    }
    out
}

/// Who else is invited: the CN, else the address's local part. The user's own addresses and rooms are left out.
fn attendees(ev: &Component, me: &[String]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for p in ev.properties.iter().filter(|p| p.name.as_str().eq_ignore_ascii_case("ATTENDEE")) {
        let value = p.val.as_str().trim();
        let address = if value.len() >= 7 && value[..7].eq_ignore_ascii_case("mailto:") { &value[7..] } else { value };
        if me.iter().any(|m| m.trim().eq_ignore_ascii_case(address)) {
            continue;
        }
        if param(p, "CUTYPE").is_some_and(|t| t.eq_ignore_ascii_case("ROOM") || t.eq_ignore_ascii_case("RESOURCE")) {
            continue;
        }
        let name = param(p, "CN").filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case(address)).map(str::to_string);
        let name = name.or_else(|| address.split('@').next().filter(|l| !l.is_empty()).map(str::to_string));
        if let Some(n) = name.filter(|n| !names.contains(n)) {
            names.push(n);
        }
    }
    names
}

/// Windows time zone names (as Outlook writes them in TZID) → IANA, from CLDR's windowsZones.xml (the "001"
/// territory). The common zones only; an unknown name falls back to local time.
const WINDOWS_ZONES: &[(&str, &str)] = &[
    ("Dateline Standard Time", "Etc/GMT+12"),
    ("UTC-11", "Etc/GMT+11"),
    ("Hawaiian Standard Time", "Pacific/Honolulu"),
    ("Alaskan Standard Time", "America/Anchorage"),
    ("Pacific Standard Time (Mexico)", "America/Tijuana"),
    ("Pacific Standard Time", "America/Los_Angeles"),
    ("US Mountain Standard Time", "America/Phoenix"),
    ("Mountain Standard Time (Mexico)", "America/Mazatlan"),
    ("Mountain Standard Time", "America/Denver"),
    ("Central America Standard Time", "America/Guatemala"),
    ("Central Standard Time (Mexico)", "America/Mexico_City"),
    ("Central Standard Time", "America/Chicago"),
    ("Canada Central Standard Time", "America/Regina"),
    ("SA Pacific Standard Time", "America/Bogota"),
    ("Eastern Standard Time (Mexico)", "America/Cancun"),
    ("Eastern Standard Time", "America/New_York"),
    ("US Eastern Standard Time", "America/Indiana/Indianapolis"),
    ("Venezuela Standard Time", "America/Caracas"),
    ("Atlantic Standard Time", "America/Halifax"),
    ("SA Western Standard Time", "America/La_Paz"),
    ("Pacific SA Standard Time", "America/Santiago"),
    ("Newfoundland Standard Time", "America/St_Johns"),
    ("E. South America Standard Time", "America/Sao_Paulo"),
    ("Argentina Standard Time", "America/Argentina/Buenos_Aires"),
    ("SA Eastern Standard Time", "America/Cayenne"),
    ("Greenland Standard Time", "America/Nuuk"),
    ("UTC", "Etc/UTC"),
    ("Coordinated Universal Time", "Etc/UTC"),
    ("GMT Standard Time", "Europe/London"),
    ("Greenwich Standard Time", "Atlantic/Reykjavik"),
    ("Morocco Standard Time", "Africa/Casablanca"),
    ("W. Europe Standard Time", "Europe/Berlin"),
    ("Central Europe Standard Time", "Europe/Budapest"),
    ("Romance Standard Time", "Europe/Paris"),
    ("Central European Standard Time", "Europe/Warsaw"),
    ("W. Central Africa Standard Time", "Africa/Lagos"),
    ("GTB Standard Time", "Europe/Bucharest"),
    ("E. Europe Standard Time", "Europe/Chisinau"),
    ("FLE Standard Time", "Europe/Kiev"),
    ("Israel Standard Time", "Asia/Jerusalem"),
    ("Egypt Standard Time", "Africa/Cairo"),
    ("South Africa Standard Time", "Africa/Johannesburg"),
    ("Turkey Standard Time", "Europe/Istanbul"),
    ("Arabic Standard Time", "Asia/Baghdad"),
    ("Arab Standard Time", "Asia/Riyadh"),
    ("Russian Standard Time", "Europe/Moscow"),
    ("E. Africa Standard Time", "Africa/Nairobi"),
    ("Iran Standard Time", "Asia/Tehran"),
    ("Arabian Standard Time", "Asia/Dubai"),
    ("Pakistan Standard Time", "Asia/Karachi"),
    ("India Standard Time", "Asia/Kolkata"),
    ("Sri Lanka Standard Time", "Asia/Colombo"),
    ("Nepal Standard Time", "Asia/Kathmandu"),
    ("Central Asia Standard Time", "Asia/Almaty"),
    ("Bangladesh Standard Time", "Asia/Dhaka"),
    ("SE Asia Standard Time", "Asia/Bangkok"),
    ("China Standard Time", "Asia/Shanghai"),
    ("Singapore Standard Time", "Asia/Singapore"),
    ("Taipei Standard Time", "Asia/Taipei"),
    ("W. Australia Standard Time", "Australia/Perth"),
    ("Tokyo Standard Time", "Asia/Tokyo"),
    ("Korea Standard Time", "Asia/Seoul"),
    ("Cen. Australia Standard Time", "Australia/Adelaide"),
    ("AUS Central Standard Time", "Australia/Darwin"),
    ("E. Australia Standard Time", "Australia/Brisbane"),
    ("AUS Eastern Standard Time", "Australia/Sydney"),
    ("West Pacific Standard Time", "Pacific/Port_Moresby"),
    ("New Zealand Standard Time", "Pacific/Auckland"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, NaiveDate, TimeZone, Utc};

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap().timestamp_millis()
    }

    fn local(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        let naive = NaiveDate::from_ymd_opt(y, mo, d).unwrap().and_hms_opt(h, mi, 0).unwrap();
        Local.from_local_datetime(&naive).earliest().unwrap().timestamp_millis()
    }

    /// Wraps VEVENT (and VTIMEZONE) text in a calendar, with CRLF line ends as served.
    fn cal(body: &str) -> String {
        let text = format!("BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//Test//EN\n{}\nEND:VCALENDAR\n", body.trim());
        text.lines().map(|l| l.trim_start()).collect::<Vec<_>>().join("\r\n") + "\r\n"
    }

    fn starts(events: &[Event]) -> Vec<i64> {
        let mut s: Vec<i64> = events.iter().map(|e| e.start_ms).collect();
        s.sort();
        s
    }

    const STANDUP: &str = "
        BEGIN:VEVENT
        UID:standup@test
        SUMMARY:Stand-up
        DTSTART;TZID=America/Toronto:20261005T093000
        DTEND;TZID=America/Toronto:20261005T094500
        RRULE:FREQ=WEEKLY;BYDAY=MO
        END:VEVENT";

    #[test]
    fn a_single_event_in_a_named_zone_has_the_right_utc_times() {
        let ics = cal("
            BEGIN:VEVENT
            UID:one@test
            SUMMARY:Roadmap review
            DTSTART;TZID=America/Toronto:20261020T100000
            DTEND;TZID=America/Toronto:20261020T110000
            END:VEVENT");
        let e = parse(&ics, utc(2026, 10, 19, 0, 0), utc(2026, 10, 22, 0, 0), &[]);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].title, "Roadmap review");
        assert_eq!(e[0].start_ms, utc(2026, 10, 20, 14, 0)); // EDT is UTC-4
        assert_eq!(e[0].end_ms, utc(2026, 10, 20, 15, 0));
        assert!(!e[0].all_day);
    }

    #[test]
    fn utc_times_are_read_as_utc() {
        let ics = cal("
            BEGIN:VEVENT
            UID:z@test
            SUMMARY:Sync
            DTSTART:20261020T170000Z
            DTEND:20261020T173000Z
            END:VEVENT");
        let e = parse(&ics, utc(2026, 10, 20, 0, 0), utc(2026, 10, 21, 0, 0), &[]);
        assert_eq!((e[0].start_ms, e[0].end_ms), (utc(2026, 10, 20, 17, 0), utc(2026, 10, 20, 17, 30)));
    }

    #[test]
    fn floating_times_are_local() {
        let ics = cal("
            BEGIN:VEVENT
            UID:f@test
            SUMMARY:Lunch
            DTSTART:20261020T120000
            DTEND:20261020T130000
            END:VEVENT");
        let e = parse(&ics, utc(2026, 10, 19, 0, 0), utc(2026, 10, 22, 0, 0), &[]);
        assert_eq!((e[0].start_ms, e[0].end_ms), (local(2026, 10, 20, 12, 0), local(2026, 10, 20, 13, 0)));
    }

    #[test]
    fn an_outlook_windows_zone_name_is_mapped() {
        let ics = cal("
            BEGIN:VTIMEZONE
            TZID:Eastern Standard Time
            BEGIN:STANDARD
            DTSTART:16010101T020000
            TZOFFSETFROM:-0400
            TZOFFSETTO:-0500
            RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=1SU;BYMONTH=11
            END:STANDARD
            BEGIN:DAYLIGHT
            DTSTART:16010101T020000
            TZOFFSETFROM:-0500
            TZOFFSETTO:-0400
            RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=2SU;BYMONTH=3
            END:DAYLIGHT
            END:VTIMEZONE
            BEGIN:VEVENT
            UID:outlook@test
            SUMMARY:Client call
            DTSTART;TZID=Eastern Standard Time:20261020T100000
            DTEND;TZID=Eastern Standard Time:20261020T103000
            END:VEVENT
            BEGIN:VEVENT
            UID:outlook-winter@test
            SUMMARY:Winter call
            DTSTART;TZID=\"Eastern Standard Time\":20261210T100000
            DTEND;TZID=\"Eastern Standard Time\":20261210T103000
            END:VEVENT");
        let e = parse(&ics, utc(2026, 10, 1, 0, 0), utc(2027, 1, 1, 0, 0), &[]);
        assert_eq!(starts(&e), vec![utc(2026, 10, 20, 14, 0), utc(2026, 12, 10, 15, 0)]); // EDT, then EST
    }

    #[test]
    fn a_weekly_stand_up_is_expanded_within_the_window_only() {
        let ics = cal(STANDUP);
        let e = parse(&ics, utc(2026, 10, 12, 0, 0), utc(2026, 11, 3, 0, 0), &[]);
        // Oct 5 is before the window; Nov 2 is after DST ends, still 9:30 in Toronto (UTC-5).
        assert_eq!(starts(&e), vec![utc(2026, 10, 12, 13, 30), utc(2026, 10, 19, 13, 30), utc(2026, 10, 26, 13, 30), utc(2026, 11, 2, 14, 30)]);
        assert!(e.iter().all(|e| e.title == "Stand-up" && e.end_ms - e.start_ms == 15 * 60_000));
        assert!(parse(&ics, utc(2026, 9, 1, 0, 0), utc(2026, 10, 5, 13, 0), &[]).is_empty());
    }

    #[test]
    fn an_occurrence_in_progress_at_the_window_start_counts() {
        let e = parse(&cal(STANDUP), utc(2026, 10, 19, 13, 40), utc(2026, 10, 19, 13, 50), &[]);
        assert_eq!(starts(&e), vec![utc(2026, 10, 19, 13, 30)]);
    }

    #[test]
    fn exdate_removes_one_occurrence() {
        let ics = cal(&STANDUP.replace("RRULE:", "EXDATE;TZID=America/Toronto:20261019T093000\nRRULE:"));
        let e = parse(&ics, utc(2026, 10, 12, 0, 0), utc(2026, 10, 27, 0, 0), &[]);
        assert_eq!(starts(&e), vec![utc(2026, 10, 12, 13, 30), utc(2026, 10, 26, 13, 30)]);
    }

    #[test]
    fn a_recurrence_id_override_moves_one_occurrence() {
        let ics = cal(&format!(
            "{STANDUP}
            BEGIN:VEVENT
            UID:standup@test
            RECURRENCE-ID;TZID=America/Toronto:20261019T093000
            SUMMARY:Stand-up (moved)
            DTSTART;TZID=America/Toronto:20261019T110000
            DTEND;TZID=America/Toronto:20261019T111500
            END:VEVENT"
        ));
        let e = parse(&ics, utc(2026, 10, 12, 0, 0), utc(2026, 10, 27, 0, 0), &[]);
        assert_eq!(starts(&e), vec![utc(2026, 10, 12, 13, 30), utc(2026, 10, 19, 15, 0), utc(2026, 10, 26, 13, 30)]);
        assert_eq!(e.iter().find(|e| e.start_ms == utc(2026, 10, 19, 15, 0)).unwrap().title, "Stand-up (moved)");
    }

    #[test]
    fn a_cancelled_occurrence_override_removes_it() {
        let ics = cal(&format!(
            "{STANDUP}
            BEGIN:VEVENT
            UID:standup@test
            RECURRENCE-ID;TZID=America/Toronto:20261019T093000
            STATUS:CANCELLED
            SUMMARY:Stand-up
            DTSTART;TZID=America/Toronto:20261019T093000
            DTEND;TZID=America/Toronto:20261019T094500
            END:VEVENT"
        ));
        let e = parse(&ics, utc(2026, 10, 12, 0, 0), utc(2026, 10, 27, 0, 0), &[]);
        assert_eq!(starts(&e), vec![utc(2026, 10, 12, 13, 30), utc(2026, 10, 26, 13, 30)]);
    }

    #[test]
    fn until_without_utc_marker_still_ends_the_series() {
        // Some exporters write UNTIL in the event's zone, which RFC 5545 doesn't allow; it's read in that zone.
        let ics = cal("
            BEGIN:VEVENT
            UID:daily@test
            SUMMARY:Daily
            DTSTART;TZID=America/Toronto:20261019T093000
            DTEND;TZID=America/Toronto:20261019T100000
            RRULE:FREQ=DAILY;UNTIL=20261021T093000
            END:VEVENT");
        let e = parse(&ics, utc(2026, 10, 1, 0, 0), utc(2026, 11, 1, 0, 0), &[]);
        assert_eq!(starts(&e), vec![utc(2026, 10, 19, 13, 30), utc(2026, 10, 20, 13, 30), utc(2026, 10, 21, 13, 30)]);
    }

    #[test]
    fn an_all_day_event_is_all_day() {
        let ics = cal("
            BEGIN:VEVENT
            UID:holiday@test
            SUMMARY:Thanksgiving
            DTSTART;VALUE=DATE:20261012
            DTEND;VALUE=DATE:20261013
            END:VEVENT");
        let e = parse(&ics, utc(2026, 10, 12, 12, 0), utc(2026, 10, 12, 13, 0), &[]);
        assert_eq!(e.len(), 1);
        assert!(e[0].all_day);
        assert_eq!((e[0].start_ms, e[0].end_ms), (local(2026, 10, 12, 0, 0), local(2026, 10, 13, 0, 0)));
    }

    #[test]
    fn a_cancelled_event_is_skipped() {
        let ics = cal("
            BEGIN:VEVENT
            UID:gone@test
            SUMMARY:Cancelled sync
            STATUS:CANCELLED
            DTSTART:20261020T170000Z
            DTEND:20261020T173000Z
            END:VEVENT");
        assert!(parse(&ics, utc(2026, 10, 20, 0, 0), utc(2026, 10, 21, 0, 0), &[]).is_empty());
    }

    #[test]
    fn attendees_use_their_names_or_the_address_and_leave_the_user_out() {
        let ics = cal("
            BEGIN:VEVENT
            UID:people@test
            SUMMARY:1:1
            DTSTART:20261020T170000Z
            DTEND:20261020T173000Z
            ORGANIZER;CN=Miguel:mailto:miguel@example.com
            ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;CN=Ana Reyes;X-NUM-GUESTS=0:mailto:ana@example.com
            ATTENDEE;CN=\"Lee, Sam\":mailto:sam@example.com
            ATTENDEE;ROLE=REQ-PARTICIPANT:mailto:jordan.k@example.com
            ATTENDEE;CN=miguel@example.com:mailto:Miguel@Example.com
            END:VEVENT");
        let e = parse(&ics, utc(2026, 10, 20, 0, 0), utc(2026, 10, 21, 0, 0), &["miguel@example.com".to_string()]);
        assert_eq!(e[0].attendees, vec!["Ana Reyes", "Lee, Sam", "jordan.k"]);
    }

    #[test]
    fn a_malformed_file_gives_no_events() {
        for bad in [
            "",
            "not a calendar",
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nSUMMARY:x\r\n",
            &cal("BEGIN:VEVENT\nSUMMARY:No start\nEND:VEVENT"),
            &cal("BEGIN:VEVENT\nSUMMARY:Bad start\nDTSTART:2026-10-20\nEND:VEVENT"),
            &cal("BEGIN:VEVENT\nSUMMARY:Bad rule\nDTSTART:20261020T170000Z\nRRULE:FREQ=SOMETIMES\nEND:VEVENT"),
        ] {
            let e = parse(bad, utc(2026, 1, 1, 0, 0), utc(2027, 1, 1, 0, 0), &[]);
            assert!(e.iter().all(|e| e.title != "No start" && e.title != "Bad start"), "{bad:?}");
        }
    }

    #[test]
    fn a_pathological_rule_is_capped_and_returns_quickly() {
        let ics = cal("
            BEGIN:VEVENT
            UID:spam@test
            SUMMARY:Every second
            DTSTART:20000101T000000Z
            DTEND:20000101T000001Z
            RRULE:FREQ=SECONDLY
            END:VEVENT
            BEGIN:VEVENT
            UID:minutely@test
            SUMMARY:Every minute
            DTSTART:20261020T000000Z
            DTEND:20261020T000100Z
            RRULE:FREQ=MINUTELY
            END:VEVENT");
        let t = std::time::Instant::now();
        let e = parse(&ics, utc(2026, 10, 20, 0, 0), utc(2026, 10, 27, 0, 0), &[]);
        assert!(t.elapsed() < std::time::Duration::from_secs(2), "took {:?}", t.elapsed());
        assert_eq!(e.iter().filter(|e| e.title == "Every minute").count(), MAX_OCCURRENCES);
        assert!(e.len() <= 2 * MAX_OCCURRENCES);
    }

    #[test]
    fn every_windows_zone_name_maps_to_a_known_zone() {
        for (windows, name) in WINDOWS_ZONES {
            assert!(iana(name).is_some(), "{windows} -> {name}");
        }
    }

    #[test]
    fn a_custom_tzid_uses_its_vtimezone_location_and_an_unknown_one_is_local() {
        let ics = cal("
            BEGIN:VTIMEZONE
            TZID:Custom Toronto
            X-LIC-LOCATION:America/Toronto
            END:VTIMEZONE
            BEGIN:VEVENT
            UID:a@test
            SUMMARY:Custom
            DTSTART;TZID=Custom Toronto:20261020T100000
            DTEND;TZID=Custom Toronto:20261020T110000
            END:VEVENT
            BEGIN:VEVENT
            UID:b@test
            SUMMARY:Unknown
            DTSTART;TZID=Somewhere Standard Time:20261021T100000
            DTEND;TZID=Somewhere Standard Time:20261021T110000
            END:VEVENT");
        let e = parse(&ics, utc(2026, 10, 19, 0, 0), utc(2026, 10, 23, 0, 0), &[]);
        assert_eq!(starts(&e), vec![utc(2026, 10, 20, 14, 0), local(2026, 10, 21, 10, 0)]);
    }

    #[test]
    fn pick_finds_the_right_occurrence_of_a_parsed_series() {
        let ics = cal(&format!(
            "{STANDUP}
            BEGIN:VEVENT
            UID:focus@test
            SUMMARY:Focus block
            DTSTART;TZID=America/Toronto:20261019T090000
            DTEND;TZID=America/Toronto:20261019T120000
            END:VEVENT"
        ));
        let e = parse(&ics, utc(2026, 10, 19, 0, 0), utc(2026, 10, 20, 0, 0), &[]);
        let picked = crate::calendar::pick(&e, utc(2026, 10, 19, 13, 35)).unwrap();
        assert_eq!((picked.title.as_str(), picked.start_ms), ("Stand-up", utc(2026, 10, 19, 13, 30)));
        assert_eq!(crate::calendar::pick(&e, utc(2026, 10, 19, 14, 0)).unwrap().title, "Focus block");
    }
}
