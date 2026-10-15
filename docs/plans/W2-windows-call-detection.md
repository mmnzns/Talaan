# W2. Windows call detection — task plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax.

> Part of the Windows port (docs/WINDOWS-PORT.md, phases W0–W5). Follows W0–W1
> (docs/plans/W0-W1-windows-build-and-recorder.md). Model: Sonnet for the tasks, Opus for review.

**Goal:** On Windows, Talaan notices when a call app starts and stops using the mic, and the existing rules
(Ask / Always / Never, ignored apps, call-ended prompt) work unchanged.

**Architecture:** `platform/windows/mic_users.rs` reads the microphone consent store in the registry, which
Windows keeps per app: `LastUsedTimeStart` / `LastUsedTimeStop`, with stop = 0 while the app holds the mic. The
parsing is a pure, tested function; the registry read is a thin wrapper. `calls/apps.rs` gains Windows ids in
the same prefix tables, so `identify()`, the rules and the 1 Hz loop in `commands/calls.rs` stay as they are.

**Tech stack:** Rust, `winreg` 0.56 (already a dependency).

---

## Verified facts (2026-10-15, on the PC)

`HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone`:
- **Packaged (Store/MSIX) apps:** one subkey per package family name, e.g. `MSTeams_8wekyb3d8bbwe` (new Teams),
  `91750D7E.Slack_8she8kybcnzg4` (Slack from the Store), `5319275A.WhatsAppDesktop_cv1g1gvanyjgm`,
  `Microsoft.WindowsSoundRecorder_8wekyb3d8bbwe`, `MicrosoftWindows.Client.CBS_cw5n1h2txyewy` (Windows shell:
  voice typing), `Claude_pzs8sxrjxfjjc`.
