# Talaan — guide for Claude Code

> Generated from the Notion pages **Talaan Journey** and **Talaan v1 Build Plan** (Vibe Coding Journey). Notion is the source of truth: when a decision changes, update Notion first, then regenerate this file. Never the reverse.

## What Talaan is

A bot-free meeting recorder. It records the mic and system audio as two tracks, so no bot joins the call and it works with any app (Zoom, Meet, Teams, Slack huddles, FaceTime, browser calls). When the call ends: a local transcript labeled Me / Them (plus named speakers within Them), then a summary with action items, saved to an in-app meeting history. Fully offline by default; BYOK cloud summaries are optional and send transcript text only, never audio.

v1.0 ships Mac (Apple silicon, built on Miguel's MacBook) and Windows (x64, built on the PC and on CI) together, from this codebase.

## Build rules (follow exactly)

1. **Original work.** Written from scratch. Parla/Parlato, Meetily and MarcoZorn/verbale may be read to understand an approach (architecture, settings that work). Then write Talaan's own version. Never paste, copy or translate their code function by function.
2. **Credit only libraries and models** in `THIRD_PARTY_NOTICES.md`. No project credits for reference apps.
3. **License:** MIT.
4. **Baseline first:** empty Tauri app building and running (done in M0), then the full v1 UI with sample data, approved, then components one at a time: Recorder, Transcriber, Meeting history, Summarizer, Call detector.
5. **Commit after every change that builds and works.** CI must stay green.
6. **Talaan's own look ("Sinag", UI v3).** Talaan has its own identity reflecting Miguel's Filipino heritage (decided 2026-10-14, replacing the shared Parlato look). Its own token file (`src/styles/tokens.css`) and components. Don't copy Parlato's component files.
7. **Notion first:** keep the Talaan Journey decisions and Build Log updated as work happens. Repo docs are generated from Notion.

## How the build runs

- Milestones M0 to M10 in order, continuously. At the start of each, write its task plan in `docs/plans/` (superpowers:writing-plans), then execute with tests and a commit per task.
- Stop only at 🛑 checkpoints: **M1** (UI review), **M3** (real call test), **M5** (first real summary), **M8** (named speakers + Ask).
- TDD where there's logic: transcript merge, echo dedupe, retention, chunking, call rules, summary parsing, storage. UI and native audio get manual checks.
- Version `1.0.0` (bumped 2026-10-15, ready to tag). Miguel approved releasing v1.0.0 from the Mac session: tag `v1.0.0` on main, let CI draft the release with Windows, then add the Mac build (docs/TESTING.md, "Before publishing"). Later releases: bump, tag, same flow.
- Verify, don't assume: macOS API availability, model licenses, model folder paths and sizes. Note findings in the Build Log.
- Ask before pushing to GitHub.
- **Never link llama.cpp into the main app.** It lives in the `talaan-llm` helper because whisper.cpp and llama.cpp each bundle ggml (see docs/plans/M5-summarizer.md).

### Model per milestone

- **Opus:** M0, every milestone's task plan, M3 Recorder, M4 Transcriber, M8 Named speakers and Ask, the local summary model pick in M5, any bug that survives two Sonnet attempts.
- **Sonnet:** M1 UI, M2 Storage, M5 Summarizer wiring, M6 Call detection and menu bar, M7 Meeting features, M9 Organize and connect, M10 Release prep.

## Architecture

- Tauri 2. Rust backend in focused modules (storage, recorder, transcriber, summarizer, call detection) behind Tauri commands. React + TypeScript + Vite + Tailwind 4 frontend talks only to those commands.
- **Platform facade:** OS-specific code lives in `src-tauri/src/platform/macos/` and `platform/windows/`, which expose the same items; `platform/mod.rs` re-exports the one being built, so callers say `crate::platform::…`. Where a flow differs, shared files also carry `cfg(target_os = …)` blocks or `cfg!(windows)` values (lib.rs, commands/calendar.rs, calls.rs, mod.rs, recorder.rs, settings.rs, tray.rs, summarizer/local.rs); the Windows port touched those shared files, so check both platforms when changing them. Frontend: `src/lib/platform.ts` (`onWindows`, and `usePlatform()` for the words that differ).
- **Languages:** the UI is in English, French (Canadian) and Spanish: `src/lib/i18n` for the screens, `src-tauri/src/i18n.rs` (`tr`, `tr_with`, `tr_err`) for what Rust shows (tray, notifications, errors). Rules and what stays English: `docs/I18N.md`.
- Frontend data comes from one `MeetingStore` interface (mock in M1, real in M2), so screens never change when data sources do.
- Stack: speech (whisper.cpp with Metal, ONNX Runtime for Parakeet; default model Whisper Large Turbo) and local summary AI (llama.cpp in a separate helper process `talaan-llm`, default Qwen3.5 4B) come from voice-engine; models live in its shared folder; Core Audio process taps (macOS 14.4+, no ScreenCaptureKit fallback), macOS Keychain, GitHub Actions.

```
src/
  main.tsx, App.tsx     providers, theme, route switch (main | settings | screens | onboarding)
  styles/tokens.css     Talaan design tokens (light + dark); index.css maps them to Tailwind colors
  brand/                logo (the only place with raw colors besides tokens)
  data/                 the contract between screens and data
    types.ts            Meeting, Summary, TranscriptLine, ...
    store.ts            MeetingStore interface (mock now, real in M2)
    settings.ts         Settings + SettingsStore interface
    StoreProvider.tsx   React context; hooks.ts = useMeetings/useMeeting/useSettings (live, subscribe-based)
    index.ts            the only place the mock stores are wired in
    mock/               sample data, MockMeetingStore, MockSettingsStore (localStorage)
  lib/                  format, router (hash), share (Slack/email/Markdown/text), transcript, theme, windows, platform
    i18n/               UI strings in en/fr/es (messages/<area>.ts), t / useT; see docs/I18N.md
  ui/                   Talaan's component kit (Button, Chip, Sheet, Popover, Toast, ...) - no hex colors
  screens/
    Shell/              toolbar + main window layout
    MeetingList/        left column (search, folders, tags, day groups)
    Meeting/            header, summary, transcript, speakers, export sheet, status banners
    Ask/  Settings/  Onboarding/
    Recording/          menu bar popover, recording popover, call prompts (components)
    MenuBar/            the real windows: TrayWindow (#/tray) and PromptWindow (#/prompt)
    Stage/              dev-only `#/screens` stage that hosts the non-window surfaces
src-tauri/src/
  lib.rs                Tauri builder, startup (scan for interrupted meetings, retention), command list
  commands/             thin Tauri commands (meetings, settings, dev) + AppState (one Mutex<Storage>)
  storage/              all disk access; no Tauri types, fully unit-tested with tempdirs
    model.rs            serde types that mirror src/data/types.ts
    store.rs            Storage: create / get / list / filters / search
    ops.rs  edits.rs    rename, pin, tags, folder, speakers, delete; edits beside originals + revert
    library.rs          folders (library.json) and derived tags
    retention.rs        policy + the pass that deletes expired audio / old transcripts
    recovery.rs         interrupted recordings -> needs_recovery
    settings.rs         settings.json + delete-all
    markdown.rs  io.rs  paths.rs  dev.rs
  commands/recorder.rs  RecorderState, recorder_* / permissions_* commands, 10 Hz levels loop, watchers, stop on quit
  recorder/             cross-platform recording core (no objc2 here)
    engine.rs session.rs  Engine (start/pause/mark/stop/discard, device restarts), Session (marks, pauses, meeting.json saves)
    clock.rs            recording clock that excludes paused time
    pipeline.rs timeline.rs  host time -> sample position (resync on drift / rate change), gap fill, overlap trim
    resample.rs wav.rs levels.rs  rubato to 16 kHz mono, hound WAV with 2 s flush + repair_wav, RMS levels
    feed.rs source.rs   rtrb SPSC feed from audio callbacks, AudioSource trait
    mic.rs recovery.rs  cpal mic source, crash recovery (repair WAVs, duration)
    playback.rs         both tracks mixed into playback.wav on first Play (deleted with the audio)
  commands/transcriber.rs  worker setup, meetings_retry, models_list / download / cancel / delete
  transcriber/          speech to text (no Tauri types)
    models.rs speech.rs  Talaan's order of recommendation over the engine's catalog; speech model state from the shared folder
    vad.rs              read tracks, Silero VAD, chunk shaping per engine (pad, join, split at quiet points, pack)
    engine.rs           Engine trait; Parakeet (silence padding, resume when it stops early) and Whisper (Metal)
    helper_speech.rs    Windows: Whisper in the engine's GPU helper (SttClient), the processor taking over on a failure
    pipeline.rs         tracks -> chunks -> engine -> retry skipped speech -> echo -> merge -> language
    echo.rs merge.rs    drop Me lines the mic heard from the speakers; interleave and join into lines
    diarize.rs          named speakers: Nemotron 3 Diarization (the engine's diarize) on the Them track, warm-up pass, Speaker 1, 2, 3
    worker.rs           one background thread, queue rebuilt from `transcribing` at launch, idle unload
  commands/summarizer.rs  summary worker setup, engine factory from settings, Generate summary, LLM models, keys, local servers
  summarizer/           summaries (no Tauri types)
    prompt.rs parse.rs  the one fixed prompt + JSON schema; reply parsing, time snapping, title
    chunk.rs summarize.rs  split long calls on speaker turns, summarize parts, merge; one retry on bad output
    engines.rs          local helper, Claude, OpenAI, Gemini, OpenAI-compatible (Ollama, LM Studio); model lists
    local.rs            LlmClient for the talaan-llm helper; local models are the engine's catalog (Qwen3.5 4B / 9B, Granite 4.2 3B)
    worker.rs           summarizing -> ready (with summary_error when there's no engine or it failed)
  ask/                  Ask (no Tauri types): passages.rs (transcript windows + summary items), search.rs (BM25, cosine,
                        rank fusion), index.rs (search.bin vectors beside each meeting), answer.rs (prompt, schema, citations)
  commands/ask.rs       meetings_ask (scope -> passages -> rank -> summary engine) and the background indexer
  keys.rs               secure key storage (Keychain on the Mac, Credential Manager on Windows): API keys, the Notion
                        secret and the Windows calendar's iCal address (`calendar-ics`)
  i18n.rs               Rust's strings in English, French and Spanish (docs/I18N.md)
  importer.rs           import audio: symphonia decode -> 16 kHz mono them.wav (commands/import.rs runs it)
  calls/                call detection (pure): apps.rs bundle-id tables (call apps, ignored apps), rules.rs state machine,
                        tabs.rs (browser helpers → browser, call URLs, what a browser on the mic is)
  calendar/             which calendar event a recording belongs to (pure); EventKit read in platform/macos/calendar.rs
  notion.rs             Notion export: page blocks, client (search, pages, append); commands/notion.rs
  commands/calls.rs     1 Hz loop: mic users -> rules -> recordings and the prompt window (nudge, banner, call ended)
  commands/tray.rs      menu bar icon + timer, popover window, window_fit, main window hides on close
  llm/  stt/            3-line dev helper crates (`talaan-llm`, `talaan-stt`) that run the engine's helpers; release helpers
                        (incl. Windows Vulkan) are built by the engine's workflow. summarizer/local.rs makes the LlmClients
  models.rs             `Models`: the shared model folder (SharedStore), the one-time move in, claim and collect
  gpu.rs                the card default (Mac on; Windows on only with a discrete card) and what fell back this run
  platform/mod.rs       picks the OS module
  platform/macos/       Mac-only code
    system_tap.rs       Core Audio global process tap (excludes Talaan) + private aggregate device + IOProc
    coreaudio.rs clock.rs  property helpers, mach host clock in ns
    permissions.rs devices.rs power.rs  mic permission + deep links, built-in speakers / device watcher, sleep auto-pause
    notify.rs thread.rs  Summary ready notifications (click opens the meeting), background thread priority
    mic_users.rs        which processes are using the mic (Core Audio process objects)
    calendar.rs browser_tabs.rs  EventKit access and events; a browser's tab URLs via Apple Events
  platform/windows/     Windows-only code (mirrors macos/)
    system_tap.rs       WASAPI process loopback (excludes Talaan's process tree), reconnects after errors
    clock.rs com.rs     QPC host clock in ns (same grid as WASAPI/cpal), COM init per thread
    permissions.rs devices.rs power.rs  microphone Privacy switches + Settings links, speakers warning / device watcher, sleep auto-pause
    notify.rs thread.rs  Summary ready toasts (W3), background thread priority
    mic_users.rs        apps using the mic (consent store; W2)
    calendar.rs browser_tabs.rs  events from the private iCal address (calendar/fetch.rs, ics.rs); tab URLs via UI Automation
docs/plans/             per-milestone task plans
docs/WINDOWS-PORT.md    what the Windows port has to write, build and package (v1.0 ships Mac + Windows together)
docs/TESTING.md         what Miguel tests before release
docs/screens/M1/        light + dark screenshots from the M1 checkpoint
```

### Storage on disk (M2)

`~/Library/Application Support/Talaan` (override with `TALAAN_DATA_DIR`):

```
settings.json   library.json   meetings/<YYYY-MM-DD_HHMM_xxxx>/
  meeting.json  me.wav  them.wav  transcript.json  transcript.md  summary.json  summary.md
  transcript.edits.json  summary.edited.json  summary.done.json      (only when the user changed something)
```

- Originals (`transcript.json`, `summary.json`) are never rewritten after the transcriber / summarizer write them. Edits sit beside them; Revert deletes the edit files. `.md` files are regenerated from what the user sees.
- Ticking an action item is stored in `summary.done.json` and does not count as an edit.
- Writes are atomic (temp file + rename). Unknown JSON fields are preserved. A meeting written by a newer schema is listed as failed and never touched.
- Pinned meetings and meetings that still need their audio (recording, needs recovery, processing) are never touched by retention.
- Stop and Recover put a meeting in `transcribing`; the transcriber writes `transcript.json` and sets `summarizing`; the summarizer writes `summary.json` and sets `ready` (or `ready` with `summaryError` when no engine is set up or it failed; Generate summary retries). Failures keep `error {code, message}`; `no_model` meetings are re-queued when a model finishes downloading. 
- Models live in the shared folder (`~/Library/Application Support/Craft Concepts Digital/Models`; Windows `%LOCALAPPDATA%\Craft Concepts Digital\Models`), shared with the other apps; their state comes from disk (`models_list`), never from `settings.json`.
- In the Tauri app the stores talk to Rust (`src/data/tauri/`); in a plain browser the sample data is used. Debug builds have **Screens → Dev data** to load the sample meetings onto disk.
- Frontend and Rust share `fixtures/meeting.sample.json`: Rust round-trips it, TS checks its shape.

### Data layer rules

- Screens get data only through `useStore()` / `useSettingsStore()` (and the hooks). They never import from `data/mock`.
- To go real in M2, replace the two lines in `data/index.ts`. The interface may grow, but changes are additive.
- No raw hex colors in `src/ui` or `src/screens` (use the token classes: `bg-paper`, `text-subtle`, `shadow-hard`, ...). Check: `grep -rn "#[0-9A-Fa-f]\{6\}" src/ui src/screens`.
- Routes are hash routes: `#/`, `#/meeting/:id?t=ms`, `#/ask?m=id`, `#/settings/:tab`, `#/screens/:section`.

## Design language

- **Sinag** (Tagalog for "ray of light"), Design/UI v3. Syne (headings, wordmark, day numbers; `font-display`), Instrument Sans (UI text), DM Mono (timestamps, lowercase labels), all bundled offline via fontsource.
- Colors: warm off-white `#FAFAF7` and white, near-black ink `#111318`. **Primary buttons are ink, never a color.** Sun gold `#F5B700` for highlights, progress and the active moment; pastel gold `#FFF3C4`, blue `#E8EEFB` and red `#FCE8E8` for tags, highlights and people; flag red `#E0262F` only for the recording dot. Hairline borders, soft shadows, 9 to 18px corners, generous spacing.
- Logo: the Baybayin "ta" (traced from Miguel's reference) in gold ink (`src/brand/Logo.tsx`). Never put a dot directly above or below the letter (a kudlit changes the sound). The faint sun with three stars grouped beside it sits small in top-right corners (`src/brand/SunCorner.tsx`); it is not the national emblem.
- Labels are lowercase mono (`Label`, `SectionLabel` with the short gold rule). Copy is platform-neutral (one design for Mac and Windows).
- UX is Talaan's own (UI v2): recording lives in the menu bar; main window is a split view (meetings by day | open meeting); meeting view shows summary and transcript side by side with clickable timestamps; Settings is a separate Mac-style window with tabs (General, Call apps, Speech models, Summary AI, Connections, Privacy).
- Dark mode follows the system with a manual override (`src/lib/theme.ts`, `data-theme` on `<html>`).
- Light/dark only in Settings › General (Auto, Light, Dark); no theme button in the toolbar.
- Design reference: Google Drive › MAIN DRIVE - Miguel › FILES - Projects & Work › Talaan - Meeting Minutes › `Design/UI v3/` (README, one `*3.dc.html` per screen, logo and motif SVGs). Canvas: https://claude.ai/artifact/6rLWVFfTWDZEBF8ix7UGLv

## Key product decisions

- **Summary format is fixed** for every engine (no user prompt editing): Recap (3–5 sentences), Decisions with timestamps, Action items (owner, due date, timestamp), Open questions. Only the summary language is a setting.
- **Speakers:** Me = mic track, Them = system audio; on-device diarization splits Them into Speaker 1, 2, 3 (renameable).
- **Trigger:** no hotkey. "Record this call?" nudge, Start button in the app, or the menu bar. Per-app rules: Ask (default), Always (still shows a "Recording [app]" banner with Copy notice and Stop), Never. Browser calls are Ask/Never unless tab detection is on.
- **Ignored apps** never trigger: Parlato, VoiceInk, Superwhisper, Wispr Flow, MacWhisper, Apple Dictation, Voice Memos, Talaan.
- **Consent:** on start, a recording reminder plus one-click "copy notice" for the meeting chat.
- **Privacy:** nothing leaves the Mac unless BYOK or Notion export is used. Audio kept 7 days by default (1 / 7 / 30 days / forever / don't keep), pinned meetings keep audio, transcripts kept until deleted. Keys in the Keychain. "Delete all Talaan data" removes everything.
- **Storage:** each meeting is a plain folder: `meeting.json`, `me.wav`, `them.wav` (16 kHz mono), `transcript.json/.md`, `summary.json/.md`, edits beside originals.
- **Shared models:** every app of Craft Concepts Digital uses one model folder, so a model downloads once; Delete releases it for Talaan only (the engine erases it when no other app uses it).
- **Signing:** no paid signing. Free self-signed certificate so permissions survive updates; microphone entitlement declared; Metal GPU for speech and local AI.

## Edge cases to honor

Echo (drop Me lines matching Them within ~2 s, speaker warning on built-in output) · crash/sleep/device changes (folder created at start with "recording" status, audio flushed every few seconds, Recover/Discard on launch, follow device switches) · long meetings (chunk on speaker turns, summarize, merge, show progress) · no summary engine (save transcript-only, "Generate summary" later) · language (auto-detect per meeting, override, model picker shows languages) · titles (placeholder "Call, Zoom, Oct 14, 2:30 PM", replaced by summary title, editable) · permissions (explain before macOS asks, deep-link on denial).

## Shared engine (voice-engine)

The local AI helper, the helper protocol, speech model runtimes, the model catalog, the shared model folder and the
downloader come from https://github.com/mmnzns/voice-engine, pinned by tag in src-tauri/Cargo.toml.

- Sonnet does the heavy lifting, no exceptions: hand every piece of development work Sonnet can handle to Sonnet
  subagents. Opus only plans, briefs, reviews and decides.
- Before working on AI, speech or models, read the engine change log on the Notion page "Voice Apps Engine Sync".
- Never edit engine code from this repository. If the engine needs a change, open an issue on mmnzns/voice-engine.
- Move to a new engine tag only on purpose, on a branch, after the tests and a manual check pass. Then update this
  app's column in the change log.
- A local `src-tauri/.cargo/config.toml` [patch] pointing at ~/Developer/voice-engine is for trying changes only.
  Never commit it; releases build against the tag.

Talaan keeps the engine's build settings in the tracked root `.cargo/config.toml` (cargo reads it from the repo root and from `src-tauri`). Release builds have no LTO: the engine's Metal code doesn't link with it on macOS.

## Commands

```bash
npm install
npm run tauri dev                                  # run the app (real storage)
npm run dev                                        # same UI in a browser with sample data
npm test                                           # frontend tests (Vitest)
cargo test --manifest-path src-tauri/Cargo.toml    # Rust tests
TALAAN_SMOKE_MEETING=<meeting dir> cargo test --release --manifest-path src-tauri/Cargo.toml pipeline_smoke -- --ignored --nocapture   # real models on a real call
npm run build                                      # type-check + build frontend
scripts/build-signed-debug.sh                      # signed debug .app (needed for real mic/system-audio permissions)
scripts/build-llm-helper.sh                        # builds the dev helpers (tauri dev/build run it first)
powershell -File scripts/build-llm-helper.ps1       # Windows: builds the dev helpers
```

On Windows `npm run tauri dev` needs no signing for mic or system audio.

`npm run tauri dev` attributes permissions to the terminal; check recording on the signed .app (`src-tauri/target/debug/bundle/macos/Talaan.app`).
