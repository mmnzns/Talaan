# W4. Windows calendar and browser tabs — task plan

> **Later change:** the calendar's iCal address no longer lives in settings.json (`calendarIcsUrl`). It moved to
> secure key storage, key `calendar-ics` (Credential Manager target `calendar-ics.Talaan`), through `keys.rs`;
> settings keep only `calendarConnected`, and startup removes any old `calendarIcsUrl`. The plan below is kept as
> written.

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax.

> Part of the Windows port (docs/WINDOWS-PORT.md, phases W0–W5). Decision (Notion, 2026-10-15): both are built
> for Windows v1.0. Model: Opus for the spike and UI Automation, Sonnet for the iCal parts.

**Goal:** On Windows, a recording gets its calendar event's title and attendees, and a browser holding the mic
with a Meet / Zoom web / Teams web tab in front counts as that call (so Browser calls can use Always).

**Architecture:** Same contract as macOS. `platform/windows/calendar.rs` exposes `Access`, `access()`,
`request()`, `events(from, to)`; `platform/windows/browser_tabs.rs` exposes `tab_urls(app, family) -> Tabs`.
The pure picker (`calendar::pick`) and the URL classifier (`calls::tabs`) are shared and unchanged.

---

## Verified facts and reasoning (2026-10-15)

- **WinRT `AppointmentStore` is very likely a dead end.** It reads the store behind the old Windows "Mail and
  Calendar" app, which Microsoft retired at the end of 2024. New Outlook (installed on this PC as
  `Microsoft.OutlookForWindows`) and Google Calendar don't feed it. It also needs the `appointments` capability,
  which an unpackaged (NSIS) app may not get. Task 1 is a time-boxed spike to confirm before building the
  alternative.
- **Alternative: the calendar's private iCal address.** Google Calendar ("Secret address in iCal format"),
  Outlook.com / Microsoft 365 ("Publish calendar → ICS link") and iCloud all provide a read-only `.ics` URL. It
  works with any provider, and Talaan only downloads from it; nothing about meetings is sent. It's listed in
  Privacy › What leaves this computer ("Calendar: downloads your calendar from the address you gave"). The user
  pastes the address in Settings › Connections › Calendar (Windows only, so the macOS EventKit flow is
  untouched).
- Recurring meetings are the norm (stand-ups, 1:1s), so RRULE expansion with time zones is required. Use the
  `icalendar` crate (parsing) and the `rrule` crate (expansion, chrono-tz). Verify licenses (expected
  MIT/Apache-2.0).
- **Browser tabs:** Windows has no Apple Events. UI Automation can read the address bar of each browser window's
  *active* tab only. Chromium's omnibox is a native view with class name `OmniboxViewViews` (locale-independent),
  and its Value pattern gives the URL, often without `https://` (e.g. `meet.google.com/abc-defg-hij`).
  `calls::tabs::web_call` already accepts URLs without a scheme.
- Because only the active tab is visible, "no call tab seen" must NOT mean "not a call" on Windows (a call in a
  background tab would be wrongly ignored). Windows `tab_urls` returns `Tabs::Urls` only when one of the URLs is
  a call; otherwise `Tabs::Unknown`, so `classify` says Unsure, which falls back to the Browser rule (Ask). There
  is no permission on Windows, so never `Denied`.
- COM: the calls thread is in the multithreaded apartment (`com::init`). UI Automation clients are fine in MTA
  (Microsoft recommends MTA for UIA clients).

## Design (binding)

### Calendar on Windows
- `platform/windows/calendar.rs`:
  - `access()` = `Allowed` when an iCal address is saved in settings (`calendarIcsUrl`, non-empty), else `NotAsked`.
  - `request()` returns `access()` (there's no system prompt; the UI asks for the address instead).
  - `events(from, to)` returns the cached events, expanded to occurrences overlapping [from, to]. The platform
    module reads the URL through a small setter, `set_source(url: Option<String>)`, called by
    `commands/calendar.rs` when settings change and at startup, so the platform module never reads settings itself.
- `calendar/ics.rs` (new, cross-platform, pure, no network):
  `parse(ics: &str, from_ms: i64, to_ms: i64, me: &[String]) -> Vec<Event>`.
  - Expands RRULE, EXDATE and RECURRENCE-ID overrides within the window.
  - Handles TZID, UTC and floating times (floating = local).
  - All-day = DTSTART with VALUE=DATE.
  - Attendees from `ATTENDEE;CN=...`, falling back to the mailto local part; the user's own address (from the
    calendar's `X-WR-CALNAME`/organizer is unreliable, so pass `me` = the address in the URL path for Google, or
    nothing) is left out.
  - Cancelled events (`STATUS:CANCELLED`) are skipped.
