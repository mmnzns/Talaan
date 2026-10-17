# What Miguel tests once v1 is complete

Collected while building (M3 onward). Everything here was built and checked as far as possible without a real
call or without using the Mac while Miguel was on it; each item says what is still unverified. Use the signed
app (`scripts/build-signed-debug.sh` → `src-tauri/target/debug/bundle/macos/Talaan.app`).

## Recording (M3)
- [ ] Switch AirPods in / out during a call: recording continues, popover says which mic.
- [ ] Close the lid for a minute mid-call: a pause gap appears, recording resumes.

## Transcript and summary (M4, M5)
- [ ] A real meeting end to end: Stop → transcribing → summarizing → ready, title replaced. Read the summary: is anything wrong or missing?
- [ ] macOS asks to allow notifications the first time a summary finishes; "Summary ready" opens the meeting.
- [ ] Settings › Speech models: download / cancel / delete a model; switch to Parakeet and back.
- [ ] Settings › Summary AI: Local AI (already downloaded), Larger model download, Ollama / LM Studio if installed.
- [ ] Your own key (Claude, OpenAI, Gemini, Groq, Cerebras, Mistral or OpenRouter): paste, Test (it should name the model it picked), pick a model from the menu or type one with "Other model ID…", record a call, check the summary says that provider and model.

## Call detection and menu bar (M6)
- [ ] A real Zoom call (Zoom is on Always): recording starts by itself with the banner; leave the call → "Call ended" → stops after 30 s.
- [ ] A Slack huddle (Ask): "Record this call?" → Record; Not now; Keep recording on Call ended.
- [ ] A Google Meet in Chrome: the nudge says "browser".
- [ ] Dictating with Parlato / VoiceInk never triggers anything.
- [ ] Menu bar popover while recording: pause, mark, note, stop; the timer next to the icon.
- [ ] Close the main window: Talaan keeps running in the menu bar; the Dock icon brings it back.

## Meeting features (M7)
- [ ] Export › Markdown and Plain text through the save dialog; open the files.
- [ ] Export › PDF: the print sheet opens; Save as PDF looks right.
- [ ] Drag an audio file onto the window: it imports.
- [ ] ⋯ › Delete audio now, Show in Finder, Write the summary again.

## Named speakers and Ask (M8)
- [ ] Settings › Speech models › Tell speakers apart: Download (401 MB). Then a call with 3+ other people: the transcript says Speaker 1, 2, 3. Are the turns right?
- [ ] Rename Speaker 2 by clicking the chip: every line updates, and action items owned by "Speaker 2" now show the name.
- [ ] Turn Tell speakers apart off: the next meeting's other side is plain "Them".
- [ ] Settings › Speech models › Search by meaning: Download (640 MB).
- [ ] Ask (toolbar) across all meetings, with Wi-Fi off: answers cite the right meeting and moment; clicking a citation jumps there and plays.
- [ ] Ask from a meeting (This meeting), and with a folder / tag scope.
- [ ] Ask something none of your meetings cover: it says it couldn't find it instead of making something up.

## Organize and connect (M9)
- [ ] A folder's ⋯ menu: Rename, Delete (its meetings stay, outside any folder).
- [ ] Settings › Call apps › Tag meetings by call app: the next detected call gets #zoom (or the app's name).
- [ ] Settings › Connections › Calendar: turning it on asks for calendar access once. Record during a calendar event (or up to 10 min before it): the meeting takes the event's title, the speaker chips suggest attendees, and action-item owners use their names.
- [ ] Deny calendar access: the switch stays off and "Allow in system settings" opens the right pane.
- [ ] Notion: Settings › Connections › Notion. Make an internal integration (the link opens Notion), share a database and a page with it, paste the secret, Connect. Pick a default destination.
- [ ] Export › Notion into the database (with and without transcript), then into the page: "Open in Notion" shows the summary, to-dos with owners, and times. A long meeting's transcript arrives whole.
- [ ] A wrong secret says Notion didn't accept it; a page not shared with Talaan says how to add the connection.
- [ ] Settings › Connections › Browser call detection on. Set Call apps › Browser calls to Always. Join a Google Meet in Chrome: macOS asks once to let Talaan control Chrome (Allow); recording starts as "Google Meet" with the banner, tagged #google meet if tagging is on.
- [ ] Same with Zoom in the browser (app.zoom.us/wc/…) and Teams web. Try Safari and Arc if you use them.
- [ ] A YouTube tab or a voice-recorder site using the mic, with detection on: nothing happens.
- [ ] Say "Don't Allow" for a browser: its chip says not allowed with a link to system settings, and calls there are asked about (Ask) instead.