- **Desktop apps:** under `NonPackaged`, one subkey per full exe path with `\` replaced by `#`, e.g.
  `C:#Users#mikey#AppData#Roaming#Zoom#bin#Zoom.exe`, `C:#Program Files#Google#Chrome#Application#chrome.exe`,
  `C:#Users#mikey#AppData#Local#Parlato#parla.exe` (Parlato's exe is **parla.exe**),
  `C:#Program Files#NVIDIA Corporation#NVIDIA Broadcast#NVIDIA Broadcast.exe`,
  `C:#Program Files#SteelSeries#GG#apps#sonar#SteelSeriesSonar.exe`, `…#VoicemodV3#app#last#Voicemod.exe`,
  Discord under a versioned folder (`…#Discord#app-1.0.9251#Discord.exe`), many games.
- Values are `LastUsedTimeStart` / `LastUsedTimeStop` (QWORD, FILETIME). An app that never used the mic has
  neither. **In use = start > 0 and stop == 0.** (Confirm in Task 3 with a live capture: the smoke shows the
  test's own exe while it records.)
- No process id is stored; `MicUser.pid` is 0 on Windows (nothing reads it except Talaan's own exclusion, which
  Windows does by exe path).

## Design (binding)

- `MicUser.bundle_id` on Windows:
  - packaged → the package family name as stored (`MSTeams_8wekyb3d8bbwe`),
  - desktop → the exe **file name, lowercased** (`zoom.exe`, `chrome.exe`, `parla.exe`).
  `MicUser.path` → the full exe path with `#` turned back into `\` (desktop), or empty (packaged).
- Talaan itself is left out: desktop entries whose exe path equals `std::env::current_exe()` (case-insensitive),
  and anything named `talaan.exe` (dev and installed builds).
- `calls/apps.rs` prefix tables get Windows entries next to the Mac ones. Matching stays `starts_with`, which
  works for both: lowercased exe names (`zoom.exe`) and package family prefixes (`MSTeams_`).
- **Ignored on Windows** (never a call, whatever they do with the mic): Talaan (`talaan.exe`), Parlato
  (`parla.exe`, `parlato.exe`), Windows voice typing (`MicrosoftWindows.Client.CBS_`), Sound Recorder
  (`Microsoft.WindowsSoundRecorder_`), Wispr Flow (`wispr flow.exe`, `wisprflow.exe`), and mic processors that
  hold the mic all the time: NVIDIA Broadcast (`nvidia broadcast.exe`), SteelSeries Sonar (`steelseriessonar.exe`),
  Voicemod (`voicemod.exe`), Krisp (`krisp.exe`), Xbox Game Bar (`Microsoft.XboxGamingOverlay_`). Games and other
  unknown apps already never trigger (they're in neither table).
- Browsers on Windows (`browser` call app, and `calls/tabs.rs` `BROWSERS` for W4): `chrome.exe`, `msedge.exe`,
  `brave.exe`, `firefox.exe`, `opera.exe`, `vivaldi.exe`, `comet.exe`, and Arc for Windows (packaged,
  `TheBrowserCompany.Arc_`). Family: Chromium for all but Firefox (Firefox keeps no `BROWSERS` entry, as on the Mac).
- Call apps on Windows: Zoom `zoom.exe`; Teams `MSTeams_` (new) and `ms-teams.exe` / `teams.exe` (classic);
  Slack `slack.exe` and `91750D7E.Slack_`; WhatsApp `whatsapp.exe` and `5319275A.WhatsAppDesktop_`; Discord
  `discord.exe`; Webex `ciscocollabhost.exe`, `webexmta.exe`, `atmgr.exe`, `webex.exe`. FaceTime has no Windows app.
- Mac entries stay byte-for-byte; only additions.

## File map

- Modify `src-tauri/src/platform/windows/mic_users.rs` — real implementation + tests
- Modify `src-tauri/src/calls/apps.rs` — Windows ids + tests
- Modify `src-tauri/src/calls/tabs.rs` — Windows browser ids in `BROWSERS` + test (used in W4)
- Modify `src-tauri/src/commands/calls.rs` — `browsers_status` lists Windows browsers on Windows
- Modify `docs/WINDOWS-PORT.md` — progress

Commit after each task (`W2:` prefix), never push, commit messages end with
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Write files without a BOM.

---

### Task 1: Parse the consent store (pure)

**Files:** `src-tauri/src/platform/windows/mic_users.rs`

- [ ] **Step 1: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn e(key: &str, start: u64, stop: u64) -> Entry {
        Entry { key: key.into(), start, stop }
    }

    #[test]
    fn only_apps_holding_the_mic_now_are_users() {
        let packaged = [e("MSTeams_8wekyb3d8bbwe", 100, 0), e("91750D7E.Slack_8she8kybcnzg4", 100, 200), e("Clipchamp.Clipchamp_yxz26nhyzhsrt", 0, 0)];
        let desktop = [e(r"C:#Users#a#AppData#Roaming#Zoom#bin#Zoom.exe", 50, 0), e(r"C:#Program Files#Google#Chrome#Application#chrome.exe", 50, 60)];
        let users = users_from(&packaged, &desktop, r"C:\Program Files\Talaan\talaan.exe");
        assert_eq!(
            users,
            vec![
                MicUser { pid: 0, bundle_id: "MSTeams_8wekyb3d8bbwe".into(), path: String::new() },
                MicUser { pid: 0, bundle_id: "zoom.exe".into(), path: r"C:\Users\a\AppData\Roaming\Zoom\bin\Zoom.exe".into() },
            ]
        );
    }

    #[test]
    fn talaan_itself_is_left_out() {
        let desktop = [e(r"C:#Program Files#Talaan#Talaan.exe", 1, 0), e(r"C:#Dev#Talaan#src-tauri#target#debug#talaan.exe", 1, 0)];
        assert!(users_from(&[], &desktop, r"C:\Program Files\Talaan\talaan.exe").is_empty());
    }

    #[test]
    fn desktop_keys_become_paths_and_lowercase_exe_names() {
        assert_eq!(desktop_user(r"D:#SteamLibrary#x#Discord.EXE"), (r"D:\SteamLibrary\x\Discord.EXE".to_string(), "discord.exe".to_string()));
    }

    #[test]
    fn reading_the_real_registry_does_not_fail() {
        eprintln!("{:#?}", mic_users());
    }
}
```

- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml platform::windows::mic_users` → fails to compile.
- [ ] **Step 3: Implement**

```rust
//! Which apps are using the microphone right now: the per-app consent store Windows keeps in the registry
//! (`LastUsedTimeStop` is 0 while an app holds the mic). No extra permission. Talaan itself is left out.

use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const STORE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

/// A process capturing audio input. On Windows `bundle_id` is the packaged app's family name, or the desktop
/// app's exe file name in lower case; `path` is the desktop app's exe path (empty for packaged apps).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicUser {
    pub pid: i32,
    pub bundle_id: String,
    pub path: String,
}

/// One consent-store subkey: its name and last-used times (FILETIME; 0 when absent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub start: u64,
    pub stop: u64,
}

