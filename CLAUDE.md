# Talaan — guide for Claude Code

> Generated from the Notion pages **Talaan Journey** and **Talaan v1 Build Plan** (Vibe Coding Journey). Notion is the source of truth: when a decision changes, update Notion first, then regenerate this file. Never the reverse.

## What Talaan is

A bot-free meeting recorder. It records the mic and system audio as two tracks, so no bot joins the call and it works with any app (Zoom, Meet, Teams, Slack huddles, FaceTime, browser calls). When the call ends: a local transcript labeled Me / Them (plus named speakers within Them), then a summary with action items, saved to an in-app meeting history. Fully offline by default; BYOK cloud summaries are optional and send transcript text only, never audio.

Mac first (Apple silicon, built on Miguel's MacBook). Windows port is v1.1, built on the PC from this codebase.

## Build rules (follow exactly)

1. **Original work.** Written from scratch. Parla/Parlato, Meetily and MarcoZorn/verbale may be read to understand an approach (architecture, settings that work). Then write Talaan's own version. Never paste, copy or translate their code function by function.
2. **Credit only libraries and models** in `THIRD_PARTY_NOTICES.md`. No project credits for reference apps.
3. **License:** MIT.
4. **Baseline first:** empty Tauri app building and running (done in M0), then the full v1 UI with sample data, approved, then components one at a time: Recorder, Transcriber, Meeting history, Summarizer, Call detector.
5. **Commit after every change that builds and works.** CI must stay green.
6. **Parlato's look, Talaan's code.** Match Parlato's design language (values and feel); Talaan has its own token file (`src/styles/tokens.css`) and its own components. Don't copy Parlato's component files.
7. **Notion first:** keep the Talaan Journey decisions and Build Log updated as work happens. Repo docs are generated from Notion.

## How the build runs

- Milestones M0 to M10 in order, continuously. At the start of each, write its task plan in `docs/plans/` (superpowers:writing-plans), then execute with tests and a commit per task.
- Stop only at 🛑 checkpoints: **M1** (UI review), **M3** (real call test), **M5** (first real summary), **M8** (named speakers + Ask).
- TDD where there's logic: transcript merge, echo dedupe, retention, chunking, call rules, summary parsing, storage. UI and native audio get manual checks.
- Internal version `1.0.0-dev`. No tags or GitHub Releases until the v1.0 release gate passes.
- Verify, don't assume: macOS API availability, model licenses, model folder paths and sizes. Note findings in the Build Log.
- Ask before pushing to GitHub.

### Model per milestone

- **Opus:** M0, every milestone's task plan, M3 Recorder, M4 Transcriber, M8 Named speakers and Ask, the local summary model pick in M5, any bug that survives two Sonnet attempts.
- **Sonnet:** M1 UI, M2 Storage, M5 Summarizer wiring, M6 Call detection and menu bar, M7 Meeting features, M9 Organize and connect, M10 Release prep.

## Architecture

- Tauri 2. Rust backend in focused modules (storage, recorder, transcriber, summarizer, call detection) behind Tauri commands. React + TypeScript + Vite + Tailwind 4 frontend talks only to those commands.
- **Platform folders:** all OS-specific code lives in `src-tauri/src/platform/macos/`. `platform/mod.rs` re-exports the active platform; the Windows port adds `platform/windows/` plus one `cfg` line, without touching Mac code. Frontend platform differences follow the same rule.
- Frontend data comes from one `MeetingStore` interface (mock in M1, real in M2), so screens never change when data sources do.
- Stack: whisper.cpp (Whisper), ONNX runtime (Parakeet, Nemotron), llama.cpp (local summary AI), Core Audio process taps with ScreenCaptureKit fallback, macOS Keychain, GitHub Actions.

```
src/                    React app
  styles/tokens.css     Talaan design tokens (light + dark)
  lib/                  shared frontend logic (theme, ...)
src-tauri/src/
  lib.rs                Tauri builder + commands
  platform/mod.rs       picks the OS module
  platform/macos/       Mac-only code
docs/plans/             per-milestone task plans
```

## Design language

- Shared with Parlato: Bricolage Grotesque (sans) + IBM Plex Mono (mono), bundled offline via fontsource. Cream `#F3F2EE`, ink `#141416`, lime `#B5E61D`. 1.5px edges, hard offset shadows (3px / 5px), 12px radius.
- Main window background: **plain cream, no pattern**.
- UX is Talaan's own (UI v2): recording lives in the menu bar; main window is a split view (meetings by day | open meeting); meeting view shows summary and transcript side by side with clickable timestamps; Settings is a separate Mac-style window with tabs (General, Call apps, Speech models, Summary AI, Connections, Privacy).
- Dark mode follows the system with a manual override (`src/lib/theme.ts`, `data-theme` on `<html>`).
- Design reference: Google Drive › MAIN DRIVE - Miguel › FILES - Projects & Work › Talaan - Meeting Minutes › `Design/UI v2/` (README lists every screen). Canvas: https://claude.ai/artifact/6rLWVFfTWDZEBF8ix7UGLv

## Key product decisions

- **Summary format is fixed** for every engine (no user prompt editing): Recap (3–5 sentences), Decisions with timestamps, Action items (owner, due date, timestamp), Open questions. Only the summary language is a setting.
- **Speakers:** Me = mic track, Them = system audio; on-device diarization splits Them into Speaker 1, 2, 3 (renameable).
- **Trigger:** no hotkey. "Record this call?" nudge, Start button in the app, or the menu bar. Per-app rules: Ask (default), Always (still shows a "Recording [app]" banner with Copy notice and Stop), Never. Browser calls are Ask/Never unless tab detection is on.
- **Ignored apps** never trigger: Parlato, VoiceInk, Superwhisper, Wispr Flow, MacWhisper, Apple Dictation, Voice Memos, Talaan.
- **Consent:** on start, a recording reminder plus one-click "copy notice" for the meeting chat.
- **Privacy:** nothing leaves the Mac unless BYOK or Notion export is used. Audio kept 7 days by default (1 / 7 / 30 days / forever / don't keep), pinned meetings keep audio, transcripts kept until deleted. Keys in the Keychain. "Delete all Talaan data" removes everything.
- **Storage:** each meeting is a plain folder: `meeting.json`, `me.wav`, `them.wav` (16 kHz mono), `transcript.json/.md`, `summary.json/.md`, edits beside originals.
- **Model reuse:** offer speech models already on this Mac read-only; show them as "On this Mac", never name the other app. Confirm actual folder paths, don't guess.
- **Signing:** no paid signing. Free self-signed certificate so permissions survive updates; microphone entitlement declared; Metal GPU for speech and local AI.

## Edge cases to honor

Echo (drop Me lines matching Them within ~2 s, speaker warning on built-in output) · crash/sleep/device changes (folder created at start with "recording" status, audio flushed every few seconds, Recover/Discard on launch, follow device switches) · long meetings (chunk on speaker turns, summarize, merge, show progress) · no summary engine (save transcript-only, "Generate summary" later) · language (auto-detect per meeting, override, model picker shows languages) · titles (placeholder "Call, Zoom, Oct 14, 2:30 PM", replaced by summary title, editable) · permissions (explain before macOS asks, deep-link on denial).

## Commands

```bash
npm install
npm run tauri dev                                  # run the app
npm test                                           # frontend tests (Vitest)
cargo test --manifest-path src-tauri/Cargo.toml    # Rust tests
npm run build                                      # type-check + build frontend
```
