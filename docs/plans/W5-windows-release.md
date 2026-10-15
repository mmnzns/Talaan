# W5. Windows release: installer, release workflow, docs and release gate — task plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax.

> Part of the Windows port (docs/WINDOWS-PORT.md, phases W0 to W5). Model: Sonnet for the tasks, Opus for review.
> Ends at a 🛑 checkpoint: Miguel runs the Windows section of docs/TESTING.md on an installed build, then
> pushes and tags. Nothing in this plan pushes, tags or uploads.

**Goal:** a per-user Windows installer that runs on a clean PC, uninstalls cleanly, shows toasts, and
updates itself. A tag builds it on CI next to the Mac build, both platforms end up in one `latest.json`, and the
docs tell users how to install it.

**Architecture:**
- The installer is Tauri's own NSIS template, set up in `tauri.windows.conf.json`, plus one hooks file
  (`src-tauri/windows/installer-hooks.nsh`).
- Windows release builds go through one script, `scripts/release-windows.ps1`. CI runs it, and Miguel runs the
  same script on the PC to check the build.
- `latest.json` is never edited in place. `scripts/latest-json.mjs` rebuilds it from every updater
  signature (`*.sig`) on the draft release. The last uploader writes the full file, so neither platform can
  drop the other's entry.

**Tech stack:** Tauri 2.12 bundler (NSIS 3, `nsis_tauri_utils` 0.5.3), GitHub Actions, `gh` CLI, Node 22, vitest.

---

## Verified facts (2026-10-15, on the PC, read-only)

- **Tauri CLI** is 2.12.2 (npm). The `tauri` crate is 2.12.3, `tauri-plugin-updater` 2.14.0,
  `tauri-plugin-autostart` 2.7.0 (`auto-launch` 0.6.0) and `keyring` 3.6.3.

- **What the NSIS template already does** (embedded in `cli.win32-x64-msvc.node`):
  - `installMode` defaults to `currentUser`. The app installs to `$LOCALAPPDATA\Talaan` with no admin prompt.
  - `CreateOrUpdateStartMenuShortcut` creates `$SMPROGRAMS\Talaan.lnk` and runs `SetLnkAppUserModelId`. That
    writes `PKEY_AppUserModel_ID = ${BUNDLEID}` = `com.craftconceptsdigital.talaan`. This is exactly `APP_ID` in
    `platform/windows/notify.rs`, used whenever the exe isn't under `target` and isn't a debug build. **No
    `notify.rs` logic change is needed.** Task 4 only adds a test that keeps the two the same.
  - No `ToastActivatorCLSID` is set. A click on a toast after it has moved to Action Center may not reach
    Talaan (W3 note). That is a hand check, not a W5 fix.
  - Uninstall, when "Delete the application data" (`$(deleteAppData)`) is ticked and it isn't an update:
    `RmDir /r "$APPDATA\${BUNDLEID}"` and `"$LOCALAPPDATA\${BUNDLEID}"` only. Talaan's real data
    (`%APPDATA%\Talaan`) and Credential Manager entries stay behind.
  - Every non-update uninstall already runs `DeleteRegValue HKCU "...\CurrentVersion\Run" "${PRODUCTNAME}"`,
    ticked or not. It does not touch `...\Explorer\StartupApproved\Run`, which `auto-launch` also writes
    under the same value name.
  - Hook order: `NSIS_HOOK_PREINSTALL` runs before `CheckIfAppIsRunning` (main exe only) in `Section Install`.
    `NSIS_HOOK_POSTUNINSTALL` runs after the app-data block. Both `$DeleteAppDataCheckboxState` and
    `$UpdateMode` are visible in the hooks. The hooks file is `!include`d before the sections.
  - The default WebView2 mode is `downloadBootstrapper`.
- **Autostart:** the `Run` value name is `app.package_info().name` = `Talaan` (productName). The plugin passes
  `current_exe()` as the path.
- **Credential Manager:** `keyring` 3.6.3 `windows-native` writes generic credentials (`CRED_TYPE_GENERIC` = 1)
  with target `"{account}.{service}"`. Today that is `claude.Talaan`, `openai.Talaan`, `gemini.Talaan`,
  `compatible.Talaan` and `notion.Talaan`. `keys::clear_all` lists them by hand. No `calendar-ics` key exists on
  `windows-port` yet: `calendarIcsUrl` is still in settings.json. See open question 1.
- **Updater on Windows:**
  - Default `installMode` is passive. The plugin runs `<setup>.exe /P /R /UPDATE`, then
    `std::process::exit(0)`. The installer relaunches Talaan.
  - The `/UPDATE` mode keeps app data, the Run key and shortcuts.
  - `updates_install` already refuses while recording.
- **Runtime DLLs** (import tables of existing builds):
  - `talaan-llm.exe` (release, and the `binaries/` copy) imports `VCRUNTIME140.dll`, `VCRUNTIME140_1.dll` and
    `MSVCP140.dll`.
  - `target/debug/talaan.exe` imports `MSVCP140.dll`, `MSVCP140_1.dll` and `directml.dll`. Tauri's static
    vcruntime removes `VCRUNTIME140` but not the C++ library.
  - `DirectML.dll` is a symlink that `ort-sys` (copy-dylibs) drops next to the binaries, pointing into
    `%LOCALAPPDATA%\ort.pyke.io\dfbin\...`. ONNX Runtime itself is linked statically.
  - None of these are in the installer today. A PC without the VC++ Redistributable can't start Talaan, and
    without the bundled `DirectML.dll` it falls back to System32's (version unknown).
  - Tauri 2.12 has `bundle.windows.bundleVCRuntime` (default false).
- **The optimization flags reach the release build:**
  - `.cargo/config.toml` is at the repo root. Cargo looks for config from the working directory upward, and both
    `tauri build` (runs in `src-tauri`) and `build-llm-helper.ps1` (`Set-Location src-tauri`) are below it. CI
    checks out the repo root.
  - The current release caches confirm it: `whisper-rs-sys-*/out/build/CMakeCache.txt` and
    `llama-cpp-sys-2-*/out/build/CMakeCache.txt` both say `CMAKE_C_FLAGS_RELEASE:STRING=-O2 -DNDEBUG`. That file
    is the check used in Task 3.
- **`.gitignore`:** `src-tauri/binaries/` is ignored (`src-tauri/.gitignore`).
- **tauri-action v0 (`upload-version-json.ts`):**
  - It needs a release id. It downloads an existing `latest.json` asset, keeps its `platforms`, writes its own
    keys over them, then deletes the old asset and uploads the new one.
  - It writes `windows-x86_64` plus `windows-x86_64-nsis`, and `darwin-aarch64` plus `darwin-aarch64-app`.
  - Two jobs finishing close together can each read the old file, so the later upload drops the other's entry.
    It also never runs for the local Mac flow.
- **`scripts/release-local.sh`** builds the .dmg, `Talaan.app.tar.gz` and `.sig`. It uploads nothing and writes
  no `latest.json`.
- **The `macos` job in `release.yml`** fails at "Signing certificate" today, because the `APPLE_CERTIFICATE*`
  secrets aren't set (M10).
