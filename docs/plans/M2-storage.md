# M2. Storage and data model — task plan

> Generated from Notion "Talaan v1 Build Plan" › M2. Model: **Sonnet** (plan written by Opus). No 🛑 checkpoint; go straight on to M3 planning when done.
> Done when: tests pass for create, read, update, delete, retention (expired audio removed, pinned kept) and recovery detection; the app runs on real storage.

## Goal

Meetings live on disk as plain folders that anyone can read without Talaan. A Rust `storage` module owns them, Tauri commands expose it, and a `TauriMeetingStore` / `TauriSettingsStore` replace the mocks inside the Tauri app. Screens don't change.

## Architecture decisions (Opus, binding)

### Where data lives

- Root: `dirs::data_dir()/Talaan` → `~/Library/Application Support/Talaan` on Mac, `%APPDATA%\Talaan` on Windows (same code, no platform folder needed). Override with env `TALAAN_DATA_DIR` (tests, and for a throwaway dev library).
- The storage module takes a root `PathBuf` and never calls Tauri, so it's unit-testable with `tempfile`.

```
Talaan/
  settings.json                 frontend-owned settings blob (see below)
  library.json                  { "schemaVersion": 1, "folders": [{ "id", "name" }] }
  meetings/
    2026-10-14_1430_k3f9/       id = local start time + 4 random [a-z0-9]; sortable and readable
      meeting.json              metadata (schema below)
      me.wav  them.wav          written by the recorder in M3; deleted by retention
      transcript.json           original from the transcriber (never rewritten after M4 writes it)
      transcript.edits.json     { "<lineId>": "edited text" }   only if edited
      transcript.md             generated from the effective (edited) transcript
      summary.json              original from the summarizer
      summary.edited.json       full edited Summary              only if edited
      summary.md                generated from the effective summary
```

Edits live beside originals (plan requirement). **Revert = delete the two edit files and regenerate the `.md` files.** Readers merge original + edits; `edited` flags are computed on read, never stored.

### meeting.json

Rust structs mirror `src/data/types.ts` with `#[serde(rename_all = "camelCase")]`. Persisted fields:

`schemaVersion` (1), `id`, `title`, `titleIsPlaceholder`, `app`, `appLabel`, `startedAt` (RFC 3339 with offset), `durationMs`, `language`, `status`, `pinned`, `folderId?`, `tags`, `peopleCount`, `calendarEvent?`, `speakers` (map `"me"|"them:N"` → name), `marks`, `pauses`, `engine?` (summary engine used), `audioDeleted` (bool).

Derived on read, **not stored**: `hasAudio` (audio files present), `audioKeptUntil` (startedAt + retention days while audio exists; `None` when pinned or retention is `forever`), `progress` (in-memory job registry, empty until M4/M5), `summary`, `transcript`, edited flags.

Fix the UI rule while here: header chip shows "Audio kept" when `pinned && hasAudio`, "Audio until …" when `audioKeptUntil`, else "No audio kept". Add `hasAudio: boolean` to `Meeting` (additive).

### Writes

- Every write is atomic: write `<file>.tmp` in the same folder, `fsync`, `rename`. Helper `write_json_atomic`.
- One `Mutex<Storage>` in Tauri state; commands lock it. Single process, so no file locking.
- Unknown fields in `meeting.json` are preserved (`#[serde(flatten)] extra: Map`), so newer builds' data survives older builds.
- `schemaVersion` > supported → that meeting is listed with status `failed` and a clear error, never overwritten.

### Status, recovery and crash marker

- `create_meeting(app, appLabel, startedAt)` creates the folder with `status: "recording"` and a placeholder title (`"Call, Zoom, Oct 14, 2:30 PM"`, `titleIsPlaceholder: true`). M3 calls this the moment recording starts.
- On startup, `scan_for_interrupted()` flips every `recording` meeting to `needs_recovery`. (Nothing can be legitimately recording at launch.)
- `recover_meeting`, `retry_processing`, `generate_summary` exist as commands but return `Err(NotYetAvailable("…M4/M5"))` in M2. The TS store turns that into a toast. They become real in M4 (transcribe) and M5 (summarize). Discard/Delete work fully.

### Retention

