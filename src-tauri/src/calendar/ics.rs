//! Events from an iCalendar (.ics) file, such as a calendar's private iCal address (Google, Outlook,
//! iCloud). Pure: bytes in, events out, no network or OS calls. Used by the Windows calendar; compiles everywhere.
//!
//! Real feeds are messy, so reading is tolerant and per event: a byte order mark, any line ends, folds that
//! split a UTF-8 character, unknown or broken lines, stray END lines, garbage after the calendar and several
//! calendars in one file are all fine, and an event that can't be read is skipped without losing the others.
//! Reading (`Calendar::read`) is done once per download; expanding (`Calendar::events`) is cheap per window.
//!
//! Times: `Z` is UTC; `TZID=` is an IANA name (Google, iCloud) or a Windows zone name (Outlook, e.g. "Eastern
//! Standard Time", mapped through `WINDOWS_ZONES`), or a custom id whose VTIMEZONE names its IANA zone in
//! `X-LIC-LOCATION`; anything else, and floating times, are local. `VALUE=DATE` (all-day) starts at local midnight.

use super::Event;
use chrono::{DateTime, Days, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use rrule::{RRule, RRuleSet, Tz, Unvalidated, Validated};
use std::collections::HashMap;

/// At most this many occurrences of one recurring event, so a pathological rule can't hang or flood.
pub const MAX_OCCURRENCES: usize = 1000;
/// At most this many steps through one rule (including those before the window), for the same reason. A daily
/// meeting started 30 years ago is about 11,000 steps.
const MAX_STEPS: usize = 100_000;

/// The events overlapping [`from_ms`, `to_ms`), with recurring ones expanded into occurrences. `me` holds the
/// user's own email addresses, which are left out of the attendees. A file that can't be read gives no events.
/// The app reads once per download through `Calendar` instead; this is the one-shot form the tests use.
#[cfg(test)]
pub fn parse(ics: &str, from_ms: i64, to_ms: i64, me: &[String]) -> Vec<Event> {
    Calendar::read(ics.as_bytes(), me).events(from_ms, to_ms)
}

/// A calendar file, read once and expanded for any window.
pub struct Calendar {
    items: Vec<Item>,
}

/// One event (or one series) as read.
struct Item {
    title: String,
    attendees: Vec<String>,
    emails: Vec<String>,
    join_url: Option<String>,
    declined: bool,
    start: DateTime<Tz>,
    all_day: bool,
    length: Length,
    recurrence: Option<Recurrence>,
}

struct Recurrence {
    rule: RRule<Validated>,
    rdates: Vec<DateTime<Tz>>,
    exdates: Vec<DateTime<Tz>>,
    /// Days left out of a timed series by a date-only EXDATE (`EXDATE;VALUE=DATE:20261019`), in the series' zone.
    exdays: Vec<NaiveDate>,
    until_ms: Option<i64>,
}

#[derive(Clone, Copy)]
enum Length {
    Ms(i64),
    /// All-day: ends at local midnight this many days later.
    Days(u32),
}

impl Length {
    /// The end of an occurrence that starts at `start`.
    fn end_ms(self, start: &DateTime<Tz>) -> i64 {
        match self {
            Length::Ms(ms) => start.timestamp_millis().saturating_add(ms),
            Length::Days(n) => start
                .with_timezone(&chrono::Local)
                .date_naive()
                .checked_add_days(Days::new(n.into()))
                .and_then(|d| in_zone(Tz::LOCAL, d.and_hms_opt(0, 0, 0)?))
                .map(|d| d.timestamp_millis())
                .unwrap_or(start.timestamp_millis().saturating_add(i64::from(n) * 86_400_000)),
        }
    }

    /// The longest an occurrence can last, for cheap window checks.
    fn max_ms(self) -> i64 {
        match self {
            Length::Ms(ms) => ms,
            Length::Days(n) => i64::from(n) * 86_400_000 + 3_600_000,
        }
    }
}

impl Calendar {
    /// Reads a calendar file. Never fails: what can't be read is left out.
    pub fn read(bytes: &[u8], me: &[String]) -> Calendar {
        let text = unfold(bytes);
        let blocks = blocks(&text);
        let zones: HashMap<String, Tz> = blocks.zones;

        let mut masters: Vec<(Option<String>, Item)> = Vec::new();
        let mut overrides: Vec<(String, DateTime<Tz>)> = Vec::new(); // (UID, original start) replaced or cancelled
        let mut moved: Vec<(Option<String>, Item)> = Vec::new(); // overrides that still happen
        let mut cancelled_series: Vec<String> = Vec::new();
        for lines in &blocks.events {
            let ev = Block(lines);
            let uid = ev.value("UID").map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
            let cancelled = ev.value("STATUS").is_some_and(|s| s.trim().eq_ignore_ascii_case("CANCELLED"));
            if let Some(rid) = ev.get("RECURRENCE-ID") {
                if let (Some(uid), Some(rid)) = (&uid, when(rid, &zones)) {
                    overrides.push((uid.clone(), rid.at));
                }
                if !cancelled {
                    moved.extend(item(&ev, &zones, me, false).map(|it| (uid, it)));
                }
            } else if cancelled {
                cancelled_series.extend(uid);
            } else if let Some(it) = item(&ev, &zones, me, true) {
                masters.push((uid, it));
            }
        }

        // A cancelled series takes its moved occurrences with it.
        let mut items: Vec<Item> = moved.into_iter().filter(|(uid, _)| uid.as_ref().is_none_or(|u| !cancelled_series.contains(u))).map(|(_, it)| it).collect();

        // An override replaces its occurrence in the series with the same UID. Series without a UID match nothing.
        let mut by_uid: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, (uid, _)) in masters.iter().enumerate() {
            if let Some(u) = uid {
                by_uid.entry(u.as_str()).or_default().push(i);
            }
        }
        let mut replaced: Vec<(usize, DateTime<Tz>)> = Vec::new();
        for (uid, at) in &overrides {
            for &i in by_uid.get(uid.as_str()).map(Vec::as_slice).unwrap_or_default() {
                replaced.push((i, *at));
            }
        }
        for (i, at) in replaced {
            if let Some(r) = masters[i].1.recurrence.as_mut() {
                r.exdates.push(at);
            }
        }
        items.extend(masters.into_iter().map(|(_, it)| it));
        Calendar { items }
    }

    /// How many events (series count once) were read.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// The events overlapping [`from_ms`, `to_ms`), recurring ones expanded, sorted by start.
    pub fn events(&self, from_ms: i64, to_ms: i64) -> Vec<Event> {
        let mut out = Vec::new();
        for it in &self.items {
            let start_ms = it.start.timestamp_millis();
            if start_ms >= to_ms {
                continue;
            }
            let occurrences: Vec<(i64, i64)> = match &it.recurrence {
                None => {
                    let end = it.length.end_ms(&it.start);
                    if end <= from_ms {
                        continue;
                    }
                    vec![(start_ms, end)]
                }
                Some(r) => {
                    if r.until_ms.is_some_and(|u| u.saturating_add(it.length.max_ms()) <= from_ms) {
                        continue;
                    }
                    expand(it, r, from_ms, to_ms)
                }
            };
            for (s, e) in occurrences {
                if s < to_ms && e > from_ms {
                    out.push(Event {
                        title: it.title.clone(),
                        start_ms: s,
                        end_ms: e,
                        all_day: it.all_day,
                        attendees: it.attendees.clone(),
                        emails: it.emails.clone(),
                        join_url: it.join_url.clone(),
                        // Cancelled events are dropped when the file is read.
                        cancelled: false,
                        declined: it.declined,
                    });
                }
            }
        }
        out.sort_by_key(|e| e.start_ms);
        out
    }
}