- **Delete everything:**
  - `commands/settings.rs` `data_delete_all` ends with `app.restart()`.
  - Every UI string says it quits: `PrivacyTab.tsx` ("Then quits.", "Talaan will quit."), and the i18n branch
    `claude/optimistic-leavitt-332ac0` (`wipe_desc` and `wipe_body` in EN, FR and ES).
  - `app_quit` uses `app.exit(0)`. The `ExitRequested` handler runs `rec_cmd::stop_on_exit`.
- **`docs/INSTALL.md`** (untracked) mentions features that only exist on the unmerged branches
  (`claude/optimistic-leavitt-332ac0`, `claude/eloquent-hugle-187866`). These are Settings › General › Check setup,
  Test call audio, Check again, Open Windows Settings, and the working Privacy › Open folder / Export all. On
  `windows-port`, Open folder and Export all are still placeholder toasts.
- **Version:** `1.0.0-dev` in `tauri.conf.json`, `Cargo.toml` and `package.json`.

## Design (binding)

- **Install mode:** `currentUser`, set explicitly.
- **WebView2:** `embedBootstrapper`, silent.
  - Every Windows 11, and Windows 10 2004+ with updates, already has the runtime, so this almost never runs.
  - When it does, the ~1.8 MB embedded bootstrapper saves one more download from go.microsoft.com, and it runs
    without prompts. It still needs internet, which Talaan needs anyway for its models.
  - `offlineInstaller` (+127 MB) isn't worth it.
- **Runtime DLLs:**
  - `bundleVCRuntime: true` puts the VC++ runtime next to `talaan.exe` and `talaan-llm.exe` (same folder).
  - `DirectML.dll` from ONNX Runtime's package ships as a resource at `$INSTDIR\DirectML.dll`.
  - Both are release-only (the resource goes in a config passed by `release-windows.ps1`), so `tauri dev` and
    `cargo test` don't change.
- **Hooks file** (`src-tauri/windows/installer-hooks.nsh`):
  - PREINSTALL and PREUNINSTALL close a running `talaan-llm.exe`, so an update or uninstall never hits a locked
    helper. The helper exits on stdin EOF, but it finishes a running summary first.
  - POSTUNINSTALL (not an update) removes `StartupApproved\Run\Talaan`.
  - With the box ticked it also removes `%APPDATA%\Talaan` and every Credential Manager entry in
    `keys::ACCOUNTS`.
  - A Rust test fails if an account in `ACCOUNTS` is missing from the .nsh.