- Pure function `apply_retention(meetings_dir, policy, now) -> RetentionReport` in `storage::retention`. Policy from settings `retention`: `1d | 7d | 30d | forever | none`.
- For each meeting with audio, not pinned, not `recording`: if `none` or `startedAt + days <= now` → delete `me.wav`/`them.wav`, set `audioDeleted: true`. `forever` never deletes.
- Runs at startup and every hour (Tokio interval in a spawned task), and right after the retention setting changes.
- Transcript retention (`transcriptRetention`: forever / 1y / 90d) uses the same pass: deletes the whole meeting folder unless pinned. Test it too.

### Settings

- `settings.json` holds the frontend `Settings` object as opaque JSON. Rust reads only what it needs through a typed view (`retention`, `transcriptRetention`; later milestones add fields). Defaults stay in TS (`defaultSettings`), merged on load as today.
- Commands: `settings_get` → `Value | null`, `settings_set(value)`. Emits `settings-changed` to all windows (replaces the localStorage `storage` event).
- `downloadModel` / `testApiKey` stay simulated in `TauriSettingsStore` until M4/M5 (delegate to the mock helpers). `deleteAllData` is real: remove the root folder, then restart the app (`tauri::process::restart` via `app.restart()`), which lands in onboarding. Keychain and model cleanup are added to it in M5/M4.

### Events

After any mutation the backend emits `meetings-changed` (no payload). `TauriMeetingStore.subscribe` listens with `@tauri-apps/api/event`. Both windows stay in sync.

### Frontend wiring

- `src/data/tauri/meetingStore.ts`, `src/data/tauri/settingsStore.ts` implement the interfaces via `invoke`.
- `src/data/index.ts`: `inTauri() ? Tauri stores : mock stores`. The browser (`npm run dev`) keeps the mock, so screenshots and quick UI work still run without the backend.
- `ask` stays mock-backed in the Tauri store (real in M8): returns the empty-answer state, not canned pricing answers.
- `listMeetings` filtering (folder, tag, query) happens in Rust. Query is a simple case-insensitive scan of title, summary and transcript text; M7 replaces it with a real index.

### Dev data

Debug builds only (`#[cfg(debug_assertions)]`): command `dev_seed(meetings: Vec<Meeting>)` writes meetings (with transcript and summary files, no audio) and `dev_wipe()`. The Screens stage gets a **Dev data** section with "Load sample meetings" (uses `seedMeetings()` from the mock) and "Wipe library". This keeps the M1 click-through usable on real storage.

### Contract check between Rust and TS

One fixture, `fixtures/meeting.sample.json` (the Q4 roadmap sync with summary, edits, marks, pauses). Rust test: deserializes it and round-trips it unchanged. TS test: `import sample from "../../fixtures/meeting.sample.json"` then `const m: Meeting = sample satisfies Meeting` (compile-time) plus a shape assertion. Either side drifting breaks a test.

### Crates

`serde`, `serde_json`, `chrono` (serde feature), `dirs`, `rand`, `thiserror`, `tokio` (already via Tauri; use `tauri::async_runtime`), dev: `tempfile`. Add each to `THIRD_PARTY_NOTICES.md` with its license (all MIT/Apache-2.0; verify).

## Module layout

```
src-tauri/src/
  storage/
    mod.rs          Storage { root }, public API, errors (thiserror)
    model.rs        MeetingFile, Meeting, TranscriptLine, Summary, ... (serde)
    paths.rs        layout, id generation, placeholder titles
    io.rs           write_json_atomic, read_json, file presence
    markdown.rs     transcript.md / summary.md rendering
    retention.rs    apply_retention (pure over a dir + policy + now)
    library.rs      folders (library.json), tags (derived)
  commands/
    meetings.rs     #[tauri::command] wrappers, event emission
    settings.rs
    dev.rs          debug-only seed/wipe
  lib.rs            state setup, startup scan + retention task, handler list
```

Errors cross the bridge as `{ code, message }` (serialize the thiserror enum). Codes: `not_found`, `not_yet_available`, `unsupported_schema`, `io`.

## Tasks

TDD for every storage task: write the Rust test first (`tempfile::TempDir` root), see it fail, implement, see it pass. Commit after each task with `M2:` prefix; `cargo test`, `npm test`, `npm run build` green.