/// Occurrences (start, end) of a series that can overlap the window, at most `MAX_OCCURRENCES`.
fn expand(it: &Item, r: &Recurrence, from_ms: i64, to_ms: i64) -> Vec<(i64, i64)> {
    let set = RRuleSet::new(it.start).rrule(r.rule.clone()).set_rdates(r.rdates.clone()).set_exdates(r.exdates.clone()).limit();
    let reach = it.length.max_ms();
    let mut out = Vec::new();
    for at in set.into_iter().take(MAX_STEPS) {
        let s = at.timestamp_millis();
        if s >= to_ms {
            break;
        }
        if s.saturating_add(reach) > from_ms && !r.exdays.contains(&at.date_naive()) {
            let e = it.length.end_ms(&at);
            if e > from_ms {
                out.push((s, e));
                if out.len() >= MAX_OCCURRENCES {
                    break;
                }
            }
        }
    }
    out
}

/// Builds an item from an event's lines; `None` when it has no readable start. `series` reads RRULE/RDATE/EXDATE.
fn item(ev: &Block, zones: &HashMap<String, Tz>, me: &[String], series: bool) -> Option<Item> {
    let start = when(ev.get("DTSTART")?, zones)?;
    let length = length(ev, &start, zones);
    let recurrence = if series { ev.value("RRULE").and_then(|rule| recurrence(rule, ev, &start, zones)) } else { None };
    // A rule that can't be read leaves the first occurrence, unless it's excluded.
    if recurrence.is_none() && series && ev.value("RRULE").is_some() && ev.times("EXDATE", zones).contains(&start.at) {
        return None;
    }
    let (attendees, emails, declined) = attendees(ev, me);
    Some(Item {
        title: ev.value("SUMMARY").map(|s| unescape(s).trim().to_string()).unwrap_or_default(),
        attendees,
        emails,
        join_url: join_url(ev),
        declined,
        start: start.at,
        all_day: start.all_day,
        length,
        recurrence,
    })
}

