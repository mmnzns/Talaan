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
- [ ] Your own key (Claude or OpenAI): paste, Test (it should name the model it picked), record a call, check the summary says that provider.

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