fn in_use(e: &Entry) -> bool {
    e.start > 0 && e.stop == 0
}

/// `C:#Program Files#Zoom#Zoom.exe` → (`C:\Program Files\Zoom\Zoom.exe`, `zoom.exe`).
pub fn desktop_user(key: &str) -> (String, String) {
    let path = key.replace('#', "\\");
    let exe = path.rsplit('\\').next().unwrap_or("").to_lowercase();
    (path, exe)
}

pub fn users_from(packaged: &[Entry], desktop: &[Entry], own_exe: &str) -> Vec<MicUser> {
    let packaged = packaged.iter().filter(|e| in_use(e)).map(|e| MicUser { pid: 0, bundle_id: e.key.clone(), path: String::new() });
    let desktop = desktop.iter().filter(|e| in_use(e)).filter_map(|e| {
        let (path, exe) = desktop_user(&e.key);
        let own = path.eq_ignore_ascii_case(own_exe) || exe == "talaan.exe";
        (!own).then_some(MicUser { pid: 0, bundle_id: exe, path })
    });
    packaged.chain(desktop).collect()
}

fn entries(key: &RegKey) -> Vec<Entry> {
    key.enum_keys()
        .filter_map(Result::ok)
        .filter(|name| name != "NonPackaged")
        .filter_map(|name| {
            let sub = key.open_subkey(&name).ok()?;
            let start = sub.get_value::<u64, _>("LastUsedTimeStart").unwrap_or(0);
            let stop = sub.get_value::<u64, _>("LastUsedTimeStop").unwrap_or(0);
            Some(Entry { key: name, start, stop })
        })
        .collect()
}

/// Every app other than Talaan that has the microphone open right now.
pub fn mic_users() -> Vec<MicUser> {
    let Ok(store) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(STORE) else { return vec![] };
    let packaged = entries(&store);
    let desktop = store.open_subkey("NonPackaged").map(|k| entries(&k)).unwrap_or_default();
    let own = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    users_from(&packaged, &desktop, &own)
}
```

- [ ] **Step 4: Run** → pass. The real-registry test prints the current users (probably none).
- [ ] **Step 5: Commit** `W2: which apps hold the mic on Windows (consent store)`

### Task 2: Windows call apps, ignored apps and browsers

**Files:** `src-tauri/src/calls/apps.rs`, `src-tauri/src/calls/tabs.rs`

- [ ] **Step 1: Failing tests** (append to the existing test modules)

`calls/apps.rs`:
```rust
    #[test]
    fn windows_call_apps_are_found() {
        assert_eq!(identify("zoom.exe", "", &[]), Some(("zoom", "Zoom")));
        assert_eq!(identify("MSTeams_8wekyb3d8bbwe", "", &[]), Some(("teams", "Microsoft Teams")));
        assert_eq!(identify("ms-teams.exe", "", &[]), Some(("teams", "Microsoft Teams")));
        assert_eq!(identify("slack.exe", "", &[]), Some(("slack", "Slack")));
        assert_eq!(identify("91750D7E.Slack_8she8kybcnzg4", "", &[]), Some(("slack", "Slack")));
        assert_eq!(identify("5319275A.WhatsAppDesktop_cv1g1gvanyjgm", "", &[]), Some(("whatsapp", "WhatsApp")));
        assert_eq!(identify("discord.exe", "", &[]), Some(("discord", "Discord")));
        assert_eq!(identify("ciscocollabhost.exe", "", &[]), Some(("webex", "Webex")));
        for b in ["chrome.exe", "msedge.exe", "brave.exe", "firefox.exe", "comet.exe", "TheBrowserCompany.Arc_ttt1ap7aakyb4"] {
            assert_eq!(identify(b, "", &[]).map(|a| a.0), Some("browser"), "{b}");
        }
    }

    #[test]
    fn windows_dictation_and_mic_processors_never_count() {
        for b in ["parla.exe", "talaan.exe", "MicrosoftWindows.Client.CBS_cw5n1h2txyewy", "Microsoft.WindowsSoundRecorder_8wekyb3d8bbwe", "nvidia broadcast.exe", "steelseriessonar.exe", "voicemod.exe", "krisp.exe", "wispr flow.exe", "Microsoft.XboxGamingOverlay_8wekyb3d8bbwe"] {
            assert_eq!(identify(b, "", &[]), None, "{b}");
            assert!(ignored(b, "", &[]).is_some(), "{b}");
        }
        // Games and other apps that use the mic are in neither table.
        assert_eq!(identify("bf6.exe", r"D:\SteamLibrary\steamapps\common\Battlefield 6\bf6.exe", &[]), None);
    }
