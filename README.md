<p align="center"><img src="src-tauri/icons/128x128@2x.png" width="96" alt="Talaan"></p>

# Talaan

**Meeting notes that stay on your computer.** Talaan records both sides of any call (your mic and the call's audio), then writes the transcript and a summary with decisions and action items. No bot joins your meeting, and it works with any app: Zoom, Google Meet, Teams, Slack huddles, FaceTime, Discord, WhatsApp.

*Talaan* is Tagalog for "record". The logo is "ta" in Baybayin, the native Filipino script.

**Status:** v1.0 in final testing. macOS 14.4 or later on Apple silicon. Windows comes after v1.0.

## What it does

- **Asks before it records.** When a call app starts using your mic, Talaan offers to record. Set any app to Always (with a banner and a one-click notice to paste in the chat) or Never. Dictation apps never trigger it.
- **Transcript, Me and Them.** Your mic is "Me", the call is "Them"; optionally split into Speaker 1, 2, 3 and rename them once.
- **Summary in one format.** Recap, decisions with times, action items with owner and due date, open questions. Click any time to jump there and play it.
- **Ask your meetings.** Questions across all your meetings, answered with links to the exact moment.
- **Edit, copy, export.** Fix a word, copy as text, export Markdown, PDF, plain text or to Notion. Import a Voice Memo.
- **Organize.** Folders, tags, calendar titles and attendees.

## Privacy

Everything runs on your computer by default: speech recognition, speaker separation, search and the summary AI, with Wi-Fi off too. Nothing leaves it unless you choose:

| What | When |
|---|---|
| Transcript text | Only if you set Summary AI to your own key (Claude, OpenAI, Gemini, any OpenAI-compatible service). Never audio. |
| Summary (and transcript if you tick it) | Only when you click Export › Notion. |
| The latest version number from GitHub | At launch, if "Check for updates automatically" is on. |
| Usage data, analytics | Never collected. |

Audio is kept 7 days by default (1 day to forever, or not at all), then deleted; pin a meeting to keep its audio. Meetings are plain files in `~/Library/Application Support/Talaan`, readable without the app. Settings › Privacy › Delete everything removes it all, including models and keys.

## Install

1. Download the `.dmg` from [Releases](https://github.com/mmnzns/Talaan/releases/latest), open it, drag Talaan to Applications.
2. The first time, macOS says it can't check the app (Talaan isn't notarized; it's free and signed with its own certificate). Right-click Talaan › **Open**, or open it once, then System Settings › Privacy & Security › **Open Anyway**. Only once.
3. Follow the setup: allow the microphone and system audio, pick a speech model, choose how summaries are written.

Updates install from Settings › General (or the "update ready" chip) and keep your permissions, meetings and settings.

## Models

Downloaded on demand, checked by checksum, kept on your computer. Licenses in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

| Job | Model | Size |
|---|---|---|
| Speech (recommended) | Whisper Large Turbo (OpenAI) | 575 MB |
| Speech (fastest, 25 European languages) | Parakeet v3 (NVIDIA) | 671 MB |
| Speech (light) | Whisper Small (OpenAI) | 488 MB |
| Summaries | Qwen3.5 4B (recommended) or 9B | 2.7 / 5.7 GB |
| Speaker separation (optional) | Nemotron 3 Diarization (NVIDIA) | 401 MB |
| Search by meaning (optional) | Qwen3-Embedding 0.6B | 640 MB |

Already have Ollama or LM Studio? Point Summary AI at it instead.

## Build from source

Requires Rust, Node 22 and the Xcode Command Line Tools.

```bash
npm install
npm run tauri dev
```

Tests: `npm test` and `cargo test --manifest-path src-tauri/Cargo.toml --workspace`. `npm run dev` shows the UI in a browser with sample data.

Permissions (microphone, system audio, calendar, browser automation) are tied to the app's signature, so test them in a signed build: `scripts/build-signed-debug.sh`. Releases: push a `v*` tag (see `.github/workflows/release.yml`), or `scripts/release-local.sh`.

## Support

Talaan is free and open source. If it saves you time, you can [buy me a coffee](https://buymeacoffee.com/mnmnzns).

## License

MIT. Third-party libraries, fonts and models are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
