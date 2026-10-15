# M9. Organize and connect — task plan

> Generated from Notion "Talaan v1 Build Plan" › M9. Planned on Opus (Miguel: stay on Opus; no 🛑 stops, he tests the whole app at the end).
> Done when: a meeting during a calendar event gets its title and attendees; a meeting exports to a Notion database; a Google Meet tab on Always auto-starts with the banner; a YouTube tab using the mic does not trigger.

## Starting point

- Folders: create, filter, move a meeting (M2). Missing: rename and delete a folder.
- Tags: add/remove per meeting and filter (M2). Missing: auto-tag by call app.
- Settings › Connections exists from M1 with **sample values**: Calendar and tab detection default on, Notion shows "Connected as Miguel" with made-up destinations. A first-run user would see those. All become real and default off.

## Architecture decisions (Opus, binding)

### Folders and tags
- `storage/library.rs`: `rename_folder`, `delete_folder` (its meetings move to no folder; nothing else is deleted). Commands + FolderNav: a folder's ⋯ menu with Rename and Delete (confirm).
- Setting `autoTagApps` (default off): a meeting created by a call app gets the app's name as a tag (`zoom`, `slack`, `google meet`…); manual recordings and imports get none.

### Calendar (EventKit)
- `platform/macos/calendar.rs` (objc2-event-kit): access status, request full access (macOS 14+ API), and "events overlapping [from, to]" returning title, start, end, attendee names (EKParticipant names; the user's own entry is skipped). All-day events are ignored.
- Pure picker in `calendar/` (cross-platform, TDD): given events and the recording start, choose the event that contains the start, else the one starting within 10 minutes after it; prefer the shortest when several overlap (a 30-min call inside an all-afternoon block).
- When a recording starts and Calendar is on and allowed: the meeting gets `calendar_event` (title, attendees) and the event title replaces the placeholder title (not a placeholder any more, so the summary keeps it). Checked again at stop, for calls started a few minutes early.
- Attendees go into the summary prompt so action-item owners use real names. Speaker chips already suggest attendees (`suggestSpeakerNames`).
- Settings: the Calendar switch asks macOS the first time it's turned on; denied shows a link to Privacy & Security › Calendars. `NSCalendarsFullAccessUsageDescription` in Info.plist.

### Notion export
- No Talaan server, so no public OAuth (it needs a client secret). The user connects with an **internal integration secret** they create in Notion and share pages with; stored in secure key storage like API keys. Settings explains the two steps with a link.
- `connect/notion.rs` (blocking reqwest, Notion-Version pinned): `whoami` (workspace name), `destinations` (pages and databases shared with the integration, via search), `export` (summary as headings + bullets + to-dos with timestamps; transcript optional as a toggle block of paragraphs; batched to the 100-children limit). Into a database: a new row whose title property is the meeting title, body as above; into a page: a child page.
- Export sheet's Notion option becomes real (destination picker defaulting to Settings, "Include transcript"), returns the new page URL; "Open in Notion".
- Privacy › What leaves this computer lists Notion (only when you click Export).
- Tested against a local fake Notion server (no real secret here).

### Browser tab detection
- `platform/macos/browser_tabs.rs`: the URL of every tab via Apple Events (`osascript`-free: NSAppleScript in-process) for Chrome-family browsers (Chrome, Arc, Edge, Brave, Comet) and Safari, only for a browser that currently holds the mic, only when the setting is on. `NSAppleEventsUsageDescription` in Info.plist; macOS asks once per browser.
- Pure classifier in `calls/` (TDD): `meet.google.com/abc-defg-hij`, `*.zoom.us/wc/…` and `app.zoom.us/wc/…`, `teams.microsoft.com` / `teams.live.com` meeting paths → that call (with its name: "Google Meet", "Zoom", "Teams"). Lobby pages and everything else → none.
- Rules: with detection on, a browser on the mic **with** a call tab behaves as its own call ("Google Meet") and can use Always; **without** one it doesn't trigger at all (YouTube, a voice recorder site). With detection off, browsers stay Ask/Never as now. Permission denied for a browser → treated as detection off for that browser (Ask).
- Connections tab shows each browser's real state (allowed / not asked / denied / not installed).

## Tasks
1. Folders rename/delete + auto-tag (storage TDD, commands, UI).
2. Real Connections defaults and states (no sample values).
3. Calendar: picker (TDD), EventKit module, wiring at start/stop, prompt attendees, Settings permission flow.
4. Notion: client (TDD against a fake server), commands, Settings connect flow, Export sheet, Privacy list.
5. Browser tabs: classifier (TDD), Apple Events module, rules integration (TDD), Settings states.
6. Checks: calendar on a test event (headless EventKit read), Notion against the fake server, Meet tab via the debug mic hook + a real Chrome tab if allowed; docs, Build Log.

## Out of scope
Notion OAuth (needs a server), Firefox tab detection (no AppleScript tab URLs), Google/Microsoft calendar APIs (EventKit already syncs them when added to macOS Calendar).
