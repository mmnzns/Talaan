# Talaan

A bot-free meeting recorder for macOS. Talaan records your mic and the call's audio on your Mac, so no bot joins the meeting and it works with any app: Zoom, Google Meet, Teams, Slack huddles, FaceTime. When the call ends you get a transcript labeled Me / Them, a summary, and action items. Everything runs on your Mac by default.

*Talaan* is Tagalog for "record" or "log".

**Status:** in development (`1.0.0-dev`). No release yet. Windows comes after v1.0.

## Develop

Requires Rust, Node 22 and the Xcode Command Line Tools.

```bash
npm install
npm run tauri dev
```

Tests: `npm test` and `cargo test --manifest-path src-tauri/Cargo.toml`.

Recording needs macOS 14.4 or later. To test the microphone and system audio permissions the way users get them, build a signed debug app with `scripts/build-signed-debug.sh` and open `src-tauri/target/debug/bundle/macos/Talaan.app` (under `npm run tauri dev` macOS attributes the permissions to your terminal instead).

## Click through M1 (UI with sample data)

In a browser everything runs on sample data. In the Tauri app (M2 onward) meetings are real files on disk; open **Screens → Dev data → Load sample meetings** (debug builds) to click through on real storage. Nothing records or transcribes yet; that arrives milestone by milestone.

```bash
npm run tauri dev      # the real window, real storage in ~/Library/Application Support/Talaan (a second Settings window opens from the gear or Cmd+,)
npm run dev            # same UI in a browser at http://localhost:1420, sample data (handy for screenshots)
```

| Where | What to try |
|---|---|
| Main window | Search, folders, tags, day groups. Click **Q4 roadmap sync**: click any time on a decision or action item to jump to that moment in the transcript. **Edit**, change a line, **Revert to original**. Rename a speaker with the pencil chip. Title, tags and folder are editable in place. |
| Meeting states | **Vendor demo** (summarizing), **Imported: client kickoff** (transcript only, try *Generate summary*), **Call, Zoom, 8:50 AM** (interrupted: *Recover*), **Call, Teams, Oct 8** (failed: *Try again*). |
| Copy / Export | **Copy for Slack**, **Export** sheet (Markdown, PDF, text, Notion). Saving is simulated until M7. |
| Ask | Toolbar **Ask**, or *Ask about this meeting* in the `…` menu. Try "What did we decide about pricing?" and click a citation. |
| Settings | Gear icon or `Cmd+,`. Six tabs. Appearance override lives under General. |
| Onboarding | **Screens → Onboarding → Restart onboarding**, or *Privacy → Delete everything*. |
| Menu bar, recording, prompts | `#/screens`: menu bar popover, the recording popover (pause, mark, quick note, levels), the Ask nudge, the Always banner and the call-ended prompt. The Record button opens the recording popover. |
| Empty state | Add `?mock=empty` to the browser URL. |

Screenshots of every screen in light and dark are in [`docs/screens/M1/`](docs/screens/M1). They were captured at 900×600 in a browser.

## License

MIT. Third-party libraries and models are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