fn recurrence(rule: &str, ev: &Block, start: &When, zones: &HashMap<String, Tz>) -> Option<Recurrence> {
    let rule = until_in_utc(rule, start.at.timezone()).parse::<RRule<Unvalidated>>().ok()?.validate(start.at).ok()?;
    let until_ms = rule.get_until().map(|u| u.timestamp_millis());
    // On a timed series a date-only EXDATE means that whole day (an all-day series matches it as a time already).
    let exdays = if start.all_day { vec![] } else { ev.days("EXDATE") };
    Some(Recurrence { rule, rdates: ev.times("RDATE", zones), exdates: ev.times("EXDATE", zones), exdays, until_ms })
}

/// DTEND − DTSTART, else DURATION, else a day for all-day events and nothing for the rest (RFC 5545).
fn length(ev: &Block, start: &When, zones: &HashMap<String, Tz>) -> Length {
    let end = ev.get("DTEND").and_then(|p| when(p, zones));
    if start.all_day {
        let days = match (&end, ev.value("DURATION").and_then(iso_duration_ms)) {
            (Some(e), _) if e.all_day => (e.at.date_naive() - start.at.date_naive()).num_days(),
            (Some(e), _) => return Length::Ms((e.at.timestamp_millis() - start.at.timestamp_millis()).max(0)),
            (None, Some(d)) => d / 86_400_000,
            (None, None) => 1,
        };
        return Length::Days(days.clamp(0, 3660) as u32);
    }
    if let Some(e) = end {
        return Length::Ms((e.at.timestamp_millis() - start.at.timestamp_millis()).max(0));
    }
    Length::Ms(ev.value("DURATION").and_then(iso_duration_ms).unwrap_or(0).max(0))
}

// ---- Reading lines and blocks ----

/// Strips a byte order mark, turns CRLF / CR / LF into LF and joins folded lines, all before UTF-8 decoding so a
/// fold inside a multibyte character comes out whole. Invalid UTF-8 becomes U+FFFD.
fn unfold(bytes: &[u8]) -> String {
    let b = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'\r' || c == b'\n' {
            i += if c == b'\r' && b.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
            if matches!(b.get(i), Some(b' ' | b'\t')) {
                i += 1; // a fold: the next line continues this one
            } else {
                out.push(b'\n');
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// One content line: `NAME;PARAM=value;PARAM="quoted; value":value`.
struct Line<'a> {
    name: &'a str,
    params: Vec<(&'a str, String)>,
    value: &'a str,
}

/// Splits a content line; `None` for a line that isn't one (no colon, a name with odd characters).
fn content_line(line: &str) -> Option<Line<'_>> {
    let name_end = line.find([';', ':'])?;
    let name = &line[..name_end];
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return None;
    }
    let mut params = Vec::new();
    let mut rest = &line[name_end..];
    loop {
        if let Some(value) = rest.strip_prefix(':') {
            return Some(Line { name, params, value });
        }
        rest = rest.strip_prefix(';')?;
        let key_end = rest.find(['=', ';', ':'])?;
        let key = &rest[..key_end];
        rest = &rest[key_end..];
        let Some(after_eq) = rest.strip_prefix('=') else {
            params.push((key, String::new()));
            continue;
        };
        // The value runs to the next ';' or ':' outside double quotes; the quotes themselves are dropped.
        let (mut value, mut quoted, mut end) = (String::new(), false, None);
        for (i, ch) in after_eq.char_indices() {
            match ch {
                '"' => quoted = !quoted,
                ';' | ':' if !quoted => {
                    end = Some(i);
                    break;
                }
                _ => value.push(ch),
            }
        }
        params.push((key, value));
        rest = &after_eq[end?..];
    }
}

/// The lines of one event (its own, not those of a VALARM inside it).
struct Block<'a, 'b>(&'b [Line<'a>]);

impl Block<'_, '_> {
    fn get(&self, name: &str) -> Option<&Line<'_>> {
        self.0.iter().find(|l| l.name.eq_ignore_ascii_case(name))
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.get(name).map(|l| l.value)
    }

    /// The date-only values of a list property (`EXDATE;VALUE=DATE:20261019,20261026`).
    fn days(&self, name: &str) -> Vec<NaiveDate> {
        self.0
            .iter()
            .filter(|l| l.name.eq_ignore_ascii_case(name))
            .flat_map(|l| l.value.split(',').filter(|v| v.trim().len() == 8).filter_map(|v| NaiveDate::parse_from_str(v.trim(), "%Y%m%d").ok()))
            .collect()
    }

    /// Every value of a list property that may repeat (EXDATE, RDATE), e.g. `EXDATE;TZID=…:20261019T093000,20261026T093000`.
    fn times(&self, name: &str, zones: &HashMap<String, Tz>) -> Vec<DateTime<Tz>> {
        self.0
            .iter()
            .filter(|l| l.name.eq_ignore_ascii_case(name))
            .flat_map(|l| {
                let date = param(l, "VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE"));
                l.value.split(',').filter_map(move |v| parse_time(v, param(l, "TZID"), date, zones)).map(|w| w.at)
            })
            .collect()
    }
}