- **Publisher fields** (Windows config only, so the Mac bundle doesn't change): publisher, homepage, copyright,
  shortDescription. The publisher becomes the NSIS manufacturer registry key, so pick it once (open question 2).
- **Release workflow:**
  - A `draft` job creates the draft release for the tag once.
  - `windows` runs `scripts/release-windows.ps1` with the updater secrets and uploads the setup .exe and its .sig.
  - `macos` stays as it is but only runs when the repo variable `MAC_RELEASE_ON_CI` is `true`. It no longer writes
    `latest.json` (`includeUpdaterJson: false`).
  - `updater-json` (after both) runs `node scripts/latest-json.mjs <tag>`.
  - The Mac local flow gets `scripts/upload-mac-release.sh <tag>`, which uploads and runs the same script.
  - **Order doesn't matter:** whoever runs last rebuilds `latest.json` from all signatures present.
- **Asset names on the release:**
  - `Talaan_<v>_x64-setup.exe` and `.exe.sig`
  - `Talaan_<v>_aarch64.dmg`
  - `Talaan_aarch64.app.tar.gz` and `.sig`. This is tauri-action's naming; the local upload copies to it.
  - `latest.json`
- **`latest.json` keys:**
  - `windows-x86_64` and `windows-x86_64-nsis` point to the setup .exe.
  - `darwin-aarch64` and `darwin-aarch64-app` point to the .app.tar.gz.
  - URLs are `https://github.com/mmnzns/Talaan/releases/download/<tag>/<asset>`. They work once the draft is
    published.
- **Delete everything quits.** Any active recording is discarded first. Otherwise, on Windows the open WAV files
  make `remove_dir_all` fail halfway, and on the Mac `stop_on_exit` writes a half meeting into the fresh folder.
  The UI is not touched.

## File map

- Create:
  - `src-tauri/windows/installer-hooks.nsh`
  - `scripts/release-windows.ps1`
  - `scripts/latest-json.mjs`, `scripts/latest-json.test.mjs`
  - `scripts/upload-mac-release.sh`
  - `scripts/release-notes.md`
- Modify:
  - `src-tauri/tauri.windows.conf.json`
  - `src-tauri/src/keys.rs` (`ACCOUNTS` + test)
  - `src-tauri/src/platform/windows/notify.rs` (test + doc)
  - `src-tauri/src/commands/settings.rs`, `src-tauri/src/commands/recorder.rs` (discard on wipe)
  - `src/data/tauri/settingsStore.ts` (comment only)
  - `.github/workflows/release.yml`
  - `README.md`, `docs/INSTALL.md` (commit it), `docs/TESTING.md`, `docs/WINDOWS-PORT.md`, `CHANGELOG.md`,
    `THIRD_PARTY_NOTICES.md`

Rules:
- Commit per task with the `W5:` prefix. Never push or tag.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- No BOM in any file (PowerShell: `[IO.File]::WriteAllText`). Stage only your own files: another agent commits
  on this branch.
- **macOS behaviour must not change,** except Task 5 (quit instead of restart, as the UI already says).
  `release-local.sh` stays as it is.
- Before each commit run `cargo test --manifest-path src-tauri/Cargo.toml --workspace`, `npm test` and
  `npm run build`.

---

### Task 1: Baseline release build on the PC (no commit)

Find out what today's config produces before changing it.

- [ ] Quit any installed Talaan. Then run:

```powershell
npm run tauri build
Get-ChildItem src-tauri\target\release\bundle\nsis
```

  Expected: `Talaan_1.0.0-dev_x64-setup.exe`. Note the exact name: NSIS's `VIProductVersion` needs four numbers,
  and the bundler drops `-dev`. If the build fails on the version, write it down. Release builds use `1.0.0`
  anyway, and Task 3's checks run with `1.0.0-dev`.
  **Result (W5 run, 2026-10-15):** `Talaan_1.0.0-dev_x64-setup.exe`, 10.5 MB. The bundler keeps `-dev` in the
  file name (it only drops it for `VIProductVersion`), and the build works.
- [ ] Check what the release exe imports (from a "Developer PowerShell for VS"):

```powershell
dumpbin /dependents src-tauri\target\release\talaan.exe
dumpbin /dependents src-tauri\target\release\talaan-llm.exe
```

  Expected: `MSVCP140.dll` (and `_1`), `VCRUNTIME140*.dll` for the helper, and `directml.dll`. Write the list into
  Task 3's commit message.
- [ ] Check the optimization flags:

```powershell
Select-String -Path src-tauri\target\release\build\*\out\build\CMakeCache.txt -Pattern '^CMAKE_(C|CXX)_FLAGS_RELEASE:STRING'
```

  Expected: every line ends with `-O2 -DNDEBUG`.

### Task 2: Installer config and uninstall hooks

**Files:** `src-tauri/tauri.windows.conf.json`, create `src-tauri/windows/installer-hooks.nsh`, modify `src-tauri/src/keys.rs`

- [ ] **Failing test** in `keys.rs` (pure, runs on the Mac too):

```rust
    #[test]
    fn the_windows_uninstaller_removes_every_key() {
        let nsh = include_str!("../windows/installer-hooks.nsh");
        for a in ACCOUNTS {
            assert!(nsh.contains(&format!("TalaanDeleteKey \"{a}\"")), "{a} is missing from installer-hooks.nsh");
        }
    }
```

- [ ] `keys.rs`: one list for "delete everything" and the uninstaller.

```rust
/// Every account Talaan stores (service "Talaan"). Windows names them "<account>.Talaan" in Credential Manager.
/// The Windows uninstaller deletes the same list (src-tauri/windows/installer-hooks.nsh, checked by a test).
pub const ACCOUNTS: [&str; 6] = ["claude", "openai", "gemini", "compatible", "notion", "calendar-ics"];

/// "Delete all Talaan data" removes every key too.
pub fn clear_all() {
    for p in ACCOUNTS {
        let _ = clear(p);
    }
}
```

  If the calendar agent already added `"calendar-ics"` somewhere, use its constant. Don't add a second name.
  Deleting a key that was never saved is already `Ok` (`NoEntry`), so this is harmless on the Mac.
- [ ] `src-tauri/windows/installer-hooks.nsh`:

```nsis
; Talaan's NSIS hooks (bundle.windows.nsis.installerHooks in tauri.windows.conf.json).
; Tauri's uninstaller, when "Delete the application data" is ticked, removes only %APPDATA% and %LOCALAPPDATA%
; folders named after the bundle id. Talaan keeps meetings, models and settings in %APPDATA%\Talaan and API keys
; in Credential Manager, so those go here too. Updates (/UPDATE) never delete anything.

; One Credential Manager entry: keyring writes generic credentials (type 1) named "<account>.Talaan".
!macro TalaanDeleteKey ACCOUNT
  System::Call 'advapi32::CredDeleteW(w "${ACCOUNT}.Talaan", i 1, i 0) i .r0'
!macroend

; The summary helper can outlive Talaan for a few seconds; close it so its exe isn't locked.
!macro NSIS_HOOK_PREINSTALL
  !insertmacro CheckIfAppIsRunning "$INSTDIR\talaan-llm.exe" "${PRODUCTNAME}"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro CheckIfAppIsRunning "$INSTDIR\talaan-llm.exe" "${PRODUCTNAME}"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    ; Tauri removes the Run value; auto-launch also wrote this one under the same name.
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Talaan"
    ${If} $DeleteAppDataCheckboxState = 1
      SetShellVarContext current
      RmDir /r "$APPDATA\Talaan"
      !insertmacro TalaanDeleteKey "claude"
      !insertmacro TalaanDeleteKey "openai"
      !insertmacro TalaanDeleteKey "gemini"
      !insertmacro TalaanDeleteKey "compatible"
      !insertmacro TalaanDeleteKey "notion"
      !insertmacro TalaanDeleteKey "calendar-ics"
    ${EndIf}
  ${EndIf}
!macroend
```

  `CheckIfAppIsRunning` is the template's own macro, defined before `Section Install` uses it. If `makensis` says
  it is unknown inside a hook, replace both lines with a silent kill:
  `nsExec::Exec 'taskkill /F /IM talaan-llm.exe'` then `Pop $0`.
- [ ] `tauri.windows.conf.json` (the `build` block stays as it is):

```json
{
  "build": {
    "beforeDevCommand": "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-llm-helper.ps1 && npm run dev",
    "beforeBuildCommand": "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-llm-helper.ps1 && npm run build"
  },
  "bundle": {
    "targets": ["nsis"],
    "publisher": "Craft Concepts Digital",
    "homepage": "https://github.com/mmnzns/Talaan",
    "copyright": "© 2026 Craft Concepts Digital. MIT License.",
    "shortDescription": "Meeting notes that stay on your computer",
    "windows": {
      "webviewInstallMode": { "type": "embedBootstrapper", "silent": true },
      "bundleVCRuntime": true,
      "nsis": {
        "installMode": "currentUser",
        "installerIcon": "icons/icon.ico",
        "installerHooks": "windows/installer-hooks.nsh"
      }
    }
  }
}
```

- [ ] Run the tests. Then `npm run tauri build`. Expected: the build succeeds and `makensis` reports no errors
  for the hooks.
- [ ] **Install check (do it):**
  - Run the setup .exe. There is no UAC prompt, it installs to `%LOCALAPPDATA%\Talaan`, and Talaan starts.
  - `Get-ChildItem $env:LOCALAPPDATA\Talaan` lists `talaan.exe`, `talaan-llm.exe`, `uninstall.exe` and the VC
    runtime DLLs (`vcruntime140.dll`, `vcruntime140_1.dll`, `msvcp140.dll`, `msvcp140_1.dll`).
  - **If the `_1` DLLs are missing,** `bundleVCRuntime` doesn't cover them. Add them as resources in Task 3 from
    `"$(vswhere -latest -find VC\Redist\MSVC\*\x64\Microsoft.VC143.CRT)"`.
- [ ] **Uninstall check (do it):**
  - Save a fake key: `cmdkey /generic:notion.Talaan /user:notion /pass:x`.
  - Uninstall with the box ticked, then run:

```powershell
Test-Path $env:APPDATA\Talaan          # False
cmdkey /list | Select-String Talaan     # nothing
reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v Talaan   # "unable to find"
```

  Install again and uninstall **without** the box: `%APPDATA%\Talaan` is still there.
- [ ] Commit `W5: per-user NSIS installer; uninstall removes Talaan's data, keys and startup entry when asked`.

### Task 3: Release build script and runtime DLLs

**Files:** create `scripts/release-windows.ps1`; modify `THIRD_PARTY_NOTICES.md`

- [ ] Write `scripts/release-windows.ps1`:

```powershell
# Builds the Windows release: Talaan_<version>_x64-setup.exe and, when TAURI_SIGNING_PRIVATE_KEY is set (CI),
# its updater signature (.exe.sig). The release workflow runs this on windows-latest; run it on the PC to check
# the same build before tagging. Output: src-tauri\target\release\bundle\nsis.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
$version = node -p "require('./src-tauri/tauri.conf.json').version"
if ($env:GITHUB_REF_NAME -and $env:GITHUB_REF_NAME -ne "v$version") { throw "Tag $env:GITHUB_REF_NAME but tauri.conf.json says $version" }

# ONNX Runtime (Parakeet, diarization) is linked statically but imports DirectML.dll. ort-sys's build script
# drops it next to the binaries (as a link into its cache); copy the real file where the bundle can take it.
cargo build --release --manifest-path src-tauri/Cargo.toml -p ort-sys
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
New-Item -ItemType Directory -Force src-tauri\binaries | Out-Null
Copy-Item src-tauri\target\release\DirectML.dll src-tauri\binaries\DirectML.dll -Force

# Release-only config: updater artifacts when the key is there, and DirectML.dll beside talaan.exe.
$signed = [bool]$env:TAURI_SIGNING_PRIVATE_KEY
$conf = @{ bundle = @{ createUpdaterArtifacts = $signed; resources = @{ 'binaries/DirectML.dll' = 'DirectML.dll' } } } | ConvertTo-Json -Depth 5
$confPath = Join-Path $env:TEMP 'talaan-release.conf.json'
[IO.File]::WriteAllText($confPath, $conf)
npm run tauri build -- --config $confPath
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# whisper.cpp and llama.cpp must be optimized (.cargo/config.toml); cmake-rs alone builds them at /Od.
$caches = Get-ChildItem src-tauri\target\release\build -Directory |
  Where-Object { $_.Name -match '^(whisper-rs-sys|llama-cpp-sys-2)-' } |
  ForEach-Object { Join-Path $_.FullName 'out\build\CMakeCache.txt' } | Where-Object { Test-Path $_ }
if (@($caches | Where-Object { $_ -match 'whisper-rs-sys' }).Count -eq 0 -or @($caches | Where-Object { $_ -match 'llama-cpp-sys-2' }).Count -eq 0) { throw "No CMake cache for whisper.cpp or llama.cpp" }
foreach ($c in $caches) {
  $lines = Select-String -Path $c -Pattern '^CMAKE_(C|CXX)_FLAGS_RELEASE:STRING=(.*)$'
  foreach ($l in $lines) { if ($l.Matches[0].Groups[2].Value -notmatch '-O2') { throw "Not optimized: $c ($($l.Line))" } }
}
Write-Host "whisper.cpp and llama.cpp built with -O2"

$out = 'src-tauri\target\release\bundle\nsis'
$exe = Get-ChildItem $out -Filter '*_x64-setup.exe' | Sort-Object LastWriteTime | Select-Object -Last 1
if (-not $exe) { throw "No installer in $out" }
if ($signed -and -not (Test-Path "$($exe.FullName).sig")) { throw "No updater signature for $($exe.Name)" }
Get-ChildItem $out -Filter "$($exe.BaseName)*" | ForEach-Object { "{0}  {1:N1} MB" -f $_.Name, ($_.Length / 1MB) }
```

  Notes for the worker:
  - `-p ort-sys` is a dependency, not a member. Cargo allows it. If the lockfile ever holds two `ort-sys`
    versions, use `-p ort-sys@2.0.0-rc.13`.
  - `Copy-Item` follows the symlink, so `binaries\DirectML.dll` is a real file.
  - tauri-build copies resources into `target\release\` at compile time. It writes the same bytes over the
    symlink's target, which is harmless.
- [ ] **Run it (unsigned):** `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-windows.ps1`.
  Expected: "whisper.cpp and llama.cpp built with -O2", then the installer name and size.
- [ ] **Run it signed with a throwaway key** (the real key never leaves CI):

```powershell
npm run tauri signer generate -- --ci -p test -f -w "$env:TEMP\talaan-test.key"
$env:TAURI_SIGNING_PRIVATE_KEY = "$env:TEMP\talaan-test.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "test"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-windows.ps1
Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY, Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

  Expected: `Talaan_<v>_x64-setup.exe` and `Talaan_<v>_x64-setup.exe.sig` (a short base64 text file). The
  configured pubkey isn't checked at build time, so the throwaway key works.
- [ ] **Clean PC check (do it, Windows Sandbox).** It has Edge and WebView2 but no VC++ Redistributable.
  - Turn on "Windows Sandbox" (Windows Features) and map `src-tauri\target\release\bundle\nsis` into it.
  - Install, open Talaan, download Parakeet and transcribe the fixture: drag `fixtures\three-speakers\them.wav`
    onto the window.
  - Expected: Talaan starts (no "MSVCP140.dll was not found"), Parakeet loads (DirectML.dll), and the local
    summary helper starts.
  - If Talaan doesn't start, run `talaan.exe` from a console in the Sandbox and read the error dialog. Fix it in
    this task.
- [ ] `THIRD_PARTY_NOTICES.md`: add rows for:
  - DirectML (Microsoft DirectML redistributable; check the license file in the ort cache folder next to
    `DirectML.dll`). **W5 run:** the ort cache folder holds only `DirectML.dll` (1.15.4) and `onnxruntime.lib`, no
    license file; the row cites the Microsoft.AI.DirectML NuGet package's license terms instead;
  - the Microsoft Visual C++ Redistributable (Microsoft Software License Terms, redistributable files);
  - NSIS (zlib/libpng license, only the installer stub);
  - `nsis_tauri_utils` (MIT/Apache-2.0).
- [ ] Commit `W5: scripts/release-windows.ps1 (DirectML.dll and VC++ runtime in the installer, -O2 check)`.

### Task 4: Toasts from the installed app

**Files:** `src-tauri/src/platform/windows/notify.rs`

- [ ] **Test** (it fails if someone changes either side alone):

```rust
    /// The installer stamps the Start Menu shortcut with the bundle identifier as its AppUserModelID (Tauri's
    /// NSIS template, SetLnkAppUserModelId). Windows shows our toasts only under that id.
    #[test]
    fn the_app_id_is_the_identifier_the_installer_puts_on_the_shortcut() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../../../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], APP_ID);
    }
```

- [ ] In the module doc comment, change "the installer's (Talaan's bundle id)" to "the Start Menu shortcut the
  installer creates carries the bundle id as its AppUserModelID".
- [ ] **Check (do it, installed build from Task 3):**

```powershell
$f = (New-Object -ComObject Shell.Application).Namespace("$env:APPDATA\Microsoft\Windows\Start Menu\Programs")
$f.ParseName("Talaan.lnk").ExtendedProperty("System.AppUserModel.ID")
```

  Expected: `com.craftconceptsdigital.talaan`. Then record 20 s and stop. A "Summary ready" toast appears, and
  clicking it opens the meeting.
- [ ] Commit `W5: test that toasts use the installer's AppUserModelID`.

### Task 5: Delete everything quits (and discards a live recording)

**Files:** `src-tauri/src/commands/settings.rs`, `src-tauri/src/commands/recorder.rs`, `src/data/tauri/settingsStore.ts`

- [ ] `recorder.rs`, next to `stop_on_exit`:

```rust
/// "Delete everything" while recording: drop the recording (its files are about to go) so nothing writes into
/// the emptied folder and Windows has no open WAV files blocking the delete.
pub fn discard_for_wipe(app: &AppHandle) {
    let Some(rec) = app.try_state::<RecorderState>() else { return };
    let mut engine = rec.lock();
    if engine.is_active() {
        let state = app.state::<AppState>();
        let _ = engine.discard(&state.lock(), platform::host_now_ns());
    }
}
```

- [ ] `settings.rs`:

```rust
/// Deletes everything, then quits (as the Privacy tab says). The next launch starts at onboarding.
#[tauri::command]
pub fn data_delete_all(app: AppHandle, state: State<AppState>) -> CmdResult<()> {
    super::recorder::discard_for_wipe(&app);
    state.lock().delete_all_data()?;
    crate::keys::clear_all();
    super::updates::remove_login_item(&app);
    app.exit(0);
    Ok(())
}
```

  The lock order is the recorder, then storage, the same as `recorder_discard`. `discard_for_wipe` drops both
  locks before `state.lock()`. The `MEETINGS_CHANGED` and `SETTINGS_CHANGED` notifications go away because the
  app exits.
- [ ] `settingsStore.ts:166` comment: `// Talaan deletes everything, then quits; the next launch is first-run.`
  No UI text changes: it already says "quits" here and on the i18n branch.
- [ ] Tests green (`tauriStores.test.ts` still only checks that the command is called).
- [ ] **Manual:**
  - Windows: Settings › Privacy › Delete everything › Delete everything. Talaan quits, and the tray icon
    disappears. Start it again: onboarding, `%APPDATA%\Talaan` holds only an empty `meetings`, and
    `cmdkey /list | Select-String Talaan` shows nothing.
  - Repeat while recording: same result, and no half meeting afterwards.
  - Mac (note it for Miguel in TESTING.md): same, from the Dock.
- [ ] Commit `W5: Delete everything quits, as the Privacy tab says (and drops a live recording first)`.

### Task 6: One latest.json from every signature

**Files:** create `scripts/latest-json.mjs`, `scripts/latest-json.test.mjs`, `scripts/release-notes.md`

- [ ] **Failing tests** (`scripts/latest-json.test.mjs`; vitest's default include picks it up):

```js
import { describe, expect, it } from "vitest";
import { buildLatest, platformsFor } from "./latest-json.mjs";

const base = { repo: "mmnzns/Talaan", tag: "v1.0.0", notes: "n", pubDate: "2026-10-20T00:00:00.000Z" };

describe("latest.json", () => {
  it("maps updater signatures to the updater's platform keys", () => {
    expect(platformsFor("Talaan_1.0.0_x64-setup.exe.sig")).toEqual(["windows-x86_64", "windows-x86_64-nsis"]);
    expect(platformsFor("Talaan_aarch64.app.tar.gz.sig")).toEqual(["darwin-aarch64", "darwin-aarch64-app"]);
    expect(platformsFor("Talaan_1.0.0_aarch64.dmg")).toEqual([]);
  });

  it("holds both platforms whichever was uploaded first", () => {
    const sigs = [
      { name: "Talaan_aarch64.app.tar.gz.sig", signature: "MAC\n" },
      { name: "Talaan_1.0.0_x64-setup.exe.sig", signature: "WIN" },
    ];
    const a = buildLatest({ ...base, sigs });
    const b = buildLatest({ ...base, sigs: [...sigs].reverse() });
    expect(a).toEqual(b);
    expect(a.version).toBe("1.0.0");
    expect(a.platforms["windows-x86_64"]).toEqual({
      signature: "WIN",
      url: "https://github.com/mmnzns/Talaan/releases/download/v1.0.0/Talaan_1.0.0_x64-setup.exe",
    });
    expect(a.platforms["darwin-aarch64"].signature).toBe("MAC");
  });

  it("refuses to write an empty file", () => {
    expect(() => buildLatest({ ...base, sigs: [] })).toThrow(/no updater signatures/);
  });
});
```

- [ ] `scripts/latest-json.mjs`:

```js
// Rebuilds latest.json (what Talaan's updater reads) from every updater signature on a GitHub release, then
// uploads it. Each platform uploads its own files at its own time (Windows on CI, the Mac from this Mac), and
// whoever runs this last writes the full file, so no platform's entry is lost.
//   node scripts/latest-json.mjs v1.0.0            uses the draft or published release for that tag (gh CLI)
//   node scripts/latest-json.mjs v1.0.0 --dir DIR  prints the file built from DIR/*.sig, uploads nothing
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

export function platformsFor(name) {
  if (/_x64-setup\.exe\.sig$/.test(name)) return ["windows-x86_64", "windows-x86_64-nsis"];
  if (/\.app\.tar\.gz\.sig$/.test(name)) return /x86_64|_x64/.test(name) ? ["darwin-x86_64", "darwin-x86_64-app"] : ["darwin-aarch64", "darwin-aarch64-app"];
  return [];
}

export function buildLatest({ repo, tag, notes, pubDate, sigs }) {
  const platforms = {};
  for (const { name, signature } of [...sigs].sort((a, b) => a.name.localeCompare(b.name))) {
    const url = `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(name.slice(0, -".sig".length))}`;
    for (const key of platformsFor(name)) platforms[key] = { signature: signature.trim(), url };
  }
  if (Object.keys(platforms).length === 0) throw new Error("no updater signatures on the release");
  return { version: tag.replace(/^v/, ""), notes, pub_date: pubDate, platforms };
}

const gh = (...args) => execFileSync("gh", args, { encoding: "utf8" });

function main([tag, flag, dir]) {
  if (!tag) throw new Error("usage: node scripts/latest-json.mjs v1.0.0 [--dir DIR]");
  const repo = process.env.GITHUB_REPOSITORY || "mmnzns/Talaan";
  const notes = `What's new: https://github.com/${repo}/releases/tag/${tag}`;
  const pubDate = new Date().toISOString();
  const local = flag === "--dir";
  const from = local ? dir : mkdtempSync(join(tmpdir(), "talaan-latest-"));
  if (!local) gh("release", "download", tag, "--repo", repo, "--pattern", "*.sig", "--dir", from, "--clobber");
  const sigs = readdirSync(from).filter((n) => n.endsWith(".sig")).map((name) => ({ name, signature: readFileSync(join(from, name), "utf8") }));
  const json = JSON.stringify(buildLatest({ repo, tag, notes, pubDate, sigs }), null, 2);
  if (local) return console.log(json);
  const file = join(from, "latest.json");
  writeFileSync(file, json);
  gh("release", "upload", tag, file, "--repo", repo, "--clobber");
  console.log(`latest.json: ${Object.keys(JSON.parse(json).platforms).join(", ")}`);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) main(process.argv.slice(2));