## Look (Sinag)
- [ ] Every screen in light and dark: main window, meeting, Ask, Settings (all tabs), menu bar idle and recording, nudge, Always banner, call ended, Export, onboarding. Anything that looks off against Design/UI v3?
- [ ] The app icon in the Dock and Finder, and the "ta" in the menu bar (light and dark menu bar).

## Release prep (M10)
- [ ] Settings › General: Microphone lists your real devices; pick one, record, unplug it: the default is used.
- [ ] Open at login (in the installed app): log out and in, the "ta" is in the menu bar.
- [ ] About: Buy me a coffee and Source on GitHub open in the browser.
- [ ] Onboarding on a fresh start (Settings › Privacy › Delete everything): the microphone and system audio prompts appear at that step; choosing a speech model or Local AI downloads nothing until you click its Download button; skipping both shows the "No speech model yet" note at the end.
- [ ] The release build: `scripts/release-local.sh` made a signed .dmg (src-tauri/target/aarch64-apple-darwin/release/bundle/dmg). Install it from the .dmg, Open Anyway once.
- [ ] Update path: install a build, then publish a newer version; "update ready" appears, Update now installs and relaunches with permissions and meetings intact.


## Windows release gate (W5)

Use an installer from `scripts/release-windows.ps1` (or the draft release). On a PC that has run `tauri dev`,
quit the dev app first: one Talaan at a time.

**Install**
- [ ] Clean PC (Windows Sandbox): install, Talaan starts, Parakeet transcribes, the local summary works (VC++
  runtime and DirectML.dll are in the installer).
- [ ] Download from the draft release in Edge: the browser warning, then SmartScreen "Windows protected your
  PC" › More info › Run anyway. No administrator prompt. Installed to %LOCALAPPDATA%\Talaan.
- [ ] Start menu has Talaan; its shortcut's AppUserModelID is com.craftconceptsdigital.talaan (command in W5
  plan, Task 4).

**First run**
- [ ] Onboarding end to end: mic Allow, system audio (no prompt), Parakeet download with progress, Local AI
  download, call app rules, extras, Open Talaan.
- [ ] Mic switch off in Privacy & security: Talaan says so and the button opens the right page.

**Calls**
- [ ] A real Microsoft Teams call (app) on Always: recording starts with the banner; leave → Call ended → stops.
- [ ] A real Zoom call: same.
- [ ] Google Meet in Edge or Chrome, tab in front, Browser calls on Always: starts by itself. Join with the Meet
  tab in front, then switch to another tab before the call is noticed (or join from a background tab): it still
  starts by itself (the tab is titled with the meeting code, "Meet - abc-defg-hij"; a meeting tab titled by its name instead still asks). Same with Teams web.
- [ ] Both sides in the transcript, Me/Them right, no echo lines (headphones). Summary written offline.
- [ ] Calendar: paste the Google "Secret address in iCal format", Test and connect shows events; a recording
  during an event takes its title and attendees.
- [ ] Offline start: turn Wi-Fi off, start a recording during an event, turn Wi-Fi on, stop with Transcript only on:
  within a minute or two (or 5 minutes later) the meeting takes the event's title.

