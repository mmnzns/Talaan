# Changelog

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