- `calendar/fetch.rs` (new): download with `reqwest` blocking (already a dependency), 15 s timeout, body ≤ 20 MB.
  Cache the body in memory with its fetch time. Refetch when older than 15 minutes. On failure keep the old copy.
  Pure decision fn `needs_refresh(last_fetch_ms, now_ms) -> bool` tested.
- Settings: add `calendarIcsUrl: string` (default ""). Settings › Connections › Calendar on Windows shows an
  address field with a "Where do I find this?" link list (Google, Outlook, iCloud help pages) and Test (fetches
  and shows "Found N events this week" or the error). The macOS row is unchanged; gate with `onWindows` from
  `src/lib/os.ts`.
- Privacy › What leaves this computer: add the calendar download line (Windows, only when an address is set).

### Browser tabs on Windows
- `platform/windows/browser_tabs.rs`, `tab_urls(app_exe: &str, family: Family) -> Tabs`:
  - Find top-level visible windows whose process image name equals `app_exe`
    (EnumWindows + GetWindowThreadProcessId + QueryFullProcessImageNameW).
  - For each, find the first descendant with ClassName `OmniboxViewViews` (Chromium; TreeScope_Descendants,
    with a condition on ClassName), and read its Value.
  - Collect the URLs.
  - Pure `windows_result(urls: Vec<String>) -> Tabs`: `Urls(urls)` if any `web_call(u)` is Some, else `Unknown`.
  - Time budget: stop after ~300 ms total (UIA can be slow on a busy browser) → `Unknown`.
  - The calls loop already caches results for 4 s.
- Firefox: no `BROWSERS` entry, as on the Mac (stays Ask). Arc for Windows: try the same omnibox class. If it
  isn't found, `Unknown` (Ask).
- `browsers_status` on Windows: an installed browser shows `allowed` (no permission exists). Check how the
  frontend shows that state and that the copy makes sense on Windows ("Reads the address of the tab in front";
  no "Allow in System Settings" button).

## File map
- `src-tauri/src/platform/windows/calendar.rs`, `src-tauri/src/calendar/ics.rs` (new), `src-tauri/src/calendar/fetch.rs` (new), `src-tauri/src/calendar/mod.rs` (mod lines)
- `src-tauri/src/commands/calendar.rs` (feed the URL to the platform, Test command), `src-tauri/src/lib.rs` (register the command)
- `src-tauri/src/platform/windows/browser_tabs.rs`, `src-tauri/src/commands/calls.rs` (browsers_status state on Windows)
- `src/data/settings.ts`, `src/data/defaultSettings.ts`, `src/screens/Settings/ConnectionsTab.tsx`, `src/screens/Settings/PrivacyTab.tsx` (+ their data/tauri bindings)
- `src-tauri/Cargo.toml` (icalendar, rrule; windows features `Win32_UI_Accessibility`, `Win32_UI_WindowsAndMessaging`), `THIRD_PARTY_NOTICES.md`, `docs/WINDOWS-PORT.md`

Rules: TDD for pure logic; commit per task (`W4:`); never push; commit messages end with
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`; no BOM; stage only your own files; **macOS
unchanged** (all new UI behind `onWindows`, all new Rust either pure/shared or under `platform/windows`). Run
`cargo test --workspace`, `npm test`, `npm run build` before each commit.

**Merge note:** the i18n branch (`claude/eloquent-hugle-187866`) moves UI strings to `t()` keys and is merged into
`windows-port` later. Write new UI strings as plain English constants grouped at the top of each component, so
they're easy to move into keys at merge time.

---

### Task 1: Spike — can an unpackaged app read the Windows calendar store? (≤ 45 min)

- [ ] Ignored test `appointment_store_spike` in `platform/windows/calendar.rs`: call
  `Windows.ApplicationModel.Appointments.AppointmentManager::RequestStoreAsync(AllCalendarsReadOnly)` through the
  `windows` crate (feature `ApplicationModel_Appointments`, `Foundation`, `Foundation_Collections`). Print the
  result: error HRESULT, or the number of appointments in the next 7 days.
- [ ] **Decision rule:** if it errors (expected) OR returns 0 events while the user clearly has calendars in new
  Outlook/Google, record the result in WINDOWS-PORT.md and go with iCal (Tasks 2–4). Only if it returns real
  events, stop and report (the plan would change). Remove the feature flags you added if not used. Commit the
  spike test only if useful as documentation (keep it `#[ignore]`).