fn param<'a>(l: &'a Line, key: &str) -> Option<&'a str> {
    l.params.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.trim())
}

struct Blocks<'a> {
    events: Vec<Vec<Line<'a>>>,
    zones: HashMap<String, Tz>,
}

/// Walks the lines with a stack of open components. Collects each VEVENT inside a VCALENDAR (one left open ends
/// at the next BEGIN:VEVENT or END:VCALENDAR) and the VTIMEZONEs that name an IANA zone. An END that matches
/// nothing open is ignored; text outside any VCALENDAR is ignored.
fn blocks(text: &str) -> Blocks<'_> {
    let mut stack: Vec<String> = Vec::new();
    let mut events = Vec::new();
    let mut zones = HashMap::new();
    let mut event: Option<Vec<Line>> = None;
    let mut tz: (Option<String>, Option<String>) = (None, None);

    fn close<'a>(name: &str, event: &mut Option<Vec<Line<'a>>>, events: &mut Vec<Vec<Line<'a>>>, tz: &mut (Option<String>, Option<String>), zones: &mut HashMap<String, Tz>) {
        if name == "VEVENT" {
            events.extend(event.take());
        } else if name == "VTIMEZONE" {
            if let (Some(id), Some(z)) = (tz.0.take(), tz.1.take().as_deref().and_then(iana)) {
                zones.insert(id, z);
            }
            *tz = (None, None);
        }
    }

    for raw in text.split('\n') {
        let Some(line) = content_line(raw.trim_end_matches([' ', '\t', '\0'])) else { continue };
        if line.name.eq_ignore_ascii_case("BEGIN") {
            let what = line.value.trim().to_ascii_uppercase();
            let in_calendar = stack.iter().any(|s| s == "VCALENDAR");
            if what == "VEVENT" {
                // An event left open ends here.
                if let Some(pos) = stack.iter().rposition(|s| s == "VEVENT") {
                    for name in stack.split_off(pos).iter().rev() {
                        close(name, &mut event, &mut events, &mut tz, &mut zones);
                    }
                }
                if in_calendar {
                    event = Some(Vec::new());
                }
            }
            if what == "VTIMEZONE" {
                tz = (None, None);
            }
            if what == "VCALENDAR" || in_calendar {
                stack.push(what);
            }
        } else if line.name.eq_ignore_ascii_case("END") {
            let what = line.value.trim().to_ascii_uppercase();
            if let Some(pos) = stack.iter().rposition(|s| *s == what) {
                for name in stack.split_off(pos).iter().rev() {
                    close(name, &mut event, &mut events, &mut tz, &mut zones);
                }
            }
        } else {
            match stack.last().map(String::as_str) {
                Some("VEVENT") => {
                    if let Some(ev) = event.as_mut() {
                        ev.push(line);
                    }
                }
                Some("VTIMEZONE") if line.name.eq_ignore_ascii_case("TZID") => tz.0 = Some(line.value.trim().trim_matches('"').to_string()),
                Some("VTIMEZONE") if line.name.eq_ignore_ascii_case("X-LIC-LOCATION") => tz.1 = Some(line.value.trim().to_string()),
                _ => {}
            }
        }
    }
    // A file cut short: keep what was read of the open event.
    for name in std::mem::take(&mut stack).iter().rev() {
        close(name, &mut event, &mut events, &mut tz, &mut zones);
    }
    Blocks { events, zones }
}

/// TEXT unescaping (RFC 5545 3.3.11), done once: `\,` `\;` `\\` and `\n`/`\N` (a newline).
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

// ---- Times ----

