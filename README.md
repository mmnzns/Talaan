<p align="center"><img src="src-tauri/icons/128x128@2x.png" width="96" alt="Talaan"></p>

# Talaan

**Meeting notes that stay on your computer.** Talaan records both sides of any call (your mic and the call's audio), then writes the transcript and a summary with decisions and action items. No bot joins your meeting, and it works with any app: Zoom, Google Meet, Teams, Slack huddles, FaceTime (Mac), Discord, WhatsApp.

*Talaan* is Tagalog for "record". The logo is "ta" in Baybayin, the native Filipino script.

**Status:** v1.0 in final testing. Mac (macOS 14.4 or later, Apple silicon) and Windows (Windows 10 version 2004 or later, or Windows 11, on a 64-bit Intel or AMD PC).

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
| Your calendar (Windows) | Downloaded from the private iCal address you paste, when a recording starts (at most every 15 minutes). Nothing is sent. |
| Usage data, analytics | Never collected. |

Audio is kept 7 days by default (1 day to forever, or not at all), then deleted; pin a meeting to keep its audio. Meetings are plain files, readable without the app: `~/Library/Application Support/Talaan` on the Mac, `%APPDATA%\Talaan` on Windows. Settings › Privacy › Delete everything removes it all, including models and keys.

## Install

Download from [Releases](https://github.com/mmnzns/Talaan/releases/latest). Step by step, with screenshots of what to click: [docs/INSTALL.md](docs/INSTALL.md).

- **Windows:** run `Talaan_<version>_x64-setup.exe`. It installs for your account only, with no administrator password. Windows SmartScreen says "Windows protected your PC" the first time: click **More info**, then **Run anyway**. Talaan isn't signed with a paid certificate; it's free, and built from this source code.
- **Mac:** open the `.dmg` and drag Talaan to Applications. macOS blocks it twice (the `.dmg`, then the app): click **Done**, then **System Settings › Privacy & Security › Open Anyway**. Only once.

Then follow the setup: allow the microphone (and system audio on the Mac), pick a speech model, choose how summaries are written. Updates install from Settings › General and keep your meetings and settings.

## Models

Downloaded on demand, checked by checksum, kept on your computer. Licenses in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

| Job | Model | Size |
|---|---|---|
| Speech (recommended on the Mac) | Whisper Large Turbo (OpenAI) | 575 MB |
| Speech (fastest; recommended on Windows, 25 European languages) | Parakeet v3 (NVIDIA) | 671 MB |
| Speech (light) | Whisper Small (OpenAI) | 488 MB |
| Summaries | Qwen3.5 4B (recommended) or 9B | 2.7 / 5.7 GB |
| Speaker separation (optional) | Nemotron 3 Diarization (NVIDIA) | 401 MB |
| Search by meaning (optional) | Qwen3-Embedding 0.6B | 640 MB |

Already have Ollama or LM Studio? Point Summary AI at it instead.

## Build from source

- Mac: requires Rust, Node 22 and the Xcode Command Line Tools.
- Windows: requires Rust (`x86_64-pc-windows-msvc`), Node 22, Visual Studio Build Tools with C++, CMake and LLVM (`LIBCLANG_PATH` set to its `bin`).

```bash
npm install
npm run tauri dev
```

Tests: `npm test` and `cargo test --manifest-path src-tauri/Cargo.toml --workspace`. `npm run dev` shows the UI in a browser with sample data.

Permissions (microphone, system audio, calendar, browser automation) are tied to the app's signature, so test them in a signed build: `scripts/build-signed-debug.sh`. Releases: push a `v*` tag: CI builds Windows (`scripts/release-windows.ps1`) and drafts the release; build the Mac side with `scripts/release-local.sh`, then `scripts/upload-mac-release.sh <tag>`.

## Support

Talaan is free and open source. If it saves you time, you can [buy me a coffee](https://buymeacoffee.com/mnmnzns).

## License

MIT. Third-party libraries, fonts and models are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
