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

## License

MIT. Third-party libraries and models are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
