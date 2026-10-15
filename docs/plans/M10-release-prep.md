# M10. Onboarding, updates and release prep — task plan

> Generated from Notion "Talaan v1 Build Plan" › M10. Planned on Opus (Miguel: stay on Opus; he tests the whole app at the end).
> Done when: a clean user account can install, onboard and record a meeting with no Terminal; an update from a test build to a newer build keeps permissions.

## Decisions (Opus, binding)

- **Updater:** `tauri-plugin-updater` with Talaan's own minisign key (`~/.tauri/talaan.key`, password in the login Keychain; both are GitHub secrets on `mmnzns/Talaan`). Endpoint: the latest GitHub Release's `latest.json`. Checked at launch (at most daily) and from Settings › General › Updates; "Update now" downloads, installs and relaunches. Nothing is downloaded without a click.
- **Signing:** the free self-signed "Parlato Code Signing" certificate (decision log), so permissions survive updates. The release workflow imports it from `APPLE_CERTIFICATE*` secrets like Parlato's; those secrets are **not set yet** (needs Miguel). Until then releases are built and signed locally with `scripts/release-local.sh`.
- **Open at login:** real, through `tauri-plugin-autostart` (macOS launch agent); the setting applies on change and at launch.
- **Microphone:** a real device list (cpal input devices); the recorder uses the chosen device and falls back to the system default when it's gone.
- **Onboarding** uses the real stores: microphone and system audio permissions through the recorder client, speech model download with progress, summary engine (local download or key later), call app rules, extras (calendar asks for access; tab detection; Notion points to Settings).
- **About** in Settings › General: version, MIT, source on GitHub, Buy Me a Coffee (`https://buymeacoffee.com/mnmnzns`).
- **Docs:** README (install, Open Anyway, privacy, models and their licenses, build from source), CHANGELOG with the v1.0 section (unreleased), THIRD_PARTY_NOTICES complete.

## Tasks
1. Updater plugin, config, Rust commands, Settings rows, launch check.
2. Open at login.
3. Real microphone list and selection.
4. Onboarding wired to real permissions, downloads and settings.
5. About.
6. Release workflow (tag → draft release, signed updater artifacts, latest.json) and local release script.
7. README, CHANGELOG, notices; Build Log.

## Outcome (2026-10-14)

Done: updater (launch check when allowed, Settings › Updates, never while recording), Open at login, real microphone list, onboarding on real permissions and downloads, About with Buy Me a Coffee, release workflow, local release script, README, CHANGELOG, notices. `scripts/release-local.sh` built a signed release on this Mac: Talaan.app, helper and .dmg signed by "Parlato Code Signing" with hardened runtime and the microphone, calendar and Apple Events entitlements; the updater tarball signed with Talaan's key.

Open: the release workflow needs the `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` and `APPLE_SIGNING_IDENTITY` secrets (Miguel's call: export the certificate as .p12). Until then, release from this Mac. The update path from one build to the next is on docs/TESTING.md.

