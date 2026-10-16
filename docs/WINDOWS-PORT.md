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
`com.craftconceptsdigital.talaan` as app id, dev builds borrow PowerShell's. The NSIS shortcut carries it (W5).

**Check by hand before release (W3):**

- the tray icon is visible on a light and a dark taskbar;
- the popover opens above the tray;
- a second launch focuses the first window;
- the toast appears and opens the meeting (installed build);
- the title bar follows dark mode;
- Ctrl+, opens Settings;
- clicking the tray icon again closes the popover;
- a toast clicked from Action Center (after the banner is gone) opens the meeting (v1.1 W1, below);
- Dark, then System, with Windows set to Light switches the app to light.

**Toast clicks from Action Center (v1.1, W1).** A toast's foreground activation reaches only a running process while
its banner is up; from Action Center an unpackaged app needs a COM activator (a `ToastActivatorCLSID` on the shortcut
plus a registered COM server). Talaan uses protocol activation instead: the installer (`installer-hooks.nsh`,
`NSIS_HOOK_POSTINSTALL`) registers `talaan://` under `HKCU\Software\Classes\talaan` pointing at `$INSTDIR\talaan.exe
"%1"` (removed on uninstall, kept on update), and `notify.rs` posts `<toast activationType="protocol"
launch="talaan://meeting/<id>">` whenever that key opens the running exe (else the old in-process toast, as in dev
builds). A click makes Windows start `talaan.exe "talaan://meeting/<id>"`; the single-instance plugin hands the
argument to the running Talaan (`deeplink::meeting_in_args` → `open_meeting`), or, when Talaan wasn't running, the new
instance keeps it until the window asks (`meetings_launch_link`). Only `[A-Za-z0-9_-]` ids are accepted (any web
page can open `talaan://`). Checked on the PC (2026-10-16, a test handler under `HKCU\Software\Classes\talaan`
that logs its argument, removed afterwards): the protocol toast reaches Action Center; clicking it there (real mouse
click) removes it and Windows launches the link, under PowerShell's app id and under Talaan's; `ShellExecute` and
`Launcher.LaunchUriAsync` of the link run the handler directly. **But** the toast click, like `explorer.exe
talaan://…`, went through Windows' app picker (`OpenWith.exe -Embedding`) instead of the handler: on this Windows 11
Insider build (26300) a shell-launched custom link seems to need a user choice first (the PC's other link handlers,
claude:// and codex://, have a protected `UrlAssociations` entry; adding `Application\AppUserModelID` or
`RegisteredApplications` capabilities didn't change it). The picker window didn't paint while automated, so its
choice wasn't made (that would write a protected per-user default). Hand check with the installed build:
docs/TESTING.md. If the picker shows every time, the fallback is a COM activator.

**Dev note:** the single-instance guard is Windows only. On Windows, `npm run tauri dev` exits immediately if the installed
Talaan is running; quit it first.

**W4 done.** *Calendar:* Windows' own calendar store doesn't see new Outlook or Google (spike below), so Talaan reads
the calendar's private iCal address instead, which works with any provider. The user pastes it in Settings › Connections ›
Calendar (Windows only; "Test and connect" downloads it and says how many events are in the next 7 days). The address is a
secret link, so it's kept in Windows Credential Manager (key `calendar-ics`, through `keys.rs`; removed by Disconnect and
by Delete all Talaan data), never in settings.json; settings only hold `calendarConnected`. Privacy › What leaves this
computer lists the download. `calendar/fetch.rs`
downloads it (15 s in all, 20 MB cap, `webcal://` accepted; checked when a recording starts, at most every 15 minutes; a failed download keeps the old
copy, and the recording tries once more a minute later). `calendar/ics.rs` reads it once per download and expands recurrences per window: RRULE, EXDATE,
RECURRENCE-ID overrides, IANA and Windows time zone names (Outlook's "Eastern Standard Time"), floating and all-day times,
at most 1,000 occurrences per event. Reading is tolerant: a bad event is skipped and the rest are kept. The user's own
address (from a Google address) and declined attendees are left out. The public Google holidays feed reads as 433 events.
A 21 MB synthetic feed reads in about 0.15 s in a release build. *Browser tabs:* UI Automation reads the address bar
(`OmniboxViewViews`) of each Chromium browser frame (`Chrome_WidgetWin_1`), within 300 ms. It searches the browser's own
views breadth-first (at most 12 levels and 600 elements, one cached round trip per element) and never enters web content, so
a heavy page doesn't slow it down. Every UI Automation call is capped at 250 ms (`CUIAutomation8` with `IUIAutomation2`
connection and transaction timeouts), and the budget is checked before each step. Live check (2026-10-15, a new Edge
window each, closed afterwards): Meet → `Urls` (call seen) in 39 ms, example.com → `Unknown` in 45 ms, a long Wikipedia
article → `Unknown` in 22 ms. A call tab seen means the call (Always works), but no call tab seen means "can't tell" (Ask),
never "not a call". Settings shows installed browsers as "ready" (Windows has no permission for this).
Firefox stays Ask, as on the Mac.

