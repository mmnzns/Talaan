# Windows port — handoff

Written on the Mac when v1 was feature-complete (2026-10-14), for the session that ports Talaan on the PC.
**Decision:** v1.0 ships Mac and Windows together; nothing is released until the Windows build passes the same
release gate. Read `CLAUDE.md` and the Notion pages ("Talaan Journey", "Talaan v1 Build Plan") first.

## Progress

**W0 done.** The app compiles on Windows and the tests are green; CI has a Windows job; the summarizer helper builds with
`scripts/build-llm-helper.ps1`. Parakeet (ONNX Runtime) transcribes at about 26x real time on the CPU. Whisper on the CPU
is currently very slow (391 s for a 62 s clip), under investigation.

**W1 done.** System audio is WASAPI process loopback that excludes Talaan. It reconnects after audio errors and shows a
"Call audio stopped" notice if it can't. The speakers warning sees through virtual outputs (FxSound). A device watcher follows
default-device changes, recording auto-pauses on sleep, and microphone access reads the Privacy & security switches.

**W2 done.** Call detection reads the microphone consent store in the registry (`LastUsedTimeStop` = 0 while an app holds
the mic; unreadable values never count as in use). Windows ids are in `calls/apps.rs` and `calls/tabs.rs`: Zoom, Teams, Slack,
WhatsApp, Discord, Webex, browsers; ignored: Talaan, Parlato, Wispr Flow, voice typing, Sound Recorder, Xbox Game Bar and mic
processors (NVIDIA Broadcast, SteelSeries Sonar, Voicemod, Krisp). Settings > Connections lists installed Windows browsers.
**Not done:** a real call with Zoom/Teams (your checkpoint). **Known:** a dev build or console exe (the test exe, python.exe)
gets no consent-store entry even while it records, so `mic_users_live_smoke` can't show a live entry; GUI apps do appear.
So the live source is now the active WASAPI capture sessions on every mic (pid → exe name, or package family), merged with
the consent store; a registry entry only counts while its app is running (a killed app can leave stop stuck at 0). FxSound
and audiodg are ignored too. Discord sitting in a voice channel holds the mic, so it counts as a call; Discord stays on Ask by
default (`callApps` in `src/data/defaultSettings.ts`).

**Known:** relay apps (FxSound, Voicemeeter, Sonar) re-render audio under their own process, so Talaan's own sounds can reach
"Them" and other audio may be captured twice.

**W3 done.** One Talaan at a time (`tauri-plugin-single-instance`: a second launch focuses the first window). The tray uses
the coloured `32x32.png` icon on Windows and shows the timer in the tooltip ("Talaan · Recording 12:04"); the popover and call
prompts open above the taskbar (`place_near_icon` in `commands/tray.rs`). Windows keeps its native title bar (the 84 px inset
is macOS only), Ctrl+, opens Settings, and the native window theme follows the app theme (`core:window:allow-set-theme`).
"Summary ready" is a Windows toast (`tauri-winrt-notification`): installed builds use the bundle id
`com.craftconceptsdigital.talaan` as app id, dev builds borrow PowerShell's. **W5 must confirm** the NSIS shortcut carries
that AppUserModelID, or toasts won't show for installed builds.

**Check by hand before release (W3):**

- the tray icon is visible on a light and a dark taskbar;
- the popover opens above the tray;
- a second launch focuses the first window;
- the toast appears and opens the meeting (installed build);
- the title bar follows dark mode;
- Ctrl+, opens Settings;
- clicking the tray icon again closes the popover;
- a toast clicked from Action Center (after the banner is gone) opens the meeting (W5: if that click fails, the shortcut
  needs a ToastActivatorCLSID);
- Dark, then System, with Windows set to Light switches the app to light.

**Dev note:** the single-instance guard is Windows only. On Windows, `npm run tauri dev` exits immediately if the installed
Talaan is running; quit it first.

**Smoke tests** (all `#[ignore]`; run from `src-tauri/`, most need a working mic):

