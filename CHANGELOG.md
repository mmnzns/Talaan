# Changelog

## 1.2.2 (2026-11-13)

**Your recordings are safer**
- **Warning before the disk fills up:** while recording, Talaan checks the free space and tells you when only about 30 minutes of recording are left. If saving fails anyway, you're told at once, what was saved is kept and transcribed, and the meeting says where the audio stopped.
- **Imported recordings are kept:** cleanup now counts from the day you import a file, not the day it was recorded, so an old voice note isn't deleted right after you bring it in.
- **Failed meetings keep their audio:** a meeting that couldn't be transcribed yet keeps its audio past the cleanup date, so it can still be transcribed later.
- **A confirmation before cleanup deletes anything:** choosing a shorter time in Settings › Privacy now says how many meetings or recordings would be deleted, and asks first.
- **Talaan always opens:** one damaged meeting folder can no longer stop Talaan from starting.
- **Stop always stops:** if the recording can't be saved when you press Stop, Talaan still ends it in every window and keeps the meeting ready to recover. A meeting can't be deleted while it's being recorded.

**Clearer**
- **A failed automatic start says so:** if a call app set to Always can't be recorded, Talaan tells you once instead of retrying quietly.
- **Ask suggests questions that fit your meetings,** and questions about one meeting when you're asking about one meeting.
- **Plainer wording:** "audio file" instead of an Apple app name, the Privacy list no longer contradicts itself about transcripts, Delete everything says shared models other apps use are kept, and browser call detection says what it changes.

**Windows**
- **The window shows after an update** when Talaan was started at login.

## 1.2.1 (2026-11-10)

**Fixes**
- **Stop means stop for call apps set to Always:** pressing Stop during a call no longer starts a new recording a second later. Talaan records that app automatically again on its next call.
- **A graphics driver update gives the card another chance (Windows):** if the graphics card failed before, Talaan tries it again after the driver is updated.
- **No Dock icon when Talaan opens at login (Mac):** it stays in the menu bar until you open the window.
- **Stricter security for the app window:** Talaan's windows can now load only Talaan's own files, so nothing from the internet can run inside the app.

## 1.2.0 (2026-11-02)

**Models and speed**
- **Models are shared:** they now live in a folder shared with Craft Concepts Digital's other apps, so a model is downloaded once. The first start after updating moves the models you already have in by itself, without downloading anything again. Delete means Talaan stops using a model; the file is erased only when no other app uses it, and Talaan tells you when it stays. The "Reuse models already on this computer" setting is gone, because the shared folder does it.
- **Ask before deleting a model:** Delete now opens a confirmation that names the model, its size and what will happen, with Cancel selected.
- **Every local AI model:** Qwen 3.5 4B and 9B and Granite 4.2 3B, each with its size, the memory it needs and its licence.
- **More speech models:** full-size versions and English-only versions of Whisper and Parakeet.
- **Parakeet can use the graphics card on Windows.**
- **Graphics card on by default where it helps:** on the Mac, and on Windows PCs that have a separate graphics card. On Windows a PC with only built-in graphics now starts with the card off, including on a brand-new install; a PC with a separate card keeps what you had (a card you had switched off stays off after updating from 1.1). This is decided once, and what you set after is kept.
- **"Graphics card failed" only when it did:** the message no longer shows when a model simply has no graphics card path, and Try again now gives the card a new chance in the running app. If the card fails, Status says which part (speech or AI) it failed for and that the processor did the work.

**Summaries**
- **Change the language of any summary:** Talaan writes it again from the transcript in the language you pick (about 30 languages, with search), from the summary or the meeting's ⋯ menu. Settings › Summary AI › Summary language has the same list.

**Around the meeting**
- **Export what you want:** every way out of a meeting (copy as text, Markdown, PDF, text file, Notion) now lets you pick the summary, the transcript or both. "Copy as text" moved into Export, and the menu bar's button on recent meetings is now Export.
- **Import several audio files at once:** drop or pick as many as you like; Talaan imports them one after another.
- **A date on every meeting:** each row in the list shows its own date, so a second meeting on the same day no longer looks like it has none.

**Easier to use**
- **Keyboard focus stays inside dialogs:** Tab no longer wanders to the window behind, Escape closes only the top dialog, and focus returns to where you were.
- **Open at login stays out of the way:** Talaan now starts in the menu bar or system tray with the window closed, instead of opening the window. Call detection is ready as soon as you log in.

## 1.1.1 (2026-10-19)

- **Know when there's a new version:** Talaan checks at launch and every 6 hours, sends one notification per new version, and shows "Update now" in the main window and the tray. It never interrupts a recording: no update installs while you record, and no recording starts while an update installs.
- **Easier to see what's selected:** the chosen speech model, summary engine, setup choice, Settings tab and meeting now fill with gold.
- **Fixed:** time estimates were too short right after a restart with meetings still waiting.
- **Fixed:** Ollama at 0.0.0.0 counted as another computer in the privacy line.
- Small wording fixes in Spanish; "MIT license" now sits under the version in Settings.

## 1.1.0 (2026-10-17)

