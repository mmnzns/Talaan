# M3. Recorder (Mac) — task plan

> Generated from Notion "Talaan v1 Build Plan" › M3. Model: **Opus** (plan and build: native audio).
> Ends at a 🛑 checkpoint: Miguel records a real 10-minute Zoom call.
> Done when: a real Zoom call records both sides; force-quit mid-call then relaunch offers Recover; switching AirPods mid-call keeps recording; a pause shows as a gap.

## Verified facts (2026-10-14)

- This Mac: macOS 27.0.1, arm64. Core Audio process taps (`AudioHardwareCreateProcessTap` + `CATapDescription`) exist since macOS 14.2; the "System Audio Recording Only" permission (`NSAudioCaptureUsageDescription`) since 14.4.
- Crates exist and are current: `cpal` 0.18, `rtrb` 0.4, `rubato` 5, `hound` 3.5, `objc2-core-audio` / `objc2-av-foundation` / `objc2-app-kit` 0.3, `block2` 0.6. Verify each license when adding it (all expected MIT/Apache-2.0).
- Signing: the "Parlato Code Signing" self-signed certificate is in the login keychain (decision log: Talaan keeps Parlato's free certificate). Dev builds signed with it keep TCC permissions across rebuilds.

## Architecture decisions (Opus, binding)

### Minimum macOS 14.4, no ScreenCaptureKit fallback

System audio comes only from a Core Audio **global process tap** (all processes except Talaan). The Notion plan suggests ScreenCaptureKit as a fallback; dropping it avoids the scary "Screen Recording" permission and a second capture path, at the cost of macOS 13–14.3 support. `minimumSystemVersion: "14.4"`. Recorded as a decision in the Build Log.

### Two tracks, one clock

```
mic (cpal, input device)  ──► rtrb ring ──┐
                                          ├─► writer thread: downmix → resample to 16 kHz → TimelineWriter ─► me.wav
system tap (CoreAudio IOProc on a        │                                                               them.wav
 private aggregate device) ──► rtrb ring ─┘
```

- Audio callbacks do no allocation and no locks: they copy frames plus the callback's host timestamp into an `rtrb` SPSC ring. One writer thread per track drains it.
- **`TimelineWriter`** (pure, unit-tested) owns alignment. It turns *(host time, samples at 16 kHz)* into *sample positions on the recording clock*: position = (host time − session start − total paused time) × 16000. A gap fills with zeros, overlap is trimmed. Both tracks use the same session start, so they line up whatever the device latency, device switches or dropped buffers. Recording clock excludes paused time, matching the transcript timestamps decided in M1 (pause shown as "Paused 2 min" at its position).
- Resampling with `rubato` (FFT-based, fixed ratio per device rate), mono by averaging channels. A device switch rebuilds the resampler for the new rate.
- WAV via `hound`, 16 kHz mono 16-bit PCM. `flush()` every 2 s, which rewrites the header, so a crash loses at most ~2 s and the file stays playable.

### Cross-platform split

- `src-tauri/src/recorder/` (cross-platform): session state machine, `TimelineWriter`, resampling, WAV writing, levels (RMS), mic capture via `cpal`, the `SystemAudioSource` trait.
- `src-tauri/src/platform/macos/`: `system_tap.rs` (process tap + private aggregate device + IOProc), `permissions.rs` (mic status/request via AVFoundation, system-audio status, System Settings deep links), `devices.rs` (default device change listeners, built-in speakers detection), `power.rs` (sleep/wake notifications).
- The Windows port later adds `platform/windows/` with WASAPI loopback behind the same `SystemAudioSource` trait. Nothing in `recorder/` may import `objc2`.

### Session state machine

`Idle → Recording ⇄ Paused → Stopping → Idle`, plus `discard`. Owned by a `Recorder` in Tauri state (`Mutex`). On start: `storage.create_meeting` (folder + `recording` status, M2), open both writers, start sources. Every 5 s and on every change: write `durationMs`, `marks`, `pauses` to `meeting.json` (crash keeps them).

- **Pause:** stop feeding writers (sources keep running so resume is instant), remember pause start; on resume, add a `PauseGap { at: recording-clock position, durationMs }`. Nothing is written during a pause.
- **Mark / note:** `Mark { at: current recording-clock ms, note? }`. A note without an active mark attaches to "now".
- **Sleep:** `power.rs` sends will-sleep → auto-pause, did-wake → auto-resume (shows as a pause gap). A lid close never produces an hour of silence.
- **Device change:** default input changes → rebuild the cpal stream on the new device; default output changes → rebuild the aggregate device for the new main device. Emit `recorder-notice` ("Switched to MacBook Pro Microphone. Recording continues."). The timeline absorbs the gap.
- **Stop:** stop sources, drain rings, finalize WAVs, write duration, status → `ready` (M4 changes this to `transcribing`). **Discard:** stop and delete the meeting folder.
- **Crash recovery (extends M2):** `recover_meeting` becomes real for the audio part: repair WAV headers from file length (`repair_wav`, tested), set `durationMs` from the longer track, status → `ready`. M4 later queues transcription instead.

### Permissions

- Microphone: `AVCaptureDevice authorizationStatusForMediaType:` / `requestAccessForMediaType:`. `NSMicrophoneUsageDescription` in Info.plist, `com.apple.security.device.audio-input` entitlement.
- System audio: `NSAudioCaptureUsageDescription` in Info.plist. macOS asks on first tap creation. There is no public "status" call; **verify** what a denied tap does on this Mac (expected: tap is created but delivers silence). Detection: if the them-track has been digital silence for 10 s while the default output device is running, emit `recorder-notice` with a "System audio may be blocked" message and a button that opens System Settings › Privacy & Security › Screen & System Audio Recording.
- Deep links (verify on this Mac): `x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone` and `…?Privacy_AudioCapture`.
- Dev note: `npm run tauri dev` runs an unsigned binary whose permissions are attributed to the terminal. The permission and real-call checks run on a **signed debug .app**: `APPLE_SIGNING_IDENTITY="Parlato Code Signing" npm run tauri build -- --debug --bundles app`, with hardened runtime and `Entitlements.plist`.

### Speaker warning

`devices.rs`: default output's transport type is built-in and its data source is internal speakers (not the headphone jack) → `onBuiltInSpeakers = true`, re-evaluated on output change. Bluetooth/USB/headphone jack → false.

### Frontend

- Commands: `recorder_start { app?, appLabel? }`, `recorder_pause`, `recorder_resume`, `recorder_mark { note? }`, `recorder_stop`, `recorder_discard`, `recorder_status`, `permissions_status`, `permissions_request_mic`, `permissions_open_settings { pane }`.
- Events: `recorder-state` (state, meetingId, elapsedMs, paused, marks, notes count, mic name, onBuiltInSpeakers), `recorder-levels` (~10 Hz: me, them RMS 0..1), `recorder-notice` (text, kind).
- `src/data/recorder.ts`: `RecorderClient` interface with a Tauri implementation and a mock (the M1 popover behavior) for the browser.
- Until M6 builds the real menu bar: the toolbar **Record** button starts a real recording and shows the existing `RecordingPopover` as a floating panel in the main window, driven by real state and levels. Manual start uses app `other`, label "Call". Stop and save opens the new meeting.
- `RecordingPopover` takes its data from props/the client instead of its internal mock timers (the Screens stage keeps a mock client).

## Tasks

TDD for every pure piece (timeline, resampler wrapper, WAV repair, session state machine, levels). Native pieces get the manual checks listed. Commit after each task (`M3:`), all suites green.

1. **Crates + config.** Add `cpal`, `rtrb`, `rubato`, `hound`, and macOS-only (`[target.'cfg(target_os = "macos")'.dependencies]`) `objc2`, `objc2-foundation`, `objc2-core-audio`, `objc2-core-audio-types`, `objc2-av-foundation`, `objc2-app-kit`, `block2`. `tauri.macos.conf.json` (minimum 14.4, entitlements), `Info.plist` (mic + audio capture strings), `Entitlements.plist`. THIRD_PARTY_NOTICES.
2. **`TimelineWriter`** (pure): positions from host time, gap fill, overlap trim, pause offsets, two-track alignment test with different latencies.
3. **Resample + downmix** wrapper over rubato with tests (48 k stereo → 16 k mono length and a 1 kHz tone survives).
4. **WAV writer + `repair_wav`**: periodic flush; repair a header truncated by a simulated crash; tests.
5. **Levels**: RMS per 100 ms window, tests.
6. **Session state machine** (pure core, sources injected): start/pause/resume/mark/note/stop/discard, pause gaps and mark positions in recording time, meeting.json updates, tests with fake sources and a fake clock.
7. **Mic source** via cpal (default input, rebuild on device change). Manual check: record 10 s from the mic, play the WAV.
8. **System tap** in `platform/macos/system_tap.rs`: global tap excluding Talaan's own process, private aggregate device, IOProc into the ring, teardown. Manual check: play a YouTube clip, record 10 s, WAV contains it; verify and note what a denied permission does.
9. **Permissions + devices + power** in `platform/macos/`: mic status/request, deep links, built-in speakers detection, device change listeners, sleep/wake. Manual checks for each.
10. **Recorder wiring**: Tauri state, commands, events, 5 s meeting.json saves, real `recover_meeting` (repair + duration), startup interaction with M2's interrupted scan.
11. **Frontend**: `RecorderClient` (Tauri + mock), toolbar Record → floating RecordingPopover with real timer, levels, pause, mark, note, notices, stop/discard; open the meeting after stop. Permission denied states with the deep-link button.
12. **Signed debug .app** build script `scripts/build-signed-debug.sh`, README dev note.
13. **Checkpoint prep**: run every done check on the signed app: 10-min call both sides, force-quit → relaunch → Recover, AirPods switch, pause gap, lid close, speakers warning. Build Log entry with findings (tap permission behavior, deep links, latency, file sizes per hour). Then **stop for Miguel's real Zoom call** 🛑.

## Out of scope for M3

Transcription (M4), call detection, nudge/banner/real menu bar (M6), echo dedupe (M4), import audio (M7).

## Outcome (2026-10-14)

Tasks 1–12 done; task 13 checks run on the signed debug app. 120 Rust tests (2 ignored hardware smoke tests), 71 TS tests.

Verified on this Mac (macOS 27.0.1, signed with the Parlato certificate):

- **Permissions:** the first Record shows the mic prompt with Talaan's usage text; the system audio prompt follows on the first tap. Talaan is then listed under *System Audio Recording Only* (not Screen Recording). Both deep links (`Privacy_Microphone`, `Privacy_AudioCapture`) open the right pane.
- **Both sides:** `say` through the speakers lands on `them.wav`, the mic on `me.wav`; the mic lags the system track by about 20 ms. The tap records before the volume control (output volume 19% still gave near full-scale samples).
- **Crash:** `kill -9` mid-recording → relaunch shows "Recording interrupted · Recover" → Recover repairs both WAVs (1:44.8, under 1 s lost), keeps the mark and the 4.3 s pause gap, status ready.
- **Discard** confirms, then deletes the folder.
- **Size:** 16 kHz mono 16-bit = 32 KB/s ≈ 115 MB per hour per track (230 MB per hour of call before retention).

Fixed during checks:

- The "system audio may be blocked" warning fired after real sound because apps keep output streams running while quiet. A blocked tap delivers zeros from the first sample, so once any sound came through, the warning is off for that recording.
- The level meter froze on its last frame when the level stayed constant (silence looked busy). It now scrolls on every levels event.
- The speakers warning missed eqMac-style routers (eqMac reports itself as USB). It now also checks whether the built-in speakers device is running underneath.
- "Copy for Slack" became **Copy as text** with no Slack markup, so it pastes cleanly anywhere.

Left for Miguel at the checkpoint: a real 10-minute Zoom call, AirPods switch mid-call, lid close mid-call.