**Background call tabs (v1.1, C1).** The same walk also reads the tab strip: every tab is a `TabItem` whose name is its
title, plus Chromium's own notes (Edge: "Example Domain - Memory usage - 22.4 MB"). The walk never enters a tab or the
address bar, and stops after the level where it has both (they sit at the same depth, about 8 levels down; everything
deeper is bookmarks, side panels and menus). `calls/tabs.rs` `title_call` knows the call pages' own titles: "Meet -
abc-defg-hij" / "Meet – <name>", "… | Microsoft Teams", "Zoom Meeting" / "Zoom Webinar", each optionally followed by
" - <note>". A matching title counts only because the browser is asked only while it's using the mic. The address bar
still wins (`Urls`); else matching titles give `Tabs::Titles`; else `Unknown`. Live check (2026-10-16, debug build):
`background_call_tab_live_smoke` starts a separate Edge with a new temp profile (so nothing opens in the user's
windows), opens a local page titled "Meet - abc-defg-hij", then example.com in front of it: `Titles` 5 times out of 5,
52-90 ms (first read 90 ms); `tab_urls` over every Edge window 74 ms. `tab_urls_live_smoke` (address bar plus tab
titles): Meet 69 ms, example.com 60 ms, Wikipedia 34 ms (before the tab strip: 39 / 45 / 22 ms). A full walk without the
early stop took 91-269 ms, too close to the 300 ms budget. **Limits:** English call-page titles only (Meet and Teams
keep "Meet" and "Microsoft Teams" in other languages; Zoom's web client may translate "Zoom Meeting"); a Meet tab left
open after a call, while the browser uses the mic for something else, reads as a call (the same is true of its
address).

**Known limits of the iCal reader** (rare in Google and Outlook feeds; fix if one shows up):

- `RECURRENCE-ID;RANGE=THISANDFUTURE` changes only that one occurrence, not the ones after it.
- `RDATE;VALUE=PERIOD` values are ignored; so is `EXRULE` (deprecated).
- A series is stepped through from its first occurrence, at most 100,000 steps: a series that repeats every few minutes
  or hours and started long ago may show nothing in the current window. Daily or weekly meetings are far below the cap
  (30 years of a daily meeting is about 11,000 steps).
- A DTSTART that doesn't fit its own RRULE isn't counted as an occurrence (RFC 5545 says it should be).
- Time zone ids written as display names ("(UTC-05:00) Eastern Time (US & Canada)") fall back to local time.

**Check by hand (W4):**

- In Google Calendar › Settings › your calendar › "Secret address in iCal format", copy the address, paste it in Talaan's
  Settings › Connections › Calendar and click Test and connect: it shows "Found N events in the next 7 days".
- Start a recording during a calendar event: the meeting takes the event's title and attendees.
- With Google Meet on Always, join a Meet call in Edge or Chrome (Meet tab in front): recording starts by itself.
- With YouTube in front and the mic in use (e.g. a voice-search or recorder site), it asks instead (Browser rule).
- The "Where do I find this?" links (Google, Outlook, iCloud) open the right help pages; the Outlook and iCloud links
  were written from memory and haven't been opened yet.

**W5 done.** *Installer:* Tauri's NSIS template, per user (`%LOCALAPPDATA%\Talaan`, no administrator prompt),
publisher Craft Concepts Digital. WebView2 comes from the embedded bootstrapper (silent) on the rare PC without it. The
VC++ runtime (`bundleVCRuntime`) and ONNX Runtime's `DirectML.dll` sit next to `talaan.exe`, so a PC without the VC++
Redistributable still starts Talaan; both are release-only (`scripts/release-windows.ps1`), so `tauri dev` is unchanged.
*Uninstall:* `src-tauri/windows/installer-hooks.nsh` closes a running `talaan-llm.exe` before install and uninstall, and
always removes the `StartupApproved\Run\Talaan` value. With "Delete the application data" ticked it also removes
`%APPDATA%\Talaan` (meetings, audio, models, settings) and every Credential Manager entry in `keys::ACCOUNTS` (a test
fails if the two lists differ); Tauri itself removes the `com.craftconceptsdigital.talaan` folders and the Run value.
Updates never delete anything. *Toasts:* the Start Menu shortcut's AppUserModelID is the bundle id, the same as
`APP_ID` in `notify.rs` (a test ties `APP_ID` to `tauri.conf.json`). *Release flow:* `scripts/release-windows.ps1` builds
the installer (and its updater `.sig` when the key is set) and fails unless whisper.cpp and llama.cpp were built with
-O2; the `windows` job in `release.yml` runs it on a tag and uploads to the draft; `scripts/latest-json.mjs` rebuilds
`latest.json` from every `.sig` on the release, so Windows (CI) and the Mac (`scripts/upload-mac-release.sh` after
`release-local.sh`) can upload in any order. The `macos` CI job only runs when the repo variable `MAC_RELEASE_ON_CI` is
`true`. *Delete everything* now quits (as the Privacy tab says) and drops a live recording first, on both platforms.
The release gate is in docs/TESTING.md ("Windows release gate (W5)").
Checked on the PC (2026-10-15, `Talaan_1.0.0-dev_x64-setup.exe`, 21.1 MB): installs per user without elevation;
`talaan.exe` starts and loads the VC++ DLLs and `DirectML.dll` from its install folder; `dumpbin /dependents` finds
every non-system import of the installed exes and DLLs next to them; the shortcut's AppUserModelID is the bundle id;
uninstall without the box keeps `%APPDATA%\Talaan` and fake `*.Talaan` keys but removes the Run and StartupApproved
values; with the box ticked `%APPDATA%\Talaan`, `%LOCALAPPDATA%\com.craftconceptsdigital.talaan`, the fake
`notion.Talaan` and `calendar-ics.Talaan` keys and the publisher registry key are gone too. **Not checked:** a clean PC
(Windows Sandbox isn't enabled on this PC), toasts and Delete everything in the installed app (need the UI).

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
| `protocol_toast_smoke` (posts one protocol toast, `talaan://meeting/<TALAAN_TOAST_MEETING or smoke>`, under the PowerShell app id) | `cargo test protocol_toast_smoke -- --ignored --nocapture` |
| `public_holiday_feed_smoke` (downloads Google's public Canadian holidays calendar and reads it; network) | `cargo test public_holiday_feed_smoke -- --ignored --nocapture` |
| `tab_urls_live_smoke` (opens a new Edge window on a Meet URL, then example.com, then a long Wikipedia article; expects `Urls`, then `Unknown` twice, with timings; closes only the windows it opened) | `cargo test tab_urls_live_smoke -- --ignored --nocapture --test-threads=1` |
| `background_call_tab_live_smoke` (a separate Edge with a temp profile: a page titled "Meet - abc-defg-hij" behind example.com; expects `Titles` under 300 ms; closes that Edge and deletes the profile) | `cargo test background_call_tab_live_smoke -- --ignored --nocapture --test-threads=1` |
| `large_feed_timing` (reads a 21 MB synthetic calendar) | `cargo test --release large_feed_timing -- --ignored --nocapture` |
| `pipeline_smoke` (real models on a recorded meeting) | `TALAAN_SMOKE_MEETING=<meeting dir> cargo test --release pipeline_smoke -- --ignored --nocapture` |

**Speed (release builds).** whisper.cpp and llama.cpp were compiled at `/Od` in release on Windows: with the Visual Studio generator the `cmake` crate overwrites `CMAKE_C_FLAGS_RELEASE`/`CMAKE_CXX_FLAGS_RELEASE` and drops every `/O` flag (debug builds were fine). A 62 s clip took 398 s; with the fix it takes about 29 s, same transcript. Fix: `.cargo/config.toml` sets both variables to `-O2 -DNDEBUG`, which the -sys crates forward as CMake defines. Also tuned: CPU Whisper uses three quarters of the logical cores (2..=12; 16 threads on a 16-thread Ryzen is 2-3x slower than 12) and flash attention is on only for Metal. Cargo does not rebuild the -sys crates when `[env]` changes, so after editing `.cargo/config.toml` run `cargo clean --manifest-path src-tauri/Cargo.toml --release -p whisper-rs-sys -p llama-cpp-sys-2`. The same flags apply on macOS (-O2 instead of CMake's -O3 for CPU code; negligible, Metal runs the models).

**CPU baseline (release builds, v1.0.3).** whisper.cpp defaults to `GGML_NATIVE=ON`, so whisper-rs-sys compiled ggml for the CPU of the machine building it: the PC's cache showed `GGML_NATIVE:BOOL=ON` with AVX-512 detected. A release built on a runner with AVX-512 would crash with "illegal instruction" on most Intel PCs the first time Whisper runs. `scripts/cpu-baseline.ps1` (dot-sourced by `release-windows.ps1` and `build-llm-helper.ps1`) sets `GGML_NATIVE=OFF`, AVX/AVX2/FMA/F16C/BMI2/SSE4.2 on and every AVX-512 option off (x86-64-v3); whisper-rs-sys and llama-cpp-sys-2 forward `GGML_*` env vars as CMake defines (llama-cpp-sys-2 already forced NATIVE off). The release script cleans the two -sys crates when their cache doesn't match, and fails if a cache has NATIVE or any AVX-512 option ON, or AVX2 not ON. It is Windows only (not `.cargo/config.toml`, which can't be scoped to a target): the Mac keeps its native Apple-silicon build. Local `cargo test` / `tauri dev` on the PC still build natively unless the script's env is set.

**GPU (Vulkan) with automatic CPU fallback (G1, branch `gpu-windows`; plan: docs/plans/G1-windows-gpu.md).**
`talaan.exe` stays free of Vulkan (ggml registers its Vulkan backend even with the GPU off, and a missing
`vulkan-1.dll` would stop it from starting). GPU work runs in two helpers next to it: `talaan-llm-vulkan.exe` (the
summary helper built with `--features vulkan`) and `talaan-stt-vulkan.exe` (new crate `src-tauri/stt`, whose lib is
also the in-process Whisper code, so both transcribe a chunk the same way). `gpu.rs` uses a helper only when Use the
GPU is on, `vulkan-1.dll` loads from System32, the helper is installed, and `gpu.json` (data folder) holds no failure
for this app version and display driver (adapter names and versions from the registry). Helpers start with
`VK_LOADER_LAYERS_DISABLE=~implicit~` (overlays). They pick the discrete GPU with the most memory, never an iGPU,
never a split (llama.cpp: one device, split mode none, `fit_params` for partial offload with a 1 GB margin; Whisper:
`gpu_device`), and say `cpu` when there is none. Fallback: a GPU start failure, crash, 5-minute stall (summary) or
per-chunk timeout (Whisper), or an error is redone on the processor (`LocalAi::with_gpu`, `transcriber/fallback.rs`);
the failure is written to `gpu.json` only when the processor then succeeds. Check setup › Graphics chip shows the GPU
or why not (Try again clears `gpu.json`). Embeddings and Parakeet stay on the processor. `TALAAN_GPU=fail` makes the
GPU builds abort on their first job, to test the fallback. Build: `build-llm-helper.ps1` builds the GPU helpers when
`VULKAN_SDK` is set (required with `TALAAN_GPU=1`) into their own target folder (`TALAAN_GPU_TARGET_DIR`, absolute;
CI uses `C:\tg` for path length), with the x86-64-v3 baseline; `release-windows.ps1` bundles them
(`bundle.resources`), checks `GGML_VULKAN` on/off per cache and fails if `talaan.exe` or `talaan-llm.exe` imports
`vulkan-1.dll` (dumpbin). CI: `.github/actions/vulkan-sdk` (LunarG 1.4.363.0, cached); Release › Run workflow uploads
the installer and the bare helpers; CI › Run workflow adds `windows-gpu`. **Not verified locally:** the Vulkan builds
themselves (no Vulkan SDK on the PC by decision); CI builds them. Hand checks: docs/TESTING.md, "GPU on Windows (G1)".

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
| `calendar.rs` | Calendar events (title, attendees) | The calendar's private iCal address (W4): download, then `calendar/ics.rs` parses and expands recurrences; the pure picker in `calendar/` is shared. WinRT `AppointmentStore` was ruled out by the W4 spike (below). |
| `browser_tabs.rs` | Tab URLs of a browser using the mic | UI Automation on each Chromium window's address bar (W4) and tab titles (v1.1); no call seen = Unsure. `calls/tabs.rs` classification is shared. |
| `thread.rs` | Background thread priority | `SetThreadPriority(THREAD_PRIORITY_BELOW_NORMAL)` / `THREAD_MODE_BACKGROUND_BEGIN`. |

**W4 spike, AppointmentStore (2026-10-15, Miguel's PC, new Outlook installed, unpackaged debug test):**
`AppointmentManager::RequestStoreAsync(AllCalendarsReadOnly)` succeeded without the `appointments` capability,
but the store holds only the retired Mail and Calendar app's local calendar and nothing from new Outlook or
Google. The test printed:

```
calendars: 1
  calendar: Ok("Calendar")
appointments in the next 7 days: 0
```

Decision: go with the iCal address. The spike test was not kept, since it needed three extra `windows`
features (`ApplicationModel_Appointments`, `Foundation`, `Foundation_Collections`) used nowhere else.

Files outside `platform/` that call `platform::macos::…` directly and need a `cfg` or a platform-neutral
re-export: `lib.rs`, `calendar/mod.rs`, `commands/{ask,recorder,calendar,calls,summarizer}.rs`.

## Build and packaging

- `src-tauri/Cargo.toml`: `keyring` (use `windows-native`) and `whisper-rs` are currently macOS-only target
  deps; add Windows entries (whisper-rs with `vulkan` for GPU, or CPU). `llm/Cargo.toml`: llama-cpp-2 with
  `vulkan` (or CPU) instead of `metal`. ONNX Runtime (Parakeet, Nemotron diarization) works as is; DirectML is
  optional.
- `scripts/build-llm-helper.sh` names the helper by target triple; Windows needs a `.ps1` (or Git Bash) version
  producing `binaries/talaan-llm-x86_64-pc-windows-msvc.exe`.
- Done (W5): `tauri.windows.conf.json`: NSIS installer, icon from `icons/icon.ico` (already generated).
- `.github/workflows/ci.yml`: add a `windows-latest` job (`cargo check`, tests) early in the port.
- Done (W5): `.github/workflows/release.yml`: add a Windows job (NSIS + updater artifacts) that merges into the same
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