```

  `gh release download/upload <tag>` also finds draft releases (gh falls back to listing drafts when the tag
  lookup returns 404). Check this once in Task 7's local dry run (open question 3).
- [ ] `scripts/release-notes.md` (the draft's description, used by CI and the Mac upload):

```markdown
**First install**

- **Windows 10 (2004 or later) or 11, x64:** run `Talaan_<version>_x64-setup.exe`. If Windows says "Windows protected your PC", click **More info**, then **Run anyway**. Only once.
- **Mac (Apple silicon, macOS 14.4 or later):** open the `.dmg` and drag Talaan to Applications. macOS blocks it twice: each time click **Done**, then **System Settings › Privacy & Security › Open Anyway**.

Step by step: [docs/INSTALL.md](https://github.com/mmnzns/Talaan/blob/main/docs/INSTALL.md). What's new: [CHANGELOG.md](https://github.com/mmnzns/Talaan/blob/main/CHANGELOG.md).
Already installed? Talaan updates itself from Settings › General › Updates. The `latest.json`, `.sig` and `.tar.gz` files are for that.
```

- [ ] `npm test` (new tests pass). Local dry run with Task 3's signed output:

```powershell
node scripts/latest-json.mjs v1.0.0-dev --dir src-tauri\target\release\bundle\nsis
```

  Expected: JSON with `windows-x86_64` and `windows-x86_64-nsis`, and the signature from the .sig.
- [ ] Commit `W5: latest.json rebuilt from every updater signature on the release`.

### Task 7: Release workflow and the Mac upload

**Files:** `.github/workflows/release.yml`, create `scripts/upload-mac-release.sh`

- [ ] `release.yml`. The header comment describes the new flow. Keep the `macos` steps exactly as they are,
  with three changes: `needs: draft`, the `if:`, and `includeUpdaterJson: false`.

```yaml
name: Release

# Push a tag like v1.0.0. This makes a draft GitHub Release with:
#   Windows (CI):  Talaan_<v>_x64-setup.exe and its .sig (built by scripts/release-windows.ps1)
#   Mac:           the .dmg, Talaan_aarch64.app.tar.gz and its .sig, uploaded from the Mac with
#                  scripts/release-local.sh then scripts/upload-mac-release.sh <tag> (or by the macos job below
#                  once the APPLE_CERTIFICATE secrets exist and the repo variable MAC_RELEASE_ON_CI is "true")
#   latest.json:   rebuilt from every .sig on the release by scripts/latest-json.mjs, by whoever uploads last
# Publish the draft by hand after the release gate (docs/TESTING.md).
#
# Secrets:
#   TAURI_SIGNING_PRIVATE_KEY / _PASSWORD   Talaan's updater key (minisign), used by both platforms
#   APPLE_CERTIFICATE, APPLE_CERTIFICATE_PASSWORD, APPLE_SIGNING_IDENTITY   macos job only (self-signed .p12)

on:
  push:
    tags: ["v*"]

permissions:
  contents: write

jobs:
  draft:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Version matches the tag
        run: |
          want="${GITHUB_REF_NAME#v}"
          have=$(node -p "require('./src-tauri/tauri.conf.json').version")
          [ "$want" = "$have" ] || { echo "::error::tag $GITHUB_REF_NAME but tauri.conf.json says $have"; exit 1; }
      - name: Draft release for this tag (once)
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          gh release view "$GITHUB_REF_NAME" >/dev/null 2>&1 || \
            gh release create "$GITHUB_REF_NAME" --draft --verify-tag --title "Talaan $GITHUB_REF_NAME" --notes-file scripts/release-notes.md

  windows:
    needs: draft
    runs-on: windows-latest
    env:
      LIBCLANG_PATH: C:\Program Files\LLVM\bin
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri
          key: release-windows
      - run: npm ci
      # Builds the helper (beforeBuildCommand), the installer and its updater signature; fails if whisper.cpp or
      # llama.cpp weren't built with -O2 (.cargo/config.toml).
      - name: Build the installer
        shell: pwsh
        env:
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        run: ./scripts/release-windows.ps1
      - name: Upload to the draft
        shell: bash
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          v="${GITHUB_REF_NAME#v}"; out=src-tauri/target/release/bundle/nsis
          gh release upload "$GITHUB_REF_NAME" "$out/Talaan_${v}_x64-setup.exe" "$out/Talaan_${v}_x64-setup.exe.sig" --clobber

  macos:
    needs: draft
    if: vars.MAC_RELEASE_ON_CI == 'true'
    runs-on: macos-latest
    steps:
      # ... unchanged steps ...
      - name: Build, sign and draft the release
        uses: tauri-apps/tauri-action@v0
        # ... unchanged env ...
        with:
          tagName: ${{ github.ref_name }}
          releaseName: "Talaan ${{ github.ref_name }}"
          releaseBody: "See CHANGELOG.md for what's new. First install: open the .dmg, drag Talaan to Applications, then right-click › Open (or System Settings › Privacy & Security › Open Anyway) once."
          releaseDraft: true
          includeUpdaterJson: false
          args: --target aarch64-apple-darwin
      # ... unchanged signature check ...

  updater-json:
    needs: [windows, macos]
    if: ${{ always() && needs.windows.result == 'success' && needs.macos.result != 'failure' }}
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
      - env:
          GH_TOKEN: ${{ github.token }}
        run: node scripts/latest-json.mjs "$GITHUB_REF_NAME"
```

  The Mac job's own "Version matches the tag" step stays (harmless duplicate). tauri-action finds the draft that
  `draft` created (it looks for a draft with the same tag).
- [ ] `scripts/upload-mac-release.sh` (run on the Mac after `release-local.sh`; `release-local.sh` is unchanged):

```bash
#!/bin/bash
# Uploads the Mac build from scripts/release-local.sh to the GitHub release for TAG (draft or not; it makes the
# draft if CI hasn't yet), then rebuilds latest.json from every updater signature on it, so the Windows entry from
# CI stays. Usage: scripts/upload-mac-release.sh v1.0.0
set -euo pipefail
cd "$(dirname "$0")/.."
TAG="${1:?usage: scripts/upload-mac-release.sh v1.0.0}"
VERSION=$(node -p "require('./src-tauri/tauri.conf.json').version")
[ "$TAG" = "v$VERSION" ] || { echo "tag $TAG but tauri.conf.json says $VERSION" >&2; exit 1; }
OUT=src-tauri/target/aarch64-apple-darwin/release/bundle
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
# Same names tauri-action uses, so a CI-built Mac release looks the same.
cp "$OUT/macos/Talaan.app.tar.gz" "$TMP/Talaan_aarch64.app.tar.gz"
cp "$OUT/macos/Talaan.app.tar.gz.sig" "$TMP/Talaan_aarch64.app.tar.gz.sig"
gh release view "$TAG" >/dev/null 2>&1 || gh release create "$TAG" --draft --verify-tag --title "Talaan $TAG" --notes-file scripts/release-notes.md
gh release upload "$TAG" "$OUT/dmg/Talaan_${VERSION}_aarch64.dmg" "$TMP/Talaan_aarch64.app.tar.gz" "$TMP/Talaan_aarch64.app.tar.gz.sig" --clobber
node scripts/latest-json.mjs "$TAG"
```

  Make it executable in git: `git update-index --chmod=+x scripts/upload-mac-release.sh`.
- [ ] **Check without CI:**
  - Optional lint: if `actionlint` is installed (`winget install rhysd.actionlint`), run
    `actionlint .github/workflows/release.yml`. Expected: no errors.
  - The Windows job's build step is Task 3's signed run, and its output was checked there.
  - The upload and `latest.json` steps were dry-run in Task 6 (`--dir`).
  - On a disposable test repo or fork (never `mmnzns/Talaan` before the gate), the full `gh` round trip is:
    `gh release create vtest --draft`, upload Task 3's .exe and .sig, run
    `GITHUB_REPOSITORY=<you>/<fork> node scripts/latest-json.mjs vtest`, then
    `gh release download vtest -p latest.json -O -`. Expected: the Windows keys, with URLs on that repo.
- [ ] Commit `W5: release workflow builds Windows on CI; one latest.json for both platforms; Mac upload script`.

### Task 8: README

**Files:** `README.md`

- [ ] Replace these parts:
  - **Status:** "v1.0 in final testing. Mac (macOS 14.4 or later, Apple silicon) and Windows (Windows 10 version
    2004 or later, or Windows 11, on a 64-bit Intel or AMD PC)."
  - **What it does:** "FaceTime" becomes "FaceTime (Mac)".
  - **Privacy:** the folder sentence becomes "Meetings are plain files, readable without the app:
    `~/Library/Application Support/Talaan` on the Mac, `%APPDATA%\Talaan` on Windows." Add the row
    "Your calendar (Windows) | Downloaded from the private iCal address you paste, every 15 minutes. Nothing is
    sent."
  - **Install** (whole section):

```markdown
## Install

Download from [Releases](https://github.com/mmnzns/Talaan/releases/latest). Step by step, with screenshots of what to click: [docs/INSTALL.md](docs/INSTALL.md).

- **Windows:** run `Talaan_<version>_x64-setup.exe`. It installs for your account only, with no administrator password. Windows SmartScreen says "Windows protected your PC" the first time: click **More info**, then **Run anyway**. Talaan isn't signed with a paid certificate; it's free, and built from this source code.
- **Mac:** open the `.dmg` and drag Talaan to Applications. macOS blocks it twice (the `.dmg`, then the app): click **Done**, then **System Settings › Privacy & Security › Open Anyway**. Only once.

Then follow the setup: allow the microphone (and system audio on the Mac), pick a speech model, choose how summaries are written. Updates install from Settings › General and keep your meetings and settings.
```

  - **Models:** the first row's job becomes "Speech (recommended on the Mac)". Parakeet becomes "Speech (fastest;
    recommended on Windows, 25 European languages)".
  - **Build from source:**
    - Mac: "Requires Rust, Node 22 and the Xcode Command Line Tools."
    - Windows: "Requires Rust (`x86_64-pc-windows-msvc`), Node 22, Visual Studio Build Tools with C++, CMake and
      LLVM (`LIBCLANG_PATH` set to its `bin`)."
    - Keep `npm install` / `npm run tauri dev`.
    - Releases: "push a `v*` tag: CI builds Windows (`scripts/release-windows.ps1`) and drafts the release; build
      the Mac side with `scripts/release-local.sh`, then `scripts/upload-mac-release.sh <tag>`."
- [ ] Commit `W5: README for Mac and Windows, linking docs/INSTALL.md`.

### Task 9: Finish and commit docs/INSTALL.md

**Files:** `docs/INSTALL.md` (untracked; add it)

- [ ] **Download table:**
  - Rows: `Talaan_1.0.0_x64-setup.exe`, `Talaan_1.0.0_aarch64.dmg`, and "`latest.json`, `.sig`,
    `Talaan_aarch64.app.tar.gz`: used by the updater".
  - Add under Windows › Download: "Edge or Chrome may warn that the file isn't commonly downloaded. In Edge:
    **...** › **Keep** › **Show more** › **Keep anyway**."
- [ ] **Windows › Install:**
  - Step 2 adds "The publisher shows as Unknown publisher. That's expected."
  - Step 3 adds "If your PC doesn't have Microsoft Edge WebView2 (rare), the installer downloads it."
  - Step 4: "Leave **Run Talaan** ticked on the last page."
- [ ] **Windows › Updates:** "Click **Update now**. A small installer window shows its progress, then Talaan opens
  again by itself."
- [ ] **Calendar on Windows** step 3: "Click **Test and connect**. Talaan shows how many events it found in the
  next 7 days."
- [ ] **Removing Talaan › Step 1:** replace "Afterwards, quit Talaan from its icon (**Quit**) before uninstalling"
  with "Talaan quits by itself when it's done."
- [ ] **Removing Talaan › Windows:**
  - After step 3: "Ticking **Delete the application data** removes everything Talaan stored: your meetings, audio,
    models and settings (`%APPDATA%\Talaan`), the app's window data, saved API keys, the Notion secret and the
    calendar address, and the startup entry. Leave it unticked to keep your meetings for a later reinstall."
  - Table: add "Program files | `%LOCALAPPDATA%\Talaan` (removed by the uninstaller)". Add `calendar-ics.Talaan`
    to the keys list.
  - PowerShell block: add `"calendar-ics"` to the `cmdkey` list. Add
    `Remove-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Run -Name Talaan -ErrorAction SilentlyContinue`.
- [ ] **Features from the unmerged branches** (open question 5). If the i18n and setup check branches are not
  merged into the release, change these:
  - Troubleshooting's first paragraph ("Settings › General › Check setup", "Test call audio");
  - "Then click **Check again** in Check setup";
  - "(or click **Open Windows Settings** in Talaan)";
  - "**Open folder** and **Export all**" in "Where your meetings are saved".
  
  Either point them at what exists, or keep them and mark the merge as a gate item in TESTING.md. Default: keep
  the text, and add the gate item.
- [ ] Read it once against the installed build: every button name must match the UI.
- [ ] `git add docs/INSTALL.md`. Commit `W5: docs/INSTALL.md for the Windows and Mac release`.

### Task 10: Release gate (TESTING.md), port docs, changelog

**Files:** `docs/TESTING.md`, `docs/WINDOWS-PORT.md`, `CHANGELOG.md`

- [ ] `docs/TESTING.md`: add `## Windows release gate (W5)` after "Release prep (M10)", with this list.

```markdown
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
- [ ] Google Meet in Edge or Chrome, tab in front, Browser calls on Always: starts by itself. Meet in a
  background tab: asks instead.
- [ ] Both sides in the transcript, Me/Them right, no echo lines (headphones). Summary written offline.
- [ ] Calendar: paste the Google "Secret address in iCal format", Test and connect shows events; a recording
  during an event takes its title and attendees.

**Shell**
- [ ] Tray icon visible on light and dark taskbars; popover opens above the tray; hover shows the timer.
- [ ] "Summary ready" toast appears and opens the meeting; a click from Action Center after the banner is
  gone also opens it (if not: note it; the shortcut needs a ToastActivatorCLSID, a follow-up).
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
- [ ] In the old build: Settings › General › Updates › Check, then Update now. The installer runs by itself,
  Talaan reopens as the new version; the meeting, settings and Open at login are still there.

**Delete and uninstall**
- [ ] Settings › Privacy › Delete everything: Talaan quits; next launch is onboarding; `cmdkey /list` has no
  Talaan entries. (Mac too: it quits instead of restarting.)
- [ ] Uninstall without "Delete the application data": %APPDATA%\Talaan stays.
- [ ] Uninstall with it ticked: %APPDATA%\Talaan, the com.craftconceptsdigital.talaan folders, every
  `*.Talaan` credential and the Run / StartupApproved entries are gone; no Talaan process left.

**Before publishing**
- [ ] Version is 1.0.0 in tauri.conf.json, Cargo.toml and package.json (not -dev).
- [ ] The release includes the setup check and i18n branches, or INSTALL.md no longer mentions their buttons.
- [ ] The draft has the .exe, .exe.sig, .dmg, Talaan_aarch64.app.tar.gz, its .sig and latest.json with four
  keys (windows-x86_64, windows-x86_64-nsis, darwin-aarch64, darwin-aarch64-app).
```

- [ ] `docs/WINDOWS-PORT.md` Progress: add **W5 done**, in plain sentences:
  - the installer: per-user, embedded WebView2 bootstrapper, VC++ runtime and DirectML.dll included;
  - the uninstall data option and what it removes;
  - toasts use the shortcut's AppUserModelID (confirmed, with a test);
  - the release flow (`release-windows.ps1`, the CI job, `latest-json.mjs`, `upload-mac-release.sh`);
  - Delete everything quits.

  Replace the W3 sentence "**W5 must confirm** the NSIS shortcut carries that AppUserModelID" with "The NSIS
  shortcut carries it (W5)." In "Build and packaging", mark the release.yml and tauri.windows.conf.json items done.
- [ ] `CHANGELOG.md` 1.0.0: add "- **Windows:** Windows 10 (2004 or later) and 11 on x64: a per-user
  installer, tray icon, toasts, calendar through its iCal address, browser call tabs. One updater for both
  platforms." In Privacy, add "Delete everything quits Talaan when done."
- [ ] Update the Notion Build Log (CLAUDE.md rule 7) with the W5 entry and the clean-PC findings (DLLs). CLAUDE.md
  still says "Windows port is v1.1". It is generated from Notion, so fix it there first (open question 8).
- [ ] Commit `W5: Windows release gate in TESTING.md, port docs, changelog`.

- [ ] **🛑 Checkpoint (Miguel):** run the "Windows release gate" list on the PC. Then bump the version, push and
  tag. CI drafts the release with Windows. Run `release-local.sh` and `upload-mac-release.sh <tag>` on the Mac,
  check the draft's `latest.json`, and publish.

---

## Open questions

1. **`calendar-ics` key:**
   - On `windows-port` the iCal address is still `calendarIcsUrl` in settings.json, so no `calendar-ics`
     credential exists.
   - The plan assumes account `calendar-ics`, service `Talaan` (Credential Manager target `calendar-ics.Talaan`).
   - Confirm the name with the agent moving it. If it keeps another name, change `ACCOUNTS`; the .nsh test catches
     a mismatch.
2. **Publisher name** in the installer and in Apps › Installed apps:
   - "Miguel Monzones" (default here) or "Craft Concepts Digital"?
   - It becomes the registry key `HKCU\Software\<publisher>\Talaan`. Changing it after v1.0 means old installs
     don't find their install path on reinstall.
3. **gh and drafts:** the plan relies on `gh release view/upload/download <tag>` finding a **draft** by tag. Recent
   `gh` does this. Confirm once on a test repo (Task 7). If it fails, look the draft up with
   `gh api repos/{owner}/{repo}/releases --jq '.[] | select(.tag_name=="vX") | .id'` and use `gh api` to upload.
4. **Mac on CI:**
   - The `macos` job now runs only when the repo variable `MAC_RELEASE_ON_CI` is `true`. Today it always fails
     (missing certificate secrets).
   - OK to gate it like this, or would you rather delete the job until the .p12 is exported?
5. **Unmerged branches:**
   - INSTALL.md (and the hand checks) use Check setup, Test call audio, Open Windows Settings and working Open
     folder / Export all.
   - These live on `claude/optimistic-leavitt-332ac0` / `claude/eloquent-hugle-187866`, not on `windows-port`.
   - Will they be merged before v1.0? This plan keeps INSTALL.md as written and makes the merge a gate item.
6. **Delete everything while recording:** the plan discards the live recording first, then deletes and quits. The
   alternative is to refuse ("Stop the recording first"). That needs a UI change in PrivacyTab, which conflicts
   with the i18n branch. OK with discarding?
7. **Toast clicks from Action Center:**
   - Tauri doesn't set a `ToastActivatorCLSID` on the shortcut. If the gate shows that clicks from Action Center do
     nothing, fixing it means a COM activator.
   - Proposed as a v1.0.1 follow-up, not W5.
8. **Docs that disagree:**
   - CLAUDE.md says "Windows port is v1.1" and "No tags or GitHub Releases until the v1.0 release gate passes".
     WINDOWS-PORT.md says v1.0 ships both.
   - Update the Notion decision, then regenerate CLAUDE.md (rule 7).
9. **Models in Roaming AppData:**
   - `dirs::data_dir()` is `%APPDATA%` (Roaming), so several GB of models live in a roaming profile. That's harmless
     at home, but slow on a company PC with roaming profiles.
   - Not changed in W5 (moving data needs a migration). Revisit for v1.1?
10. **DirectML:** ONNX Runtime pulls in a DirectML build even though Talaan runs it on the CPU (+~18 MB). If the
    Sandbox check passes with System32's DirectML.dll (remove ours and retest), we could drop it. The plan ships it,
    because it is the version ONNX Runtime was built against.

### Critical files for implementation
- C:\Dev\Talaan\src-tauri\tauri.windows.conf.json
- C:\Dev\Talaan\src-tauri\windows\installer-hooks.nsh (new)
- C:\Dev\Talaan\scripts\release-windows.ps1 (new)
- C:\Dev\Talaan\scripts\latest-json.mjs (new)
- C:\Dev\Talaan\.github\workflows\release.yml
- C:\Dev\Talaan\src-tauri\src\keys.rs
- C:\Dev\Talaan\src-tauri\src\commands\settings.rs
- C:\Dev\Talaan\docs\INSTALL.md
- C:\Dev\Talaan\docs\TESTING.md

## Decisions (2026-10-15, main session, Miguel asleep: "I leave it up to you")

1. `calendar-ics`: use whatever account name the W4 fix landed in `keys.rs`; the .nsh test catches a mismatch.
2. Publisher: **Craft Concepts Digital**, the same as Parlato (identifier com.craftconceptsdigital.*).
3. gh and drafts: there is no disposable test repo; rely on the `gh api` fallback only if the first real tag shows the problem. Note it in TESTING.md's "Before publishing".
4. Mac on CI: gate it with `MAC_RELEASE_ON_CI` (keep the job).
5. Unmerged branches: they are merged into `windows-port` before W5 starts, so INSTALL.md keeps its text.
6. Delete everything while recording: discard first, then delete and quit.
7. ToastActivatorCLSID: v1.0.1 follow-up, logged on the Notion Next Version page.
8. CLAUDE.md / Notion: the main session updates Notion's decision (v1.0 ships Mac and Windows) and CLAUDE.md to match.
9. Roaming AppData for models: v1.1, logged on the Next Version page.
10. DirectML.dll: ship it.