/// A parsed DTSTART-like value.
struct When {
    at: DateTime<Tz>,
    all_day: bool,
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
        // Checked: a gap at the very end of the calendar must not panic.
        LocalResult::None => n.checked_add_signed(chrono::Duration::hours(1)).and_then(|later| tz.from_local_datetime(&later).earliest()),
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

fn when(l: &Line, vtimezones: &HashMap<String, Tz>) -> Option<When> {
    let date = param(l, "VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE"));
    parse_time(l.value, param(l, "TZID"), date, vtimezones)
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
                let unit_ms = match (unit, in_time) {
                    ('W', false) => 7 * 86_400_000,
                    ('D', false) => 86_400_000,
                    ('H', true) => 3_600_000,
                    ('M', true) => 60_000,
                    ('S', true) => 1000,
                    _ => return None,
                };
                total = total.checked_add(n.checked_mul(unit_ms)?)?;
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

/// Who else is invited: the CN, else the email address's local part. Left out: the user's own addresses, people
/// who declined, rooms and resources, and non-email entries without a name.
/// The Zoom, Meet or Teams link: the conference fields Google and Outlook add first, then the URL, location and
/// description.
fn join_url(ev: &Block) -> Option<String> {
    const FIELDS: [&str; 6] = ["X-GOOGLE-CONFERENCE", "X-MICROSOFT-SKYPETEAMSMEETINGURL", "X-MICROSOFT-ONLINEMEETINGEXTERNALLINK", "URL", "LOCATION", "DESCRIPTION"];
    let texts: Vec<String> = FIELDS.iter().filter_map(|f| ev.value(f)).map(unescape).collect();
    super::join_link(&texts.iter().map(String::as_str).collect::<Vec<_>>())
}

/// The other people's names, their email addresses where the calendar has them, and whether the user declined.
fn attendees(ev: &Block, me: &[String]) -> (Vec<String>, Vec<String>, bool) {
    let mut names: Vec<String> = Vec::new();
    let mut emails: Vec<String> = Vec::new();
    let mut declined = false;
    let is_me = |a: &str| me.iter().any(|m| m.trim().eq_ignore_ascii_case(a));
    for l in ev.0.iter().filter(|l| l.name.eq_ignore_ascii_case("ATTENDEE")) {
        let value = l.value.trim();
        let email = value.get(..7).filter(|p| p.eq_ignore_ascii_case("mailto:")).map(|_| value[7..].trim());
        if param(l, "PARTSTAT").is_some_and(|p| p.eq_ignore_ascii_case("DECLINED")) {
            declined |= email.is_some_and(is_me);
            continue;
        }
        if param(l, "CUTYPE").is_some_and(|t| t.eq_ignore_ascii_case("ROOM") || t.eq_ignore_ascii_case("RESOURCE")) {
            continue;
        }
        if email.is_some_and(is_me) {
            continue;
        }
        let cn = param(l, "CN").filter(|n| !n.is_empty() && email.is_none_or(|a| !n.eq_ignore_ascii_case(a))).map(unescape);
        let name = cn.or_else(|| email.and_then(|a| a.split('@').next()).filter(|l| !l.is_empty()).map(str::to_string));
        if let Some(n) = name.filter(|n| !names.contains(n)) {
            names.push(n);
        }
        if let Some(a) = email.and_then(super::email_of).filter(|a| !emails.iter().any(|e| e.eq_ignore_ascii_case(a))) {
            emails.push(a);
        }
    }
    (names, emails, declined)
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

    #[test]
    fn a_time_in_the_spring_gap_moves_an_hour_later_and_the_calendar_edge_never_panics() {
        let ny = zone("America/New_York", &HashMap::new());
        let gap = NaiveDate::from_ymd_opt(2026, 3, 8).unwrap().and_hms_opt(2, 30, 0).unwrap();
        assert_eq!(in_zone(ny, gap).unwrap().to_rfc3339(), "2026-03-08T03:30:00-04:00");
        for tz in [ny, Tz::UTC, zone("Pacific/Kiritimati", &HashMap::new())] {
            let _ = in_zone(tz, NaiveDateTime::MAX);
            let _ = in_zone(tz, NaiveDateTime::MIN);
        }
    }

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
        assert_eq!(e[0].emails, vec!["ana@example.com", "sam@example.com", "jordan.k@example.com"]);
        assert_eq!(e[0].join_url, None);
        assert!(!e[0].declined);
    }

