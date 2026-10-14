# M7. Meeting features — task plan

> Generated from Notion "Talaan v1 Build Plan" › M7. Model: **Opus** (Miguel: stay on Opus, no stops).
> Done when: each feature works on a real meeting; edits survive restart and revert restores the original; search finds a word from a transcript.

## Already done in earlier milestones

Timestamp → transcript jump (M1), edit transcript and summary with Revert, edits beside untouched originals (M2), Copy as text / for email (M1), search across titles, summaries and transcripts (M2 `matches_query`), delete, pin (pinned keeps audio), rename (M2).

## Architecture decisions (Opus, binding)

### Playback

- Both tracks are already on the recording clock (pauses cut out), so transcript times are audio times.
- `meetings_playback(id)` mixes `me.wav` + `them.wav` into `playback.wav` (16 kHz mono, sum with soft clipping) the first time someone presses Play, and returns its path. Streaming mix in Rust, a few seconds for a 2-hour call, constant memory. `playback.wav` is deleted with the audio (retention, delete audio) and when either track changes.
- Frontend: one `<audio>` element through Tauri's asset protocol (`protocol-asset`, scope limited to Talaan's data folder). Clicking a time seeks and plays; the transcript line under the playhead is highlighted and kept in view; the header shows now / total.

### Export

- Save dialog (`tauri-plugin-dialog`), suggested name from the meeting title. Markdown and plain text come from the existing `toMarkdown` / `toPlainText` (what the user sees: edits included), written by `export_write(path, text)`.
- **PDF:** a print-styled page (`#/print/:id`) in a hidden window, then the macOS print sheet (Save as PDF is built in). No PDF library to bundle, and the result looks like the app.

### Import

- Drag a file onto the main window (Tauri drag-drop) or Import audio → Choose file. `meetings_import(path)`: decode with **symphonia** (WAV, MP3, M4A/AAC, FLAC, Ogg/Vorbis, AIFF), downmix, resample to 16 kHz (the recorder's rubato wrapper), write `them.wav` (one track, labeled Them), meeting `app: "import"`, title "Imported: <file name>", status `transcribing` → the usual transcribe + summarize path. Runs on a background thread with progress; unsupported files say so.

## Tasks

1. **Playback mix** (`storage` + `recorder/wav`): `playback_path`, mix with tests (lengths differ, clipping, rebuild when stale), deleted with audio.
2. **Player in the meeting view:** asset protocol config, `<audio>`, play/pause, seek on timestamp click, follow-along highlight, now/total.
3. **Export:** dialog plugin, `export_write`, Markdown / text, PDF via the print page.
4. **Import:** symphonia decode + resample + write, command with progress, drag-drop and Choose file, tests with generated WAV/FLAC fixtures.
5. **Meeting menu:** Generate summary again (regenerate), Delete audio now (keeps transcript), Show in Finder.
6. **Checks:** play a real meeting from a decision's time; export md / txt / pdf; import an m4a (Voice Memos format) end to end; search a transcript word; edit → restart → revert. Docs, notices (symphonia MPL-2.0, dialog plugin), Build Log.