| Test | Command |
|---|---|
| `system_smoke` (5 s of system audio plus mic; play something while it runs) | `cargo test system_smoke -- --ignored --nocapture` |
| `system_excludes_own_audio` (our tone must not reach "Them"; set `TALAAN_TONE_DEVICE` to a real output's name if the default is a relay app) | `TALAAN_TONE_DEVICE=<output name> cargo test system_excludes_own_audio -- --ignored --nocapture` |
| `system_silence_keeps_packets_flowing` (30 s of silence still delivers 30 s of samples) | `cargo test system_silence_keeps_packets_flowing -- --ignored --nocapture` |
| `mic_clock_smoke` (mic stamps are on the host clock) | `cargo test mic_clock_smoke -- --ignored --nocapture` |
| `tracks_line_up_smoke` (mic and loopback stamp-vs-now lag under 200 ms; a PowerShell process plays a Windows sound) | `cargo test tracks_line_up_smoke -- --ignored --nocapture` |
| `mic_users_live_smoke` (prints this exe's consent-store entry while recording and after) | `cargo test mic_users_live_smoke -- --ignored --nocapture` |
| `capture_sessions_live_smoke` (our pid is an active capture session while recording, is kept out of `mic_users()`, and is gone 1.5 s after stopping) | `cargo test capture_sessions_live_smoke -- --ignored --nocapture` |
| `toast_smoke` (posts one toast under the PowerShell app id, waits 5 s, prints what `show()` returned) | `cargo test toast_smoke -- --ignored --nocapture` |
| `pipeline_smoke` (real models on a recorded meeting) | `TALAAN_SMOKE_MEETING=<meeting dir> cargo test --release pipeline_smoke -- --ignored --nocapture` |

## What's already shared

Everything outside `src-tauri/src/platform/macos/` is cross-platform by design: storage, the transcriber
pipeline (VAD, Parakeet/Whisper, echo, merge, diarization), the summarizer and its helper protocol, Ask, call
rules and tab classification, calendar event picking, Notion, the whole React UI. Models download at runtime
(`~/AppData/Roaming/Talaan` via `dirs::data_dir()`), so nothing big lives in the repo. Fonts are bundled.
`keyring`, the updater and autostart plugins all support Windows.

## What has to be written (`src-tauri/src/platform/windows/`, plus one `cfg` line in `platform/mod.rs`)

Mirror each macOS item with the same names, so callers don't change:

| macOS (`platform/macos/`) | Job | Windows approach |
|---|---|---|
| `system_tap.rs` `SystemTap` | Capture everything the computer plays (Them), excluding Talaan | WASAPI loopback on the default render device (`AUDCLNT_STREAMFLAGS_LOOPBACK`); process-loopback exclusion (`ActivateAudioInterfaceAsync` with `PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE`, Windows 10 2004+) to leave Talaan's own sounds out. Implements `recorder::source::AudioSource`. |
| `clock.rs` `host_now_ns`, `host_ticks_to_ns` | One clock for both tracks | QueryPerformanceCounter, matching WASAPI's QPC timestamps. |
| `mic_users.rs` `mic_users()` | Which processes are using the mic (call detection) | The `CapabilityAccessManager\ConsentStore\microphone` registry keys (`LastUsedTimeStop == 0` means in use, per app/exe), or IAudioSessionManager2 capture sessions with their process ids. Return `MicUser { pid, bundle_id, path }`; on Windows put the exe name/AUMID in `bundle_id` and add Windows ids to `calls/apps.rs` (`Zoom.exe`, `ms-teams.exe`, `slack.exe`, browsers…). |
| `permissions.rs` | Mic permission + deep links | Mic privacy setting (same ConsentStore key); `ms-settings:privacy-microphone`. System audio needs no permission on Windows. |
| `devices.rs` | Built-in speakers warning, device change watcher | IMMNotificationClient for default-device changes; form factor `Speakers` + `EnumeratorName` for built-in. |
| `power.rs` `SleepWatcher` | Auto-pause on sleep | `WM_POWERBROADCAST` / `RegisterSuspendResumeNotification`. |
| `notify.rs` | "Summary ready" notification, click opens the meeting | Windows toast (tauri-plugin-notification or WinRT `ToastNotificationManager` with an activation argument). |
| `calendar.rs` | Calendar events (title, attendees) | WinRT `AppointmentStore` (needs the appointments capability) or leave Calendar hidden on Windows for v1; the pure picker in `calendar/` is ready either way. |
| `browser_tabs.rs` | Tab URLs of a browser using the mic | No AppleScript: UI Automation on the browser's address bar (active tab only), or leave browser calls on Ask/Never on Windows. `calls/tabs.rs` classification is shared. |
| `thread.rs` | Background thread priority | `SetThreadPriority(THREAD_PRIORITY_BELOW_NORMAL)` / `THREAD_MODE_BACKGROUND_BEGIN`. |

Files outside `platform/` that call `platform::macos::…` directly and need a `cfg` or a platform-neutral
re-export: `lib.rs`, `calendar/mod.rs`, `commands/{ask,recorder,calendar,calls,summarizer}.rs`.

## Build and packaging

- `src-tauri/Cargo.toml`: `keyring` (use `windows-native`) and `whisper-rs` are currently macOS-only target
  deps; add Windows entries (whisper-rs with `vulkan` for GPU, or CPU). `llm/Cargo.toml`: llama-cpp-2 with
  `vulkan` (or CPU) instead of `metal`. ONNX Runtime (Parakeet, Nemotron diarization) works as is; DirectML is
  optional.
- `scripts/build-llm-helper.sh` names the helper by target triple; Windows needs a `.ps1` (or Git Bash) version
  producing `binaries/talaan-llm-x86_64-pc-windows-msvc.exe`.
- `tauri.windows.conf.json`: NSIS installer, icon from `icons/icon.ico` (already generated).
- `.github/workflows/ci.yml`: add a `windows-latest` job (`cargo check`, tests) early in the port.
- `.github/workflows/release.yml`: add a Windows job (NSIS + updater artifacts) that merges into the same
  `latest.json` (platform `windows-x86_64`). The updater secrets are already on the repo. No paid Windows
  certificate (decision log): users click More info › Run anyway once; the README explains it.

## UI copy

One design, platform-neutral copy (decision log). The only per-OS words are system names users must recognize:
"menu bar" (Mac) vs "system tray" (Windows). These are still hard-coded as "menu bar" in about a dozen places
(`grep -rn "menu bar" src`); move them to one string table switched by platform. The window's title bar uses
`titleBarStyle: Overlay` on macOS (traffic lights inset in the toolbar, `pl-[84px]` in `Toolbar.tsx`);
Windows needs its own title bar treatment.

## Not in the repo (and where it is)

- **Updater signing key:** `~/.tauri/talaan.key` on the Mac, password in its login Keychain ("Talaan updater
  key password"), and both as GitHub secrets. CI signs Windows updates with the secrets, so the PC doesn't need
  the key. Back it up (password manager): if it's lost, installed copies can never be updated again.
- **Mac code-signing certificate:** "Parlato Code Signing", in the Mac's login keychain (not needed on Windows).
- **Design files:** Google Drive › Talaan - Meeting Minutes › `Design/UI v3/` (synced to the PC through Drive).
- **Decisions and Build Log:** Notion "Talaan Journey".

## Done when

The release gate on Notion passes on Windows too: install, onboard, record a real Zoom/Teams/Meet call with
both sides, transcribe and summarize offline, call rules, export, Ask, update from a previous build.