### Task 2: iCal parsing (pure)

- [ ] Tests first in `calendar/ics.rs`, with inline ICS fixtures:
  - a single event with TZID America/Toronto → correct UTC ms;
  - a weekly RRULE stand-up, expanded within the window and not outside it;
  - EXDATE removes one occurrence;
  - a RECURRENCE-ID override moves one occurrence;
  - an all-day event → `all_day = true`;
  - a cancelled event → skipped;
  - ATTENDEE with CN, attendee without CN (mailto local part), the user's own address left out;
  - a malformed file → empty vec, no panic.
  Then `pick()` on parsed events picks the right occurrence.
- [ ] Implement with `icalendar` + `rrule`. Keep everything in `ics.rs` pure: string in, `Vec<Event>` out.
- [ ] Commit `W4: read calendar events from an iCal address (parsing, recurrences, time zones)`.

### Task 3: iCal on Windows (fetch, platform, settings, UI)

- [ ] `calendar/fetch.rs` with `needs_refresh` tested, plus the blocking download (timeout, size cap).
- [ ] `platform/windows/calendar.rs` implements the contract over fetch + ics.
  `commands/calendar.rs` calls `platform::calendar::set_source` on startup and on settings change. That's
  Windows-only behavior; on macOS there's no such function, so gate the call with
  `#[cfg(target_os = "windows")]`.
- [ ] Command `calendar_test_ics(url) -> Result<usize, String>` (events in the next 7 days), Windows only, in
  the invoke list.
- [ ] Frontend: setting `calendarIcsUrl`; the Connections › Calendar row on Windows (address field, help links,
  Test, plain errors); the Privacy line. Find the existing Calendar row and how its connect toggle works; on
  Windows "connected" = a working address saved.
- [ ] Manual check: a public test calendar (Google provides public holiday ICS URLs, e.g.
  `https://calendar.google.com/calendar/ical/en.canadian%23holiday%40group.v.calendar.google.com/public/basic.ics`):
  Test shows a count. Don't store any private URL.
- [ ] Commit `W4: calendar on Windows via the calendar's iCal address`.

### Task 4: Browser tabs on Windows

- [ ] Pure tests first: `windows_result` (a call URL among others → `Urls`; only non-call URLs → `Unknown`;
  empty → `Unknown`).
- [ ] Implement `tab_urls` with UI Automation as designed (`IUIAutomation` via `CoCreateInstance(CUIAutomation)`,
  `FindFirst(TreeScope_Descendants, ClassName == "OmniboxViewViews")`, `GetCurrentPropertyValue(UIA_ValueValuePropertyId)`
  or the Value pattern), with a 300 ms budget. Use `super::com::init()`.
- [ ] Live check (do it): an ignored test `tab_urls_live_smoke`. Start Chrome or Edge if installed, with
  `https://meet.google.com/abc-defg-hij` (a fake code is fine; the tab doesn't need to join) via
  `Start-Process msedge.exe <url>` (Edge is always on Windows 11). Wait ~3 s and call `tab_urls("msedge.exe",
  Chromium)`. Expect `Urls([... "meet.google.com/abc-defg-hij" ...])`. Then point it at `https://example.com`
  and expect `Unknown`. Close only the window you opened (by its title or the process you started) and leave
  the user's other browser windows alone. If Edge is already running with the user's tabs, open a NEW window
  (`msedge.exe --new-window <url>`) and close only that window. Report what happened.
- [ ] `browsers_status` on Windows: installed → `allowed`. Check the Connections UI copy on Windows.
- [ ] Commit `W4: browser call tabs on Windows (UI Automation, active tab)`.

### Task 5: Docs
- [ ] WINDOWS-PORT.md: W4 done, the spike result, the iCal decision, the active-tab limitation, and hand-checks
  for Miguel (paste the Google secret iCal address and see a recording take its event title; a Meet call in
  Edge/Chrome on Always auto-starts; YouTube in front doesn't). Commit `W4: docs`.
