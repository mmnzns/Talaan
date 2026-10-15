# V-gaps: eight gaps before v1.0

> **For agentic workers:** execute task by task (superpowers:executing-plans). TDD where there is logic; one commit per task that builds and passes.

**Goal:** close eight product gaps found while reviewing v1 against other recorders, before Mac + Windows v1.0 ships.

**Architecture:** every change goes through the existing seams: storage (`src-tauri/src/storage/`) for what is on disk, the transcriber / summarizer workers for processing, thin Tauri commands, `MeetingStore` / `SettingsStore` on the frontend (Tauri + mock), screens in Sinag token classes. A parallel session is porting to Windows and owns `platform/`, `lib.rs`, `commands/{recorder,calls,calendar,ask,summarizer}.rs`, `summarizer/local.rs`, Cargo files and CI: changes there are limited to new lines in the command list.

**Tech stack:** Rust (storage, workers, serde), React + TS + Vitest.

---

## Decisions

- **Your name** (`yourName` in settings, empty by default) is the default display name for Me. It is applied when a meeting is read (`Storage::get_meeting`), so the UI, `transcript.md`, Notion, Ask and the summary prompt all see it, and changing it later updates every meeting that wasn't renamed. A per-meeting rename of Me (or Them) is stored in `meeting.json` `speakers` and wins. New meetings store `"me": "Me"`, which counts as "not renamed".
- **Owner names in summaries:** the prompt tells the model who the user is by name (`"Miguel" is the user`), so new action items say "Miguel". Older summaries whose owner is the literal `Me` are shown and exported with the user's name.
- **Too short:** a transcript with under 1 s of speech in total, or at most two lines that together have fewer than 12 words, is not sent to any engine. The meeting is `ready`, transcript-only, with `summaryError {code: "too_short"}` and a plain reason. The word guard keeps a two-line import of a long monologue summarizable. Generate summary gives the same answer.
- **Transcript only / Summarize automatically:** `summarizeAutomatically` (default on) in settings, `transcriptOnly` per meeting (`meeting.json`). When either says no, the transcriber leaves the meeting `ready` with `summaryError {code: "waiting"}` instead of handing it to the summarizer. Generate summary works as usual. The nudge sets the flag as soon as the recording has a meeting id; the recording popover has a switch.
- **Transcribe again** (⋯ menu, any finished meeting with audio): pick a speech model, confirm. Transcript edits (`transcript.edits.json`) are deleted with a warning first. Speaker names are kept (`meeting.json` `speakers`; diarization numbers speakers by first appearance, so names usually still fit and can be renamed). The summary, its edits and ticked items are removed and the meeting is summarized again (unless it is transcript-only or automatic summaries are off). A title the user typed is kept; a placeholder or summary title can be replaced. The chosen model is used once (`retranscribeModel` in `meeting.json`, cleared when the run starts) and doesn't change the default.
- **Models shown:** `Meeting.transcription` (model id, from `meeting.json`) is returned to the frontend; the ⋯ menu lists "Transcribed with <model name>" and "Summary by <engine>".
- **Export all:** the frontend picks a folder (dialog), then writes one Markdown file per meeting (summary + transcript, as the user sees them) named `YYYY-MM-DD Title.md`, de-duplicated, through `export_write`.
- **Open folder:** new `data_reveal` command reveals the data folder (same opener call as the per-meeting reveal).

## Tasks

1. **Reasoning blocks** — `summarizer/parse.rs`: `strip_reasoning(text)` removes `<think>…</think>` blocks (and an unclosed `<think>` up to the end) before `extract_json`. Tests: braces inside the block, unclosed block, no block.
2. **Too short** — `summarizer/summarize.rs`: `SummarizeError::TooShort`, `too_short(lines)`; `summarizer/worker.rs` maps it to `summary_error too_short`. Tests in both.
3. **Your name / Me and Them** — settings `yourName` (TS + defaults), storage `get_meeting` applies it (test), `prompt.rs` RULES name the user (test), `markdown.rs` maps owner `Me` (test), TS `ownerName` used in `share.ts` and SummaryCard (test), SpeakersBar renames Me and Them, Settings › General "Your name" row, mock store applies it.
4. **Privacy placeholders** — `data_reveal` command; `SettingsStore.revealData()`; `lib/exportAll.ts` (pure `exportAllFiles` tested + `exportAll` with folder dialog); PrivacyTab buttons.
5. **Summarize automatically / Transcript only** — `MeetingFile.transcript_only`, transcriber worker gate (test), `meetings_set_transcript_only` command, store methods (Tauri + mock), Settings › Summary AI switch, RecordingPopover switch, Nudge checkbox + PromptWindow wiring, SummaryCard message for `waiting`.
6. **Transcribe again** — `MeetingFile.retranscribe_model`, `Storage::reset_for_retranscribe` (test: edits, summary files gone, speakers kept), worker uses the one-off model (test), `meetings_retranscribe` command, store methods, ⋯ menu item + Sheet with model picker and warning.
7. **Find in transcript** — `lib/find.ts` `findMatches` (tests), TranscriptCard search box with count, ↑/↓ (Enter / Shift+Enter), highlighted `<mark>`, scroll to the current match.
8. **Show the models** — `Meeting.transcription` in Rust model + TS type; ⋯ menu shows speech model name and summary engine.

Each task: tests first where there is logic, `npm test`, `npm run build`, `cargo test` for the touched modules, commit.