    #[test]
    fn an_event_the_user_declined_says_so() {
        let ics = cal("
            BEGIN:VEVENT
            UID:no@test
            SUMMARY:Optional review
            DTSTART:20261020T170000Z
            DTEND:20261020T173000Z
            ATTENDEE;CN=Ana Reyes;PARTSTAT=ACCEPTED:mailto:ana@example.com
            ATTENDEE;CN=Miguel;PARTSTAT=DECLINED:mailto:miguel@example.com
            END:VEVENT
            BEGIN:VEVENT
            UID:other@test
            SUMMARY:Ana declined
            DTSTART:20261020T180000Z
            DTEND:20261020T183000Z
            ATTENDEE;CN=Ana Reyes;PARTSTAT=DECLINED:mailto:ana@example.com
            END:VEVENT");
        let e = parse(&ics, utc(2026, 10, 20, 0, 0), utc(2026, 10, 21, 0, 0), &["miguel@example.com".to_string()]);
        assert_eq!(e.iter().map(|e| e.declined).collect::<Vec<_>>(), vec![true, false]);
    }

    #[test]
    fn the_join_link_comes_from_the_conference_field_location_or_description() {
        let ics = cal("
            BEGIN:VEVENT
            UID:a@test
            SUMMARY:Google
            DTSTART:20261020T170000Z
            DTEND:20261020T173000Z
            DESCRIPTION:Agenda: https://docs.example.com/x
            X-GOOGLE-CONFERENCE:https://meet.google.com/abc-defg-hij
            END:VEVENT
            BEGIN:VEVENT
            UID:b@test
            SUMMARY:Zoom in notes
            DTSTART:20261020T180000Z
            DTEND:20261020T183000Z
            LOCATION:Room 4\\, 2nd floor
            DESCRIPTION:Ana is inviting you to a Zoom meeting.\\n\\nJoin: https://us02web.zoom.us/j/81234567890?pwd=x\\n
            END:VEVENT
            BEGIN:VEVENT
            UID:c@test
            SUMMARY:Lunch
            DTSTART:20261020T190000Z
            DTEND:20261020T193000Z
            LOCATION:https://maps.example.com/cafe
            END:VEVENT");
        let e = parse(&ics, utc(2026, 10, 20, 0, 0), utc(2026, 10, 21, 0, 0), &[]);
        let links: Vec<Option<&str>> = e.iter().map(|e| e.join_url.as_deref()).collect();
        assert_eq!(links, vec![Some("https://meet.google.com/abc-defg-hij"), Some("https://us02web.zoom.us/j/81234567890?pwd=x"), None]);
    }

    #[test]
    fn a_malformed_file_gives_no_events() {
        for (bad, count) in [
            ("", 0),
            ("not a calendar", 0),
            ("BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nSUMMARY:x\r\n", 0),
            // An event outside any VCALENDAR is ignored.
            ("BEGIN:VEVENT\r\nSUMMARY:Loose\r\nDTSTART:20261020T170000Z\r\nEND:VEVENT\r\n", 0),
            (&cal("BEGIN:VEVENT\nSUMMARY:No start\nEND:VEVENT"), 0),
            (&cal("BEGIN:VEVENT\nSUMMARY:Bad start\nDTSTART:2026-10-20\nEND:VEVENT"), 0),
            // A rule that can't be read keeps the first occurrence.
            (&cal("BEGIN:VEVENT\nSUMMARY:Bad rule\nDTSTART:20261020T170000Z\nDTEND:20261020T180000Z\nRRULE:FREQ=SOMETIMES\nEND:VEVENT"), 1),
        ] {
            let e = parse(bad, utc(2026, 1, 1, 0, 0), utc(2027, 1, 1, 0, 0), &[]);
            assert_eq!(e.len(), count, "{bad:?}");
        }
    }

    const GOOD: &str = "BEGIN:VEVENT\r\nUID:good@test\r\nSUMMARY:Good\r\nDTSTART:20261020T170000Z\r\nDTEND:20261020T173000Z\r\nEND:VEVENT\r\n";

    fn titles(ics: &str) -> Vec<String> {
        parse(ics, utc(2026, 10, 1, 0, 0), utc(2026, 11, 1, 0, 0), &[]).into_iter().map(|e| e.title).collect()
    }

    #[test]
    fn a_byte_order_mark_bare_cr_and_trailing_garbage_are_tolerated() {
        let body = format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{GOOD}END:VCALENDAR\r\n");
        assert_eq!(titles(&format!("\u{feff}{body}")), vec!["Good"]);
        assert_eq!(titles(&body.replace("\r\n", "\r")), vec!["Good"]);
        assert_eq!(titles(&body.replace("\r\n", "\n")), vec!["Good"]);
        assert_eq!(titles(&format!("{body}\r\n<html>502 Bad Gateway</html>\r\n\0\0")), vec!["Good"]);
    }

    #[test]
    fn a_bad_event_next_to_a_good_one_leaves_the_good_one() {
        let bad = "BEGIN:VEVENT\r\nUID:bad@test\r\nX-FOO+BAR:what\r\nthis line has no colon\r\nSUMMARY:Bad\r\nDTSTART:soon\r\nEND:VEVENT\r\n";
        let odd = "BEGIN:VEVENT\r\nUID:odd@test\r\nX-FOO+BAR:what\r\nSUMMARY:Odd but fine\r\nDTSTART:20261021T170000Z\r\nDTEND:20261021T173000Z\r\nEND:VTODO\r\nEND:VEVENT\r\n";
        let ics = format!("BEGIN:VCALENDAR\r\n{bad}{GOOD}{odd}END:VCALENDAR\r\n");
        assert_eq!(titles(&ics), vec!["Good", "Odd but fine"]);
    }

    #[test]
    fn an_unclosed_event_ends_at_the_next_one() {
        let ics = format!("BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:open@test\r\nSUMMARY:Open\r\nDTSTART:20261019T170000Z\r\nDTEND:20261019T173000Z\r\n{GOOD}END:VCALENDAR\r\n");
        assert_eq!(titles(&ics), vec!["Open", "Good"]);
    }

    #[test]
    fn two_concatenated_calendars_are_both_read() {
        let second = GOOD.replace("good@test", "second@test").replace("Good", "Second").replace("20261020", "20261022");
        let ics = format!("BEGIN:VCALENDAR\r\n{GOOD}END:VCALENDAR\r\nBEGIN:VCALENDAR\r\n{second}END:VCALENDAR\r\n");
        assert_eq!(titles(&ics), vec!["Good", "Second"]);
    }

    #[test]
    fn a_fold_inside_a_multibyte_character_is_joined_before_decoding() {
        // "Café" with the fold between the two bytes of "é" (C3 A9).
        let mut bytes = b"BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:f@test\r\nSUMMARY:Caf\xC3\r\n \xA9 sync\r\nDTSTART:20261020T170000Z\r\nDTEND:20261020T173000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n".to_vec();
        let cal = Calendar::read(&bytes, &[]);
        assert_eq!(cal.events(utc(2026, 10, 1, 0, 0), utc(2026, 11, 1, 0, 0))[0].title, "Café sync");
        bytes.truncate(40); // cut mid-file: no panic, no events
        assert!(Calendar::read(&bytes, &[]).events(0, i64::MAX).is_empty());
    }

    #[test]
    fn a_valarm_inside_an_event_does_not_change_its_title() {
        let ics = cal("
            BEGIN:VEVENT
            UID:alarm@test
            SUMMARY:Board meeting
            DTSTART:20261020T170000Z
            DTEND:20261020T180000Z
            BEGIN:VALARM
            ACTION:DISPLAY
            SUMMARY:Reminder
            TRIGGER:-PT10M
            END:VALARM
            END:VEVENT");
        assert_eq!(titles(&ics), vec!["Board meeting"]);
    }

    #[test]
    fn text_is_unescaped_exactly_once() {
        let ics = cal(r"
            BEGIN:VEVENT
            UID:esc@test
            SUMMARY:Q3 plan\, budget\; risks \\n not a newline
            DTSTART:20261020T170000Z
            DTEND:20261020T180000Z
            END:VEVENT");
        assert_eq!(titles(&ics), vec![r"Q3 plan, budget; risks \n not a newline"]);
    }

    #[test]
    fn odd_attendee_values_do_not_panic_and_declined_or_unnamed_ones_are_left_out() {
        let ics = cal("
            BEGIN:VEVENT
            UID:att@test
            SUMMARY:Sync
            DTSTART:20261020T170000Z
            DTEND:20261020T173000Z
            ATTENDEE:abcdefé@x.com
            ATTENDEE:é
            ATTENDEE:MAILTO:rené@example.com
            ATTENDEE;PARTSTAT=DECLINED;CN=Nope:mailto:nope@example.com
            ATTENDEE;CUTYPE=ROOM;CN=Room 4:mailto:room4@example.com
            ATTENDEE;CN=Phone Guest:tel:+15555550100
            END:VEVENT");
        let e = parse(&ics, utc(2026, 10, 20, 0, 0), utc(2026, 10, 21, 0, 0), &[]);
        assert_eq!(e[0].attendees, vec!["rené", "Phone Guest"]);
    }

    #[test]
    fn a_multi_day_all_day_event_ends_at_local_midnight() {
        let ics = cal("
            BEGIN:VEVENT
            UID:trip@test
            SUMMARY:Offsite
            DTSTART;VALUE=DATE:20261030
            DTEND;VALUE=DATE:20261103
            END:VEVENT");
        let e = parse(&ics, utc(2026, 10, 1, 0, 0), utc(2026, 12, 1, 0, 0), &[]);
        // Spans the end of daylight saving in most northern zones; still ends exactly at local midnight.
        assert_eq!((e[0].start_ms, e[0].end_ms), (local(2026, 10, 30, 0, 0), local(2026, 11, 3, 0, 0)));
    }

    #[test]
    fn an_override_without_a_uid_does_not_touch_other_series() {
        let ics = cal("
            BEGIN:VEVENT
            SUMMARY:Series without UID
            DTSTART:20261019T170000Z
            DTEND:20261019T173000Z
            RRULE:FREQ=DAILY;COUNT=3
            END:VEVENT
            BEGIN:VEVENT
            SUMMARY:Moved without UID
            RECURRENCE-ID:20261020T170000Z
            DTSTART:20261020T190000Z
            DTEND:20261020T193000Z
            END:VEVENT");
        let e = parse(&ics, utc(2026, 10, 1, 0, 0), utc(2026, 11, 1, 0, 0), &[]);
        assert_eq!(e.iter().filter(|e| e.title == "Series without UID").count(), 3);
        assert_eq!(e.iter().filter(|e| e.title == "Moved without UID").count(), 1);
    }

    #[test]
    fn a_parsed_calendar_can_be_expanded_for_several_windows() {
        let cal = Calendar::read(super::tests::cal(STANDUP).as_bytes(), &[]);
        assert_eq!(cal.events(utc(2026, 10, 12, 0, 0), utc(2026, 10, 13, 0, 0)).len(), 1);
        assert_eq!(cal.events(utc(2026, 10, 12, 0, 0), utc(2026, 10, 27, 0, 0)).len(), 3);
        assert!(cal.events(utc(2026, 9, 1, 0, 0), utc(2026, 10, 1, 0, 0)).is_empty());
    }

    #[test]
    fn a_date_only_exdate_removes_that_day_of_a_timed_series() {
        let ics = cal(&STANDUP.replace("RRULE:", "EXDATE;VALUE=DATE:20261019\nRRULE:"));
        let e = parse(&ics, utc(2026, 10, 12, 0, 0), utc(2026, 10, 27, 0, 0), &[]);
        assert_eq!(starts(&e), vec![utc(2026, 10, 12, 13, 30), utc(2026, 10, 26, 13, 30)]);
    }

    #[test]
    fn overrides_of_a_cancelled_series_are_hidden_too() {
        let ics = cal(&format!(
            "{}
            BEGIN:VEVENT
            UID:standup@test
            RECURRENCE-ID;TZID=America/Toronto:20261019T093000
            SUMMARY:Stand-up (moved)
            DTSTART;TZID=America/Toronto:20261019T110000
            DTEND;TZID=America/Toronto:20261019T111500
            END:VEVENT",
            STANDUP.replace("RRULE:", "STATUS:CANCELLED\nRRULE:")
        ));
        assert!(parse(&ics, utc(2026, 10, 12, 0, 0), utc(2026, 10, 27, 0, 0), &[]).is_empty());
    }

    #[test]
    fn huge_durations_do_not_overflow() {
        let ics = cal("
            BEGIN:VEVENT
            UID:long@test
            SUMMARY:Forever
            DTSTART:20261020T170000Z
            DURATION:P106751991167D
            END:VEVENT
            BEGIN:VEVENT
            UID:long-series@test
            SUMMARY:Forever weekly
            DTSTART:20261020T170000Z
            DURATION:P106751991167D
            RRULE:FREQ=WEEKLY;UNTIL=20261103T170000Z
            END:VEVENT");
        let e = parse(&ics, utc(2026, 10, 1, 0, 0), utc(2026, 11, 1, 0, 0), &[]);
        assert_eq!(e.iter().filter(|e| e.title == "Forever").count(), 1);
        assert!(e.iter().all(|e| e.end_ms == i64::MAX));
        assert!(parse(&ics, i64::MAX - 10, i64::MAX, &[]).iter().all(|e| e.end_ms > e.start_ms));
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

    /// Timing on a ~20 MB feed (years of history, a few hundred series). Run with
    /// `cargo test --release large_feed_timing -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn large_feed_timing() {
        let mut body = String::new();
        for i in 0..45_000 {
            let day = NaiveDate::from_ymd_opt(2015, 1, 1).unwrap() + chrono::Days::new(i / 12);
            let rule = if i % 150 == 0 { "RRULE:FREQ=WEEKLY;BYDAY=MO,WE\n" } else { "" };
            body += &format!(
                "BEGIN:VEVENT\nUID:e{i}@test\nSUMMARY:Meeting number {i} about the quarterly plan\nDTSTART;TZID=America/Toronto:{d}T{h:02}0000\nDTEND;TZID=America/Toronto:{d}T{h:02}3000\n{rule}DESCRIPTION:Notes for meeting {i}\\, with a fairly long description line that pads the size out a bit more.\nATTENDEE;CN=Ana Reyes;PARTSTAT=ACCEPTED:mailto:ana@example.com\nATTENDEE;CN=Sam Lee;PARTSTAT=ACCEPTED:mailto:sam@example.com\nORGANIZER;CN=Miguel:mailto:miguel@example.com\nEND:VEVENT\n",
                d = day.format("%Y%m%d"),
                h = 8 + i % 10
            );
        }
        let ics = cal(&body);
        let t = std::time::Instant::now();
        let parsed = Calendar::read(ics.as_bytes(), &[]);
        let read = t.elapsed();
        let t = std::time::Instant::now();
        let week = parsed.events(utc(2026, 10, 19, 0, 0), utc(2026, 10, 26, 0, 0));
        println!("{} MB, {} items: read {:?}, one week ({} events) {:?}", ics.len() / 1_000_000, parsed.len(), read, week.len(), t.elapsed());
    }
}