```

`calls/tabs.rs`:
```rust
    #[test]
    fn windows_browsers_map_to_themselves() {
        assert_eq!(browser_for("chrome.exe"), Some(("chrome.exe", "Chrome", Family::Chromium)));
        assert_eq!(browser_for("msedge.exe"), Some(("msedge.exe", "Edge", Family::Chromium)));
        assert_eq!(browser_for("TheBrowserCompany.Arc_ttt1ap7aakyb4"), Some(("TheBrowserCompany.Arc_", "Arc", Family::Chromium)));
        assert_eq!(browser_for("firefox.exe"), None);
    }
```

  Note `browser_for` today matches `mic_user == p || starts_with(p + ".")`. Package family names continue with
  the publisher hash after `_` (`TheBrowserCompany.Arc_ttt1ap7aakyb4`), so the prefix ends in `_` and must match
  with a plain `starts_with`. Extend the match: `mic_user == *p || mic_user.starts_with(&format!("{p}.")) ||
  (p.ends_with('_') && mic_user.starts_with(p))`. The existing Mac tests must still pass (`com.google.ChromeX` → None).

- [ ] **Step 2: Run** → fail.
- [ ] **Step 3: Add the entries.** In `CALL_APPS` append to each prefix list (Mac entries unchanged):
  zoom `"zoom.exe"`; teams `"MSTeams_", "ms-teams.exe", "teams.exe"`; slack `"slack.exe", "91750D7E.Slack_"`;
  webex `"ciscocollabhost.exe", "webexmta.exe", "atmgr.exe", "webex.exe"`; discord `"discord.exe"`;
  whatsapp `"whatsapp.exe", "5319275A.WhatsAppDesktop_"`; browser `"chrome.exe", "msedge.exe", "brave.exe",
  "firefox.exe", "opera.exe", "vivaldi.exe", "comet.exe", "TheBrowserCompany.Arc_"`.
  In `IGNORED`: Talaan `"talaan.exe"`; Parlato `"parla.exe", "parlato.exe"`; Wispr Flow `"wispr flow.exe",
  "wisprflow.exe"`; Apple Dictation stays Mac-only; add new rows `("Windows voice typing",
  &["MicrosoftWindows.Client.CBS_"])`, `("Sound Recorder", &["Microsoft.WindowsSoundRecorder_"])`,
  `("NVIDIA Broadcast", &["nvidia broadcast.exe"])`, `("SteelSeries Sonar", &["steelseriessonar.exe"])`,
  `("Voicemod", &["voicemod.exe"])`, `("Krisp", &["krisp.exe"])`, `("Xbox Game Bar", &["Microsoft.XboxGamingOverlay_"])`.
  Update the module doc to mention Windows ids (exe names lowercased, package family prefixes).
  In `tabs.rs` `BROWSERS` append `("chrome.exe", "chrome.exe", "Chrome", Chromium)`, `("msedge.exe", "msedge.exe",
  "Edge", Chromium)`, `("brave.exe", …"Brave")`, `("opera.exe", …"Opera")`, `("vivaldi.exe", …"Vivaldi")`,
  `("comet.exe", …"Comet")`, `("TheBrowserCompany.Arc_", "TheBrowserCompany.Arc_", "Arc", Chromium)`.
- [ ] **Step 4: Check the settings side.** The ignored-apps list the user sees comes from settings defaults in
  the frontend (search `ignoredApps` in `src/data/`). If the defaults are Mac-only names, add the Windows ones
  (NVIDIA Broadcast, SteelSeries Sonar, Voicemod, Krisp, Windows voice typing, Sound Recorder) so they show in
  Settings › Call apps on Windows, platform-switched the same way the frontend already switches per-OS strings
  (`src/lib/platform.ts` if present on this branch; otherwise a `navigator.userAgent` check in the defaults, kept
  in one place). Keep the Mac list unchanged. Report what you found and did.
- [ ] **Step 5: Run** all `calls::` tests and `npm test` → pass.
- [ ] **Step 6: Commit** `W2: Windows call apps, ignored apps and browsers`

### Task 3: The calls loop on Windows, end to end

**Files:** `src-tauri/src/commands/calls.rs` (browsers_status), tests

- [ ] **Step 1: `browsers_status` on Windows.** Today it checks `/Applications/*.app`. Split the installed check
  into a small per-OS fn (`#[cfg(target_os = "windows")]` / macOS) in `commands/calls.rs`, keeping the macOS
  branch identical. Windows: Chrome, Edge, Brave, Arc, Opera, Vivaldi, Comet, each "installed" when its exe exists
  under `%ProgramFiles%`, `%ProgramFiles(x86)%` or `%LOCALAPPDATA%` at the usual paths (Chrome
  `Google\Chrome\Application\chrome.exe`, Edge `Microsoft\Edge\Application\msedge.exe`, Brave
  `BraveSoftware\Brave-Browser\Application\brave.exe`, Opera `Programs\Opera\opera.exe`, Vivaldi
  `Vivaldi\Application\vivaldi.exe`, Comet `Perplexity\Comet\Application\comet.exe`); Arc: installed when
  `%LOCALAPPDATA%\Packages\TheBrowserCompany.Arc_*` exists. Pure helper `installed_from(candidates, exists)` with a
  test using a fake `exists`.
- [ ] **Step 2: Live check of the consent store.** An ignored test `mic_users_live_smoke` in `mic_users.rs`:
  start a `crate::recorder::mic::MicSource`, wait 1 s, assert that `mic_users()` is empty for our own exe (we're
  excluded) **and** print the raw consent-store entry for the test exe (start > 0, stop == 0 while recording) by
  reading `users_from` with an empty own_exe. Then stop the mic, wait 1 s, and print again (stop now > 0). This
  proves the in-use rule on this machine. Report the output.
- [ ] **Step 3: Rules end to end with pretend users.** The debug hook in `commands/calls.rs` reads
  `dev-mic-users.txt` in the data folder (`%APPDATA%\Talaan`) — one bundle id per line. Don't launch the app;
  instead add a unit test in `calls/rules.rs` (or wherever `Detector` is tested) that runs `identify` + the
  `Detector` with the Windows ids: `zoom.exe` with Always → AutoStart; `MSTeams_8wekyb3d8bbwe` with Ask → Nudge;
  `parla.exe` → nothing; then the user list empties → CallEnded after the grace period. Reuse the existing test
  helpers there; read them first.
- [ ] **Step 4: Run** the whole workspace test suite. **Commit** `W2: browsers status and call rules on Windows`

### Task 4: Docs

- [ ] `docs/WINDOWS-PORT.md` Progress: W2 done (consent store, Windows ids, ignored mic processors), what's not
  done (real call with Zoom/Teams: the user's checkpoint). **Commit** `W2: docs`.
