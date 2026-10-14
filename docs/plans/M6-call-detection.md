# M6. Call detection and menu bar — task plan

> Generated from Notion "Talaan v1 Build Plan" › M6. Model: **Opus** (Miguel: stay on Opus; no 🛑 stops, he tests the whole app at the end).
> Done when: tests pass for the rules engine; Zoom on Always auto-starts with the banner; Slack on Ask shows the nudge; ending a call stops after the prompt; dictating with Parlato never triggers Talaan.

## Verified facts (2026-10-14)

- **Who is using the mic:** Core Audio process objects (macOS 14.2+) expose `kAudioProcessPropertyIsRunningInput`, `…PID` and `…BundleID` per process. Checked on this Mac with a probe that opened the mic: exactly that process showed `in:1`, nothing else. Public API, no extra permission. Command-line tools have no bundle id (fall back to the executable path).
- **Calls run in helper processes**, so apps are matched by bundle-id prefix. Seen running on this Mac: `us.zoom.xos`, `us.zoom.caphost` (Zoom's capture host), `com.microsoft.teams2.helper`, `com.microsoft.teams2.modulehost`, `com.tinyspeck.slackmacgap(.helper)`, `com.apple.avconferenced` (FaceTime audio), `com.google.Chrome(.helper)`, `com.apple.WebKit.GPU` (Safari media), `com.hnc.Discord(.helper)`, `net.whatsapp.WhatsApp`, `ai.perplexity.comet(.helper)`, `com.brave.Browser`, `com.prakashjoshipax.VoiceInk`, `video.fathom.electron` (another meeting recorder).

## Architecture decisions (Opus, binding)

### Detection

- `platform/macos/mic_users.rs`: a 1 Hz poll on a background thread (≈60 processes × 2 property reads: negligible, and simpler and sturdier than per-process listeners whose objects come and go) returning `MicUser { pid, bundle_id, path }` for every process except Talaan with input running.
- `src-tauri/src/calls/` (cross-platform, pure, TDD):
  - `apps.rs`: bundle-id prefix tables. Call apps: Zoom (`us.zoom.`), Teams (`com.microsoft.teams`), Slack (`com.tinyspeck.slackmacgap`), FaceTime (`com.apple.avconferenced`, `com.apple.FaceTime`), Webex (`com.cisco.webex`, `com.webex.`), Discord, WhatsApp; browsers (`com.google.Chrome`, `com.apple.WebKit.GPU`, `com.apple.Safari`, `company.thebrowser.Browser` (Arc), `com.microsoft.edgemac`, `com.brave.Browser`, `org.mozilla.firefox`, `ai.perplexity.comet`) → the "Browser calls" rule. Ignored apps by prefix (Parlato `com.craftconceptsdigital.parlato`, VoiceInk, Superwhisper `com.superduper.superwhisper`, Wispr Flow `com.electron.wispr-flow`, MacWhisper `com.goodsnooze.MacWhisper`, Apple Dictation `com.apple.CoreSpeech`/`com.apple.DictationIM`/`com.apple.SpeechRecognitionCore`, Voice Memos `com.apple.VoiceMemos`, Fathom, Talaan). Unknown apps never trigger; ignored apps are filtered first so a dictation app can never look like a call.
  - `rules.rs`: the state machine. Input: ticks with the current set of call apps holding the mic, the recorder state, and the settings (per-app rule, detect on/off). Output actions: `Nudge(app)`, `CloseNudge`, `AutoStart(app)`, `CallEnded(app)`, `AutoStop`, `ClosePrompt`. Rules:
    - An app must hold the mic for 2 s before anything happens (blips like a mic test don't nudge).
    - Ask → nudge; Always → start recording and show the banner; Never → nothing. Browser calls can't be Always until tab detection exists (M9), so Always behaves as Ask for browsers.
    - Nothing while already recording (manual or not); a dismissed nudge stays dismissed until that app releases the mic.
    - While recording a detected call: the app releasing the mic for 3 s → "Call ended" prompt with a 30 s countdown → stop unless Keep recording. Keep = no more prompts until the app takes the mic again and releases it again. If the app takes the mic back during the countdown (rejoin), the prompt closes.
    - Manual recordings (Record button) never get auto-stopped.
- `commands/calls.rs`: runs the poll + state machine, turns actions into windows and recorder calls; recording started from detection uses the app's id and label (titles become "Call, Zoom, Oct 14, 2:30 PM").

### Menu bar and floating windows

- Tauri `tray-icon`: Talaan's mark in the menu bar; title shows the timer while recording (`● 12:04`). Click toggles the **menu bar popover**: a frameless, transparent, always-on-top window positioned under the icon, routed to `#/tray`. Idle it shows the M1 idle popover (Record, recent meetings); recording, the M1 recording popover with real state (moved here from the main-window floating panel).
- **Prompt window** (one, reused): frameless, always-on-top, on every Space, top-right under the menu bar, routed to `#/prompt?kind=nudge|banner|ended&app=…`. Holds the M1 `Nudge`, `AlwaysBanner`, `CallEndedPrompt` components with real callbacks.
- Main window close hides it (Talaan keeps running in the menu bar to detect calls); clicking the Dock icon or "Open Talaan" brings it back; Quit is in the popover. The toolbar Record button starts recording and opens the menu bar popover.
- Frontend `RecorderClient` events already reach every window; the popover and prompts use the same client.

## Tasks

1. **Mic users** (`platform/macos/mic_users.rs`) + smoke test with the mic probe.
2. **App tables** (`calls/apps.rs`) with tests (helpers, browsers, ignored first, unknown ignored).
3. **Rules state machine** (`calls/rules.rs`) TDD: debounce, Ask/Always/Never, browsers, dismissed, recording in progress, call ended + countdown + keep + rejoin, manual recordings untouched, detection off.
4. **Wiring** (`commands/calls.rs`): poll thread, actions → recorder and windows, events to the frontend, settings changes applied live.
5. **Tray + popover window:** tray icon, timer title, popover window positioning and toggle, Quit, main window hide-on-close and reopen.
6. **Prompt window:** nudge / banner / call ended with real actions (Record, Not now, Stop, Copy notice, Change rule → Settings › Call apps, Keep recording).
7. **Frontend:** `#/tray` and `#/prompt` routes, popover with real recorder + recent meetings, toolbar Record opens the popover, Call apps tab adds Discord/WhatsApp, ignored list shows real entries.
8. **Checks:** simulate Zoom/Slack with the probe (fake bundle via a test hook), verify each rule end to end headlessly; Parlato/VoiceInk never trigger; build, CI, docs, Build Log.

## Out of scope

Browser tab detection and Always for browser calls (M9), launch at login (M10), user-added call apps by picking an app (M7/M9 if time).

## Outcome (2026-10-14)

All tasks done. 217 Rust tests, 75 TS tests. Checked in the signed app with a debug-only hook (`dev-mic-users.txt` in the data folder adds pretend mic users by bundle id):

- **Slack (Ask):** nudge after ~2 s, gone when Slack let go of the mic.
- **Zoom (Always, via `us.zoom.caphost`):** recording started by itself as "Call, Zoom, Oct 14, 4:42 PM", banner "Recording Zoom automatically" with the live timer, timer in the menu bar. Zoom let go → "Call ended" after 3 s → stopped by itself ~30 s later → transcribing.
- **Discord:** detected as a call app (added to the defaults with WhatsApp).
- **Parlato and VoiceInk on the mic:** nothing at all.
- **Menu bar popover:** opens under the icon, idle menu with Start recording, recent meetings, Open Talaan / Settings / Quit; closes when clicking elsewhere.

Fixes along the way: the prompt window's fit-to-content ran before the card existed (now re-runs when the content changes) and is done in Rust (`window_fit`); prompts sit under Talaan's menu bar icon instead of the top-right corner, where macOS notifications cover them; call apps added in later versions are merged into stored settings without touching the user's rules.

Not verified with real calls (needs Miguel): a real Zoom/Slack/Meet call, the menu bar popover while recording, Keep recording.