**Shell**
- [ ] Tray icon visible on light and dark taskbars; popover opens above the tray; hover shows the timer.
- [ ] "Summary ready" toast appears and opens the meeting; a click from Action Center after the banner is
  gone also opens it (it's a `talaan://meeting/<id>` link the installer registers). The first click may show
  Windows' "Select an app to open this talaan link" picker: choose Talaan and "Always". Note whether it appeared.
  Then quit Talaan from the tray, click another "Summary ready" from Action Center: Talaan starts on that meeting.
- [ ] After uninstalling, `HKCU\Software\Classes\talaan` is gone.
- [ ] With FxSound (or Voicemeeter, Sonar) as the default output: the recording window shows "Your sound goes through
  FxSound…" once (OK hides it for good), and Check setup has a "Sound enhancer" row.
- [ ] Sit in a Zoom or Teams lobby (or a muted webinar) for over a minute while recording: no "can't hear the call"
  notice. A normal call with people talking: no notice either.
- [ ] Second launch focuses the first window. Ctrl+, opens Settings. Title bar follows dark mode.
- [ ] Open at login: sign out and in, the tray icon is back.
- [ ] Sleep mid-recording for 30 s: a pause gap, recording continues.

**Update from a previous build** (local, before any tag; the real key isn't needed)
- [ ] Make a throwaway key: `npm run tauri signer generate -- --ci -p test -f -w "$env:TEMP\talaan-test.key"`,
  copy the public key from `$env:TEMP\talaan-test.key.pub`.
- [ ] Old build: temporarily set the version to 0.9.9 in a config file and point the updater at this PC:
  `{"version":"0.9.9","plugins":{"updater":{"pubkey":"<pub>","endpoints":["http://127.0.0.1:8765/latest.json"],"dangerousInsecureTransportProtocol":true}}}`
  → `npm run tauri build -- --config <that file>`; install it, record one meeting.
- [ ] New build: same file without "version", plus `"bundle":{"createUpdaterArtifacts":true}`, with
  TAURI_SIGNING_PRIVATE_KEY=$env:TEMP\talaan-test.key and _PASSWORD=test. Copy the setup .exe and .sig to a
  folder, run `node scripts/latest-json.mjs v1.0.0 --dir <folder>`, change each url to
  `http://127.0.0.1:8765/<file>`, save as `<folder>\latest.json`, serve it: `npx http-server <folder> -p 8765`.
- [ ] In the old build: Settings › General › Updates › Check now, then Update now. The installer runs by itself,
  Talaan reopens as the new version; the meeting, settings and Open at login are still there.
- [ ] Delete the throwaway key files from %TEMP% afterwards.

**Delete and uninstall**
- [ ] Settings › Privacy › Delete everything: Talaan quits; next launch is onboarding; `%APPDATA%\Talaan` holds
  only an empty `meetings`; `cmdkey /list` has no Talaan entries.
- [ ] Same while recording: Talaan quits, and there is no half meeting afterwards.
- [ ] Mac: Delete everything quits too (instead of restarting); start it again from the Dock: onboarding.
- [ ] Uninstall without "Delete the application data": %APPDATA%\Talaan stays.
- [ ] Uninstall with it ticked: %APPDATA%\Talaan, the com.craftconceptsdigital.talaan folders, every
  `*.Talaan` credential and the Run / StartupApproved entries are gone; no Talaan process left.

**Before publishing**
- [ ] Version is 1.0.0 in tauri.conf.json, Cargo.toml and package.json (not -dev).
- [ ] The Mac upload (`scripts/upload-mac-release.sh <tag>`) ran after the CI **updater-json** job had finished,
  not while it was running, so the two latest.json uploads didn't race.
- [ ] The draft has the .exe, .exe.sig, .dmg, Talaan_aarch64.app.tar.gz, its .sig and latest.json with four
  keys (windows-x86_64, windows-x86_64-nsis, darwin-aarch64, darwin-aarch64-app). If one is missing, run
  `node scripts/latest-json.mjs <tag>` again.
- [ ] If the first tag shows that `gh release upload/download <tag>` can't find the draft (never tried on a test
  repo), look the draft up with `gh api repos/mmnzns/Talaan/releases --jq '.[] | select(.tag_name=="vX") | .id'`
  and upload through `gh api` instead (scripts/latest-json.mjs, scripts/upload-mac-release.sh, release.yml).

## GPU on Windows (G1)

Builds: Actions › **Release** › Run workflow on `gpu-windows` makes two artifacts, `talaan-windows-installer`
(the setup .exe with the GPU helpers) and `talaan-gpu-helpers` (`talaan-llm-vulkan.exe`, `talaan-stt-vulkan.exe`
and `scripts/gpu-helper-smoke.ps1`). Unzip the helpers into a new temp folder. Use temp copies of data
(`TALAAN_DATA_DIR`) for anything that writes; the helpers only read model files. The baseline to beat (1.0.2, CPU,
one-hour meeting): summary 10.5 min, Whisper Large Turbo 3.3x real time, Parakeet 159 s.

**The helpers on their own** (from the temp folder; nothing is installed)
- [ ] `powershell -File gpu-helper-smoke.ps1 -Dir .`: both say `"backend":"vulkan"` and the RTX 5070 Ti; the
  devices list has the AMD Radeon iGPU too, and `chosen` is the RTX's index, not the iGPU's.
- [ ] `... -LlmModel "$env:APPDATA\Talaan\models\qwen3.5-4b\Qwen3.5-4B-Q4_K_M.gguf"`: a JSON reply, in a few
  seconds; Task Manager's GPU graph for the RTX moves, the Radeon's doesn't.
- [ ] With a Whisper model downloaded (Speech models, any Whisper) and a meeting's `them.wav` copied to the temp
  folder: `... -WhisperModel <ggml-*.bin> -Wav them.wav` returns pieces of text.
- [ ] `$env:GGML_VK_VISIBLE_DEVICES='99'` then the plain smoke run: the helpers fail to start or say `cpu`
  (Vulkan can't start); remove the variable afterwards.

**Installed build** (quit the installed 1.0.2 first; the installer replaces it, data is kept)
- [ ] Settings › General › Check setup › Graphics chip: "NVIDIA GeForce RTX 5070 Ti (16 GB), for transcription and
  summaries".
- [ ] Re-transcribe the one-hour meeting with Whisper Large Turbo, then Generate summary: note both times against
  the baseline. Task Manager shows the RTX busy. Compare the transcript with the CPU one (same words, near
  enough).
- [ ] Parakeet: unchanged (CPU), still about 159 s.
- [ ] Settings › Speech models › Use the GPU off: Check setup says "Off, so Talaan uses the processor"; a summary
  and a Whisper transcript run on the processor (RTX idle). Turn it back on.
- [ ] Forced crash: quit Talaan, start it from PowerShell with `$env:TALAAN_GPU='fail'; & "$env:LOCALAPPDATA\Talaan\talaan.exe"`.
  Generate a summary and transcribe with Whisper: both finish (on the processor), and `%APPDATA%\Talaan\gpu.json`
  lists `llm` and `stt`. Check setup says the graphics chip failed, with Try again. Quit, start Talaan normally,
  click Try again: gpu.json is gone and the row names the RTX again.
- [ ] Overlays: with Steam (overlay on) running, the summary still runs on the GPU (implicit layers are off for
  the helpers).
- [ ] Update and uninstall while a helper runs (start a summary, then run the installer again, then uninstall):
  no "file in use" error; `talaan-llm-vulkan.exe` and `talaan-stt-vulkan.exe` are gone after uninstall.
- [ ] Windows Sandbox, if enabled (no Vulkan driver): Talaan starts, Check setup says no Vulkan driver, a summary
  and a transcript work on the processor.