**Easier to start**
- **Setup in one click:** "Download what Talaan needs" with a time estimate, or choose each model yourself. Nothing downloads until you click. Downloads keep going after setup.
- **A 1-minute test recording** on first run, so you see Talaan work before a real call.
- **Zoom asks before recording** on new installs, like every other call app.
- **Plainer settings:** a Status tab that checks everything a call needs, speech models grouped as Best for most people / Fastest / For older computers, and summary AI as "On this computer" or "Online AI with your own account".

**Better summaries**
- **Your notes shape the summary.** Notes typed during or after the call are kept and built on; each marked moment becomes a clickable highlight.
- **Meeting types:** summarize as General, 1:1, Stand-up, Interview, Sales or client call, or Lecture or webinar, plus your own instructions.
- **Draft a follow-up email** from the summary, to copy or open in your mail app.
- **Time left, not just a percentage,** for transcripts and summaries, learned from your computer's speed.

**Around the meeting**
- **Next meeting in the menu bar or tray,** with its own Start recording. Zoom, Google Meet, Teams and FaceTime invite links name the call app.
- **Suggested speaker names** from the invite. Talaan never renames anyone by itself.
- **Also save each meeting to a folder** as a Markdown file (an Obsidian vault, Documents, a synced folder). Renamed copies replace the old ones, which go to the Trash.
- **Email this summary** to the people who were invited.

**When something goes wrong**
- **Copy diagnostics** for bug reports: version, setup checks and a log that never contains what was said.
- **Fixed:** Talaan could freeze after a recording stopped, with "Meeting not found" and Quit doing nothing.
- **Fixed:** a meeting's title didn't update on screen when the summary named it.
- **Fixed:** error messages no longer blame memory without evidence.
- **More stable:** a bug in one background task no longer closes the app mid-recording.

**Windows**
- **Uses your graphics card** for speech and summaries, falling back to the processor on its own.
- **No more black console window** during summaries.
- **Calls in a background browser tab** are recognized. Clicking a notification opens the meeting.
- **Models move out of the roaming profile.** Sound enhancers like FxSound are noticed.
- **Built for any x64 processor** from the last decade (no more risk of a crash on some Intel PCs).

## 1.0.2 (2026-10-15)

- **More speech models, all on your computer:** Whisper Large v3, Large v2, Medium, Base and Tiny; Parakeet Ultra, Parakeet v2 (English), Parakeet Unified (English) and Nemotron 3.5 (35 languages), alongside Whisper Large Turbo, Parakeet v3 and Whisper Small. Setup shows the three recommended ones with "Show all models".
- **Setup downloads nothing on its own.** Choosing a speech model or the local AI only selects it; each has its own Download button, and you can skip both and download later in Settings.
- **Fixed:** your own key is remembered per provider. Switching provider no longer looks like the key was lost, and the menu marks providers that have a saved key.
- **Fixed:** Privacy › Saved API keys › Remove keys now deletes every summary provider's key from secure key storage. Before, it only hid them in Settings.

## 1.0.1 (2026-10-15)

- **Your own key:** Groq, Cerebras, Mistral and OpenRouter join Claude, OpenAI and Gemini. The model is now a menu of the supported models per provider (Recommended picks the first one your key can use), with "Other model ID…" for anything newer. Requests are tuned per model: short or hidden reasoning, and no temperature for the models that refuse one.
- **Fixed:** the dot on switches slid out of the pill when turned on.

## 1.0.0 (2026-10-15)

The first release.

- **Recording:** your mic and the call's audio as two tracks, on any call app, with no bot. Pause, mark a moment, quick notes. Survives crashes (Recover on next launch), device switches and sleep.
- **Call detection:** asks when a call app uses the mic; per-app Ask, Always (with a banner and a notice to paste) or Never; stops by itself after the call ends. Dictation apps never trigger it. Google Meet, Zoom and Teams in the browser are recognized when browser detection is on.
- **Transcripts:** Whisper Large Turbo (default), Parakeet or Whisper Small, on your computer. Echo from speakers removed. Optional speaker separation into Speaker 1, 2, 3; rename once and every line and action item follows.
- **Summaries:** one format for every engine (recap, decisions, action items with owner and due date, open questions) from the built-in local AI, Ollama or LM Studio, or your own key (Claude, OpenAI, Gemini, OpenAI-compatible). Long calls summarized in parts. A notification when it's ready.
- **Ask:** questions about one meeting, a folder, a tag or all of them, with clickable citations. Search stays on your computer.
- **Meetings:** click any time to jump and play; edit with revert; copy as text; export Markdown, PDF, text or to Notion; import audio files; folders, tags, search; calendar titles and attendees.
- **Privacy:** offline by default; audio kept 7 days unless pinned; delete everything in one click. Delete everything quits Talaan when done.
- **Look:** "Sinag", Talaan's own design: warm off-white and ink, sun gold, Syne / Instrument Sans / DM Mono, the Baybayin "ta". Light and dark.
- **Windows:** Windows 10 (2004 or later) and 11 on x64: a per-user installer, tray icon, toasts, calendar through its iCal address, browser call tabs. One updater for both platforms.