1. **Model + fixture.** `model.rs` structs, `fixtures/meeting.sample.json`, Rust round-trip test, TS `satisfies` test. Add `hasAudio` to TS `Meeting`.
2. **Paths and atomic IO.** id generation (format + uniqueness within the same minute), placeholder title, `write_json_atomic` (no `.tmp` left behind; old content intact if serialization fails).
3. **Create / read / list.** `create_meeting`, `get_meeting` (merges transcript, edits, summary, derived fields), `list_meetings` (newest first, filters). Tests: create → get; list order; folder/tag/query filters; missing optional files are fine; corrupt `meeting.json` → listed as `failed`, not a crash; unknown fields preserved after update.
4. **Update / delete.** rename (clears `titleIsPlaceholder`), pin, tags, move folder, speaker rename, delete (folder gone). Tests for each.
5. **Edits beside originals.** `update_transcript_line`, `update_summary`, `revert_edits`, markdown regeneration. Tests: original files byte-identical after edits; edited flags computed; revert removes edit files; `.md` reflects edits, then originals after revert.
6. **Library.** folders in `library.json`, `create_folder` (unique slug ids), counts, tags derived from meetings. Tests.
7. **Recovery detection.** `scan_for_interrupted` flips `recording` → `needs_recovery`; leaves others. Test.
8. **Retention.** `apply_retention` with an injected `now`. Tests: expired audio deleted and `audioDeleted` set; pinned kept; `recording` untouched; `forever` keeps; `none` deletes all unpinned; transcript retention removes old unpinned meetings only; `audioKeptUntil` derivation.
9. **Settings + delete all.** `settings.json` get/set, typed view, `delete_all_data` removes the root. Tests.
10. **Tauri commands + state + events.** Register everything, startup scan, hourly retention task, `meetings-changed` / `settings-changed` events, error mapping. Capabilities: allow `core:event:default`.
11. **TS Tauri stores.** `TauriMeetingStore`, `TauriSettingsStore`, `index.ts` switch, `NotYetAvailable` → toast in the three callers (StatusBanner, Generate summary). Vitest for the error mapping and for `index.ts` choosing mock in a plain browser.
12. **Dev data.** `dev_seed` / `dev_wipe` + Screens stage section.
13. **Manual check in the real app** (`npm run tauri dev`, `TALAAN_DATA_DIR` unset): load sample meetings, rename, edit + revert, tag, folder, pin, delete; quit and relaunch, everything persists; inspect the folder in Finder (`.md` files readable); set retention to "Don't keep" and confirm audio-less meetings are unaffected; create a fake `recording` meeting folder by hand, relaunch, see it as interrupted. Note results in the Build Log. Update CLAUDE.md file map and THIRD_PARTY_NOTICES.

## Done checks

- `cargo test` covers create, read, update, delete, retention (expired removed, pinned kept) and recovery detection, all green; CI green.
- The Tauri app shows only what's on disk; the browser dev server still shows the mock.
- A meeting folder opened in Finder is understandable without Talaan.
- No screen file changed except the three NotYetAvailable callers, the audio chip rule and the Screens stage dev section.

## Outcome (2026-10-14)

All 13 tasks done: 72 Rust tests and 65 TS tests, CI green. Differences from the plan:

- **Done flags** are stored in `summary.done.json` so ticking an action item is not an "edit" (the plan would have created `summary.edited.json` on every tick).
- **Retention** also spares meetings that still need their audio (recording, needs recovery, transcribing, summarizing), and "Don't keep" spares failed meetings so they can be retried. Pinned meetings are exempt from transcript expiry as well.
- **`data_info`** command (path and size) added so the Privacy tab no longer shows a fake "1.2 GB".
- `dev_seed` converts meetings that were mid-processing to `ready`, so the sample data has no stuck jobs.

Manual check in the real app (no `TALAAN_DATA_DIR`): sample data loaded onto disk and readable in Finder; transcript edit created `transcript.edits.json` with the original byte-identical; Revert removed it; action-item tick, pin, new tag and new folder persisted across a restart; search ran in Rust; Delete removed the folder; a hand-made `recording` folder became `needs_recovery` on relaunch; choosing "Don't keep" in Settings removed audio at once and spared pinned and interrupted meetings; "Delete everything" emptied the folder and restarted.
