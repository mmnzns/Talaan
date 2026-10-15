# W0–W1. Windows build and recorder — task plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax.

> Part of the Windows port (docs/WINDOWS-PORT.md). Phases: **W0** compiles + CI · **W1** recorder · W2 call
> detection · W3 shell (permissions UI, toasts, title bar, tray, string table) · W4 calendar + browser tabs ·
> W5 packaging + release gate. W2–W5 get their own plans when we reach them.
> Model: **Opus** (native audio), per CLAUDE.md.
> Ends at a 🛑 checkpoint: Miguel records a real call on the PC and both sides are in the meeting.

**Goal:** Talaan builds, tests and runs on Windows, and records a real call as two aligned tracks (mic = Me,
everything the PC plays except Talaan = Them).

**Architecture:** All Windows code goes in `src-tauri/src/platform/windows/`, exporting the same items as
`platform/macos/`. Callers outside `platform/` stop naming `macos` and use `crate::platform::…`, so neither
platform's code mentions the other. System audio is WASAPI **process loopback in exclude mode**: every process
except Talaan's own tree, mixed by Windows. It isn't tied to an output device.

**Tech stack:** Rust 2024, Tauri 2, `windows` 0.62 (already in the tree through Tauri and cpal), `wasapi`
(process loopback client), `winreg` (registry), cpal 0.18 (mic), whisper-rs / parakeet-rs / ort (CPU in W0).

---

## Verified facts (2026-10-14, on the PC)

- PC: Windows 11 Pro (build 26300), Ryzen 7 9800X3D, RTX 5070 Ti. Toolchain: Rust `x86_64-pc-windows-msvc`,
  VS Build Tools 2026, CMake, LLVM (`LIBCLANG_PATH` set), Node, gh. **No Vulkan SDK** (GPU builds come later).
- Repo cloned to `C:\Dev\Talaan` (outside Google Drive so build output doesn't sync). `npm install` is clean.
- **Baseline `cargo check --workspace --tests`:** whisper-rs, parakeet-rs (ort), keyring and Tauri all compile
  on Windows unchanged (CPU). Exactly 3 errors:
  1. `commands/calendar.rs:5` uses `platform::macos::calendar`
  2. `commands/ask.rs:45` uses `platform::macos::thread`
  3. `summarizer/local.rs:163` (test) uses `std::os::unix::fs::PermissionsExt`
- Tauri's build script also needs `src-tauri/binaries/talaan-llm-x86_64-pc-windows-msvc.exe` (externalBin). A
  placeholder was created locally to get past it; Task 2 builds the real one. `binaries/` is gitignored.
- **Clock:** cpal 0.18's WASAPI backend stamps input buffers with the `GetBuffer` QPC position (100 ns units)
  × 100 → ns (`cpal-0.18.2/src/host/wasapi/stream.rs` `input_timestamp`). So Talaan's Windows `host_now_ns`
  must be QPC on the same 100 ns grid, and the loopback source must stamp the same way. Then both tracks share
  one clock, as on the Mac.
- cpal 0.18 already follows the default input device for default-device streams (`_default_device_monitor`).
  Talaan's own watcher also restarts the mic on change. That's harmless (one extra restart and the "Switched to"
  notice), but Task 12 checks it.

## Platform contract (what every `platform/<os>/` exports)

| Item | Signature | W0 Windows | Real in |
|---|---|---|---|
| `NAME` | `&str` | `"windows"` | W0 |
| `host_now_ns` | `fn() -> u64` | QPC | W0 (Task 5) |
| `background_priority` | `fn()` | `THREAD_PRIORITY_BELOW_NORMAL` | W0 (Task 5) |
| `system_tap::SystemTap` | `Default + AudioSource` | stub `Err` | W1 (Task 9) |
| `devices::{on_builtin_speakers, others_playing, DeviceEvent, DeviceWatcher}` | as macOS | stub | W1 (Tasks 10–11) |
| `power::SleepWatcher::new(impl Fn(bool)) -> Self` | as macOS | stub | W1 (Task 12) |
| `permissions::{mic_status, request_mic, settings_url, open_settings}` | as macOS | real | W1 (Task 8) |
| `mic_users::{MicUser, mic_users}` | as macOS | stub `vec![]` | W2 |
| `notify::{setup, post}` | as macOS | stub no-op | W3 |
| `calendar::{Access, access, request, events}` | as macOS | stub `NotAsked` / `[]` | W4 |
| `browser_tabs::tab_urls(&str, Family) -> Tabs` | as macOS | stub `Unknown` | W4 |

## File map

- Create `src-tauri/src/platform/windows/` with `mod.rs clock.rs thread.rs system_tap.rs devices.rs power.rs
  permissions.rs mic_users.rs notify.rs calendar.rs browser_tabs.rs com.rs`
- Modify `src-tauri/src/platform/mod.rs`: the Windows `cfg` line, no more "unsupported" fallback
- Modify (macos → neutral paths): `lib.rs`, `commands/{ask,calendar,calls,recorder,summarizer}.rs`, `calendar/mod.rs` (doc)
- Modify `summarizer/local.rs`: `.exe` suffix, portable crash test
- Modify `src-tauri/Cargo.toml`, `src-tauri/llm/Cargo.toml`: Windows target deps
- Create `scripts/build-llm-helper.ps1`, `src-tauri/tauri.windows.conf.json`
- Modify `.github/workflows/ci.yml`: `windows` job
- Modify `docs/WINDOWS-PORT.md`, `CLAUDE.md`: Windows commands and progress

Rules: commit after every task that builds and passes (`W0:` / `W1:` prefix). **Mac code must still compile.**
The macOS CI job is the check, so push only when Miguel says (CLAUDE.md "Ask before pushing").

---

## W0 — compiles, tests, CI

### Task 1: Windows target dependencies

**Files:** Modify `src-tauri/Cargo.toml`, `src-tauri/llm/Cargo.toml`

- [ ] **Step 1: Add the Windows section to `src-tauri/Cargo.toml`** (after the macOS section)

```toml
[target.'cfg(target_os = "windows")'.dependencies]
keyring = { version = "3.6.3", features = ["windows-native"] }
# CPU for now; GPU (vulkan) after the W1 checkpoint, see "Later" in docs/plans/W0-W1-windows-build-and-recorder.md.
whisper-rs = "0.16.0"
wasapi = "0.22"
winreg = "0.55"
windows = { version = "0.62", features = [
  "Win32_Foundation",
  "Win32_Media_Audio",
  "Win32_Media_Audio_Endpoints",
  "Win32_Devices_FunctionDiscovery",
  "Win32_System_Com",
  "Win32_System_Com_StructuredStorage",
  "Win32_System_Variant",
  "Win32_System_Performance",
  "Win32_System_Power",
  "Win32_System_Threading",
  "Win32_UI_Shell_PropertiesSystem",
  "Win32_UI_WindowsAndMessaging",
] }
windows-core = "0.62"
```

Use the newest `wasapi` / `winreg` cargo resolves. If the `wasapi` release you get pins a different `windows`
major, that's fine (two copies of `windows` compile side by side). Verify both licenses (expected MIT) and add
them to `THIRD_PARTY_NOTICES.md` in Task 9.

- [ ] **Step 2: `src-tauri/llm/Cargo.toml`.** No change needed: the base entry is CPU (`features = ["common"]`),
  which is what Windows uses until GPU. Add a comment line above the macOS section:

```toml
# Windows uses the CPU build above until the Vulkan bake-off (W0-W1 plan, "Later").
```

- [ ] **Step 3: Verify** `cargo check --manifest-path src-tauri/Cargo.toml` still shows exactly the 3 known errors (no new ones).
- [ ] **Step 4: Commit** `W0: Windows target dependencies (CPU whisper, wasapi, winreg, windows)`

### Task 2: Build the helper on Windows

**Files:** Create `scripts/build-llm-helper.ps1`, `src-tauri/tauri.windows.conf.json`

- [ ] **Step 1: Write `scripts/build-llm-helper.ps1`**

```powershell
# Builds talaan-llm (the local AI helper, its own process so its llama.cpp never meets Whisper's ggml) and puts
# it where Tauri's externalBin expects it: src-tauri/binaries/talaan-llm-<target triple>.exe. Always optimized:
# llama.cpp is unusably slow in a debug build. Also copied next to the dev binaries so `tauri dev` finds it.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..\src-tauri')
$triple = ((rustc -vV) | Where-Object { $_ -like 'host: *' }).Substring(6)
cargo build --release -p talaan-llm
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
New-Item -ItemType Directory -Force binaries, target\debug | Out-Null
Copy-Item target\release\talaan-llm.exe "binaries\talaan-llm-$triple.exe" -Force
Copy-Item target\release\talaan-llm.exe target\debug\talaan-llm.exe -Force
```

- [ ] **Step 2: Write `src-tauri/tauri.windows.conf.json`** (Tauri merges it over `tauri.conf.json` on Windows)

```json
{
  "build": {
    "beforeDevCommand": "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-llm-helper.ps1 && npm run dev",
    "beforeBuildCommand": "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-llm-helper.ps1 && npm run build"
  },
  "bundle": {
    "targets": ["nsis"]
  }
}
```

- [ ] **Step 3: Run** `powershell -File scripts/build-llm-helper.ps1`. Expected: `src-tauri\binaries\talaan-llm-x86_64-pc-windows-msvc.exe`
  exists and is several MB (it replaces the empty placeholder). First build takes a while (llama.cpp).
- [ ] **Step 4: Commit** `W0: build the local AI helper on Windows (ps1, tauri.windows.conf.json)`

### Task 3: Helper path and the portable crash test

**Files:** Modify `src-tauri/src/summarizer/local.rs:12-18,156-168`

- [ ] **Step 1: The helper path gets the platform's executable suffix**

```rust
pub const HELPER: &str = "talaan-llm";

/// Where the helper lives: next to Talaan's own binary (in the app bundle, `Contents/MacOS/`; next to
/// `Talaan.exe` on Windows; in `target/<profile>/` during development).
pub fn helper_path() -> PathBuf {
    let name = format!("{HELPER}{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(&name))).unwrap_or_else(|| PathBuf::from(name))
}
```

- [ ] **Step 2: Make the crash test portable.** Replace the body of `a_crashing_helper_is_reported_not_fatal`
  with a fake helper per OS:

```rust
    /// A helper that dies mid-request (out of memory, crash) becomes a readable error.
    #[test]
    fn a_crashing_helper_is_reported_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        // Answers the ping, then exits on the first real request.
        #[cfg(unix)]
        let fake = {
            let f = dir.path().join("fake-llm");
            std::fs::write(&f, "#!/bin/sh\nread line\necho '{\"event\":\"ready\"}'\nread line\nexit 1\n").unwrap();
            std::fs::set_permissions(&f, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
            f
        };
        #[cfg(windows)]
        let fake = {
            let f = dir.path().join("fake-llm.cmd");
            std::fs::write(&f, "@echo off\r\nset /p line=\r\necho {\"event\":\"ready\"}\r\nset /p line=\r\nexit /b 1\r\n").unwrap();
            f
        };
        let mut ai = LocalAi::new(&fake);
        let c = Completion { model_path: "x".into(), n_ctx: 512, system: "".into(), user: "".into(), json_schema: None, max_tokens: 1, temperature: 0.0 };
        let e = ai.complete(&c, &AtomicBool::new(false), &mut |_, _, _| {}).unwrap_err();
        assert!(e.contains("stopped unexpectedly"), "{e}");
    }
```

  The smoke tests that build the helper path by hand (`local.rs` `local_ai_smoke` and the bake-off,
  `summarize.rs:229`, `ask/mod.rs:26`, `commands/ask.rs:262`) change `.join(HELPER)` to
  `.join(format!("{HELPER}{}", std::env::consts::EXE_SUFFIX))` (with `crate::summarizer::local::HELPER` where
  that's the import).

- [ ] **Step 3: Verify** (after Task 4 compiles): `cargo test --manifest-path src-tauri/Cargo.toml summarizer::local` → both tests pass.
- [ ] **Step 4: Commit** with Task 4 (it can't compile alone).

### Task 4: Platform-neutral callers + Windows stubs

**Files:** Modify `platform/mod.rs`, `lib.rs`, `commands/{ask,calendar,calls,recorder,summarizer}.rs`,
`calendar/mod.rs`. Create every file in `platform/windows/`.

- [ ] **Step 1: `platform/mod.rs`**

```rust
//! OS-specific code. Each platform gets its own folder and exposes the same items (see the contract table in
//! docs/plans/W0-W1-windows-build-and-recorder.md), so callers only ever say `crate::platform::…`.

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("Talaan runs on macOS and Windows");

#[cfg(test)]
mod tests {
    #[test]
    fn reports_a_platform_name() {
        assert!(!super::NAME.is_empty());
    }
}
```

`macos/mod.rs` already exports every module publicly, so `crate::platform::calendar` etc. resolve on the Mac.

- [ ] **Step 2: `platform/windows/mod.rs`**

```rust
//! Windows-only code: audio capture, permissions and call detection. Mirrors `platform/macos/` item for item.

pub mod browser_tabs;
pub mod calendar;
pub mod clock;
pub mod com;
pub mod devices;
pub mod mic_users;
pub mod notify;
pub mod permissions;
pub mod power;
pub mod system_tap;
pub mod thread;

pub use clock::host_now_ns;
pub use thread::background_priority;

pub const NAME: &str = "windows";
```

- [ ] **Step 3: The stubs** (each replaced by a real version in a later task or phase)

`platform/windows/com.rs`
```rust
//! COM on the calling thread. Every Windows audio call needs it; initializing twice is fine.

use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

/// Joins the multithreaded apartment (idempotent; an already-initialized thread keeps its apartment).
pub fn init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}
```

`platform/windows/mic_users.rs` (W2)
```rust
//! Which apps are using the microphone right now. Written in W2 (CapabilityAccessManager consent store).

/// A process capturing audio input. On Windows `bundle_id` holds the exe name or the packaged app's family name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicUser {
    pub pid: i32,
    pub bundle_id: String,
    pub path: String,
}

pub fn mic_users() -> Vec<MicUser> {
    vec![]
}
```

`platform/windows/notify.rs` (W3)
```rust
//! "Summary ready" notifications. Written in W3 (Windows toasts).

pub fn setup(_on_click: impl Fn(String) + Send + Sync + 'static) {}

pub fn post(_id: &str, _title: &str, _body: &str) {}
```

`platform/windows/calendar.rs` (W4)
```rust
//! The system calendar. Written in W4 (spike: WinRT AppointmentStore, else Outlook).

use crate::calendar::Event;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Allowed,
    NotAsked,
    Denied,
}

pub fn access() -> Access {
    Access::NotAsked
}

pub fn request() -> Access {
    Access::Denied
}

pub fn events(_from_ms: i64, _to_ms: i64) -> Vec<Event> {
    vec![]
}
```

`platform/windows/browser_tabs.rs` (W4)
```rust
//! A browser's call tab. Written in W4 (UI Automation on the address bar; the active tab only).

use crate::calls::tabs::{Family, Tabs};

pub fn tab_urls(_app: &str, _family: Family) -> Tabs {
    Tabs::Unknown
}
```

`system_tap.rs`, `devices.rs`, `power.rs` start as stubs too, and Tasks 9–12 replace them:

`platform/windows/system_tap.rs`
```rust
//! System audio ("them"). Written in W1 Task 9 (WASAPI process loopback).

use crate::recorder::source::{AudioSource, SharedFeed};

#[derive(Default)]
pub struct SystemTap;

impl AudioSource for SystemTap {
    fn start(&mut self, _feed: SharedFeed) -> Result<String, String> {
        Err("System audio isn't available yet on Windows".into())
    }
    fn restart(&mut self) -> Result<String, String> {
        Err("System audio isn't available yet on Windows".into())
    }
    fn stop(&mut self) {}
}
```

`platform/windows/devices.rs`
```rust
//! Default device changes and the built-in speakers check. Written in W1 Tasks 10–11.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    InputChanged,
    OutputChanged,
}

pub fn on_builtin_speakers() -> bool {
    false
}

pub fn others_playing() -> bool {
    false
}

pub struct DeviceWatcher;

impl DeviceWatcher {
    pub fn new(_on_change: impl Fn(DeviceEvent) + Send + Sync + 'static) -> Result<Self, String> {
        Ok(DeviceWatcher)
    }
}
```

`platform/windows/power.rs`
```rust
//! Sleep and wake. Written in W1 Task 12.

pub struct SleepWatcher;

impl SleepWatcher {
    pub fn new(_on_change: impl Fn(bool) + Send + Sync + 'static) -> Self {
        SleepWatcher
    }
}
```

`platform/windows/permissions.rs`, `clock.rs`, `thread.rs`: write the real versions now (Tasks 5 and 8 show the
code and tests). To keep this task small, start them as:

```rust
// permissions.rs
pub fn mic_status() -> &'static str { "granted" }
pub fn request_mic() -> bool { true }
pub fn settings_url(_pane: &str) -> Option<&'static str> { None }
pub fn open_settings(_pane: &str) -> Result<(), String> { Err("Not on this platform yet".into()) }
```
```rust
// clock.rs
pub fn host_now_ns() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}
```
```rust
// thread.rs
pub fn background_priority() {}
```

- [ ] **Step 4: Callers use `crate::platform::…`.** Exact edits:

  - `commands/ask.rs:45` → `crate::platform::background_priority();`
  - `commands/calendar.rs:5` → `use crate::platform::calendar::{self as system, Access};`
  - `calendar/mod.rs:2` doc → `` `platform::<os>::calendar` ``
  - `commands/summarizer.rs:109-114` →
    ```rust
        if wanted {
            crate::platform::notify::post(id, "Summary ready", title);
        }
    ```
  - `lib.rs:42-46` →
    ```rust
            {
                let handle = app.handle().clone();
                platform::notify::setup(move |id| commands::summarizer::open_meeting(&handle, &id));
            }
    ```
    (keep the `#[cfg(target_os = "macos")]` on `RunEvent::Reopen`: it's a macOS-only Tauri event.)
  - `commands/calls.rs`: drop `#[cfg(target_os = "macos")]` on `resolve_browsers` (line 77) and on the block in
    `setup` (line 186). Replace `crate::platform::macos::mic_users::MicUser` (×2) with
    `crate::platform::mic_users::MicUser`, `crate::platform::macos::mic_users::mic_users()` with
    `crate::platform::mic_users::mic_users()`, and `crate::platform::macos::browser_tabs::tab_urls` with
    `crate::platform::browser_tabs::tab_urls`.
  - `commands/recorder.rs`:
    - field: `watchers: Mutex<Option<(platform::devices::DeviceWatcher, platform::power::SleepWatcher)>>,` (no cfg)
    - `speakers()` → `platform::devices::on_builtin_speakers()` (no cfgs)
    - `system_source()` → `Box::new(platform::system_tap::SystemTap::default())` (no cfgs)
    - `permissions_status` → `PermissionStatus { microphone: platform::permissions::mic_status() }`
    - `permissions_request_mic` → `tauri::async_runtime::spawn_blocking(platform::permissions::request_mic).await.unwrap_or(false)`
    - `permissions_request_system_audio`: drop the cfgs; the body uses `platform::system_tap::SystemTap::default()`.
      On Windows there's no prompt, so it just proves loopback opens.
    - `permissions_open_settings` → `platform::permissions::open_settings(&pane).map_err(err)`
    - `setup`: drop the cfg on the watchers block; `use platform::{devices::DeviceEvent, devices::DeviceWatcher, power::SleepWatcher};`
    - **Keep** `#[cfg(target_os = "macos")]` on the "no call audio" silence watch (lines 353-365): its notice
      points at a macOS privacy pane, and Windows loopback can't be blocked by a permission. Silence the unused
      variable warnings it leaves on Windows with `#[cfg_attr(not(target_os = "macos"), allow(unused_variables, unused_assignments, unused_mut))]`
      on the `std::thread::spawn` closure's enclosing `let app = app.clone();` block. Simplest form: put the three
      `let mut` lines and the `heard_them |=` line inside `#[cfg(target_os = "macos")]` too.

- [ ] **Step 5: Verify:** `cargo check --manifest-path src-tauri/Cargo.toml --workspace --tests` → **0 errors** and no
  warnings from the edited files. `grep -rn "platform::macos" src-tauri/src --include=*.rs | grep -v "src/platform/"`
  → only the silence-watch line in `commands/recorder.rs`.
- [ ] **Step 6: Commit** `W0: callers use crate::platform, Windows platform stubs, portable helper path`

### Task 5: Clock and thread priority (real)

**Files:** `platform/windows/clock.rs`, `platform/windows/thread.rs`

- [ ] **Step 1: Failing tests** in `clock.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qpc_counts_become_nanoseconds_on_the_100ns_grid() {
        assert_eq!(qpc_to_ns(10_000_000, 10_000_000), 1_000_000_000);
        assert_eq!(qpc_to_ns(3, 10_000_000), 300); // 3 ticks of 100 ns
        assert_eq!(qpc_to_ns(1, 3_000_000), 300); // 333 ns floors to 300, like cpal's now()
        assert_eq!(qpc_to_ns(5, 0), 0); // never divides by zero
    }

    #[test]
    fn wasapi_positions_are_100ns_units() {
        assert_eq!(hns_to_ns(5), 500);
    }

    #[test]
    fn host_time_moves_forward_in_nanoseconds() {
        let a = host_now_ns();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let d = host_now_ns() - a;
        assert!((15_000_000..200_000_000).contains(&d), "{d}");
    }
}
```

- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml platform::windows::clock` → fails (`qpc_to_ns` not found).
- [ ] **Step 3: Implement**

```rust
//! Host time in nanoseconds: QueryPerformanceCounter on the 100 ns grid WASAPI uses for its QPC positions,
//! so cpal's mic timestamps and the loopback timestamps are on the same clock as `host_now_ns`.

use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

pub fn host_now_ns() -> u64 {
    let (mut counter, mut freq) = (0i64, 0i64);
    unsafe {
        let _ = QueryPerformanceCounter(&mut counter);
        let _ = QueryPerformanceFrequency(&mut freq);
    }
    qpc_to_ns(counter as u64, freq as u64)
}

/// A QPC reading → ns, through 100 ns units (matches cpal's `now()` and WASAPI positions exactly).
pub fn qpc_to_ns(counter: u64, freq: u64) -> u64 {
    if freq == 0 {
        return 0;
    }
    (counter as u128 * 10_000_000 / freq as u128 * 100) as u64
}

/// A WASAPI QPC position (100 ns units) → ns.
pub fn hns_to_ns(hns: u64) -> u64 {
    hns.saturating_mul(100)
}
```

- [ ] **Step 4: `thread.rs`**

```rust
//! Thread priority for long background work (transcription, indexing), so a call being recorded never stutters.
//! Below normal, not THREAD_MODE_BACKGROUND_BEGIN: background mode also throttles disk reads, which would make
//! loading a 1.5 GB model crawl.

use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};

pub fn background_priority() {
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn setting_the_priority_does_not_crash() {
        std::thread::spawn(super::background_priority).join().unwrap();
    }
}
```

- [ ] **Step 5: Run** the clock and thread tests → pass.
- [ ] **Step 6: Commit** `W0: QPC host clock and below-normal background priority`

### Task 6: Full test suite on Windows

- [ ] **Step 1: Run** `npm test`, `npm run build`, `cargo test --manifest-path src-tauri/Cargo.toml --workspace`.
- [ ] **Step 2: Triage every failure** with these rules, nothing else:
  - The test asserts a macOS path or tool (e.g. `commands/mod.rs:73` expects `/Users/a/Library/…`; `importer.rs`
    already skips when `afconvert` is missing) → gate the assertion with `#[cfg(target_os = "macos")]` and add a
    Windows twin when the logic is path-shaped (e.g. `resolve_data_dir(None, Some(r"C:\Users\a\AppData\Roaming".into()))`
    → `C:\Users\a\AppData\Roaming\Talaan`).
  - Path separator or `\r\n` differences in app code (not the test) → fix the code with `Path::join` /
    `.lines()`, never by loosening the test.
  - Anything else is a real bug → `superpowers:systematic-debugging`, then fix.
- [ ] **Step 3:** All three commands green. Note the counts (Rust and TS) for the Build Log.
- [ ] **Step 4: Commit** `W0: test suite green on Windows`

### Task 7: CI job + first launch

**Files:** `.github/workflows/ci.yml`

- [ ] **Step 1: Add the job**

```yaml
  windows:
    runs-on: windows-latest
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
      - run: npm ci
      - run: npm test
      - run: npm run build
      - run: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-llm-helper.ps1
      - run: cargo test --manifest-path src-tauri/Cargo.toml --workspace
```

  (The runner image ships LLVM and CMake; if whisper-rs's bindgen can't find libclang there, add
  `env: LIBCLANG_PATH: C:\Program Files\LLVM\bin` to the job.)

- [ ] **Step 2: First launch.** `npm run tauri dev`. Expected: the helper builds, the main window opens with real
  (empty) storage under `%APPDATA%\Talaan`, the tray icon appears, and Settings opens. Note (don't fix, those are
  W3): the title bar and its 84 px traffic-light inset, "menu bar" wording, tray popover position.
- [ ] **Step 3: Runtime check of the speech stack.** In Settings › Speech models, download Whisper Large Turbo
  (and Parakeet). Then run the existing smoke on the repo fixture:

```powershell
$env:TALAAN_SMOKE_MEETING = "C:\Dev\Talaan\fixtures\three-speakers"
cargo test --release --manifest-path src-tauri/Cargo.toml pipeline_smoke -- --ignored --nocapture
```

  Expected: a transcript with Me/Them lines (ONNX Runtime loads for Parakeet/diarization; whisper.cpp runs on
  CPU). Record the real-time factor for the Build Log. If ORT fails to load at runtime, that's a W0 blocker:
  check how parakeet-rs pulls ORT (`download-binaries` vs `load-dynamic`) before anything else.
- [ ] **Step 4: Commit** `W0: Windows CI job` (CI runs when Miguel approves the push).

---

## W1 — recorder

### Task 8: Microphone permission and settings links

**Files:** `platform/windows/permissions.rs`

Windows has no runtime prompt for desktop apps. Access is three switches in Settings › Privacy & security ›
Microphone: device-wide (HKLM), "Let apps access" (HKCU) and "Let desktop apps access" (HKCU `NonPackaged`).
Talaan is an unpackaged desktop app, so it needs all three on. Each is a `Value` string, `Allow` or `Deny`.

- [ ] **Step 1: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_switch_off_means_denied() {
        assert_eq!(status_from(Some("Allow"), Some("Allow"), Some("Allow")), "granted");
        assert_eq!(status_from(Some("Deny"), Some("Allow"), Some("Allow")), "denied");
        assert_eq!(status_from(Some("Allow"), Some("Deny"), Some("Allow")), "denied");
        assert_eq!(status_from(Some("Allow"), Some("Allow"), Some("Deny")), "denied");
    }

    #[test]
    fn missing_keys_are_the_windows_default_allow() {
        assert_eq!(status_from(None, None, None), "granted");
    }

    #[test]
    fn panes_open_windows_settings() {
        assert_eq!(settings_url("microphone"), Some("ms-settings:privacy-microphone"));
        assert_eq!(settings_url("system-audio"), Some("ms-settings:sound"));
        assert_eq!(settings_url("calendar"), Some("ms-settings:privacy-calendar"));
        assert_eq!(settings_url("nope"), None);
    }

    #[test]
    fn reading_the_real_registry_does_not_fail() {
        assert!(["granted", "denied"].contains(&mic_status()));
    }
}
```

- [ ] **Step 2: Run** → fails.
- [ ] **Step 3: Implement**

```rust
//! Microphone access (the three Privacy & security switches) and links to the right Settings pages.
//! Windows never prompts a desktop app, so "request" just reports what the switches say.

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

const STORE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

fn value(root: RegKey, path: &str) -> Option<String> {
    root.open_subkey(path).ok()?.get_value::<String, _>("Value").ok()
}

/// Device-wide, all apps, desktop apps. A missing key means Windows' default: allowed.
pub fn status_from(device: Option<&str>, apps: Option<&str>, desktop: Option<&str>) -> &'static str {
    if [device, apps, desktop].iter().any(|v| v.is_some_and(|v| v.eq_ignore_ascii_case("Deny"))) { "denied" } else { "granted" }
}

pub fn mic_status() -> &'static str {
    let device = value(RegKey::predef(HKEY_LOCAL_MACHINE), STORE);
    let apps = value(RegKey::predef(HKEY_CURRENT_USER), STORE);
    let desktop = value(RegKey::predef(HKEY_CURRENT_USER), &format!(r"{STORE}\NonPackaged"));
    status_from(device.as_deref(), apps.as_deref(), desktop.as_deref())
}

pub fn request_mic() -> bool {
    mic_status() == "granted"
}

/// Settings pages. System audio needs no permission on Windows; its link opens Sound (output device).
pub fn settings_url(pane: &str) -> Option<&'static str> {
    match pane {
        "microphone" => Some("ms-settings:privacy-microphone"),
        "system-audio" => Some("ms-settings:sound"),
        "calendar" => Some("ms-settings:privacy-calendar"),
        _ => None,
    }
}

pub fn open_settings(pane: &str) -> Result<(), String> {
    let url = settings_url(pane).ok_or("Unknown settings pane")?;
    std::process::Command::new("explorer").arg(url).spawn().map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] **Step 4: Run** → pass. **Manual:** turn off "Let desktop apps access your microphone", then call
  `permissions_status` from the app (onboarding mic step). It reads denied, and the button opens the right page.
- [ ] **Step 5: Commit** `W1: Windows microphone access and Settings links`

### Task 9: System audio through process loopback

**Files:** `platform/windows/system_tap.rs`, `THIRD_PARTY_NOTICES.md`

Design (binding):
- `wasapi::AudioClient::new_application_loopback_client(own_pid, include_tree = false)` gives
  `PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE`: everything except Talaan and its children. The
  `talaan-llm` helper makes no sound, so nothing is lost. Requires Windows 10 2004 (build 19041); older builds
  fail activation, and Talaan says so.
- Process loopback has no mix format of its own. Talaan asks for **48 kHz, stereo, f32** with autoconvert, so
  the callback pushes f32 straight into the feed, and the recorder's resampler takes it to 16 kHz mono as on the
  Mac.
- One capture thread per start: joins MTA (`com::init`), raises itself to MMCSS "Pro Audio", waits on the
  event (200 ms timeout so stop is noticed), drains every packet, stamps it with `hns_to_ns(qpc_position)`, and
  pushes with `feed.try_lock()` (never blocks, like the Mac IOProc).
- Silent packets (`AUDCLNT_BUFFERFLAGS_SILENT`) push zeros of the right length, so `heard_them` stays honest.
- Not tied to an output device: `restart()` = stop + start, and the returned name is the current default
  output's friendly name (for the "Sound now plays through X" notice).

- [ ] **Step 1: Pure helper first** (failing test). Packet flags → what to push:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_packets_become_zeros_of_the_same_length() {
        let mut out = vec![];
        packet_samples(&[0.5, -0.5, 0.25, 0.0], true, &mut out);
        assert_eq!(out, vec![0.0; 4]);
        packet_samples(&[0.5, -0.5], false, &mut out);
        assert_eq!(out, vec![0.5, -0.5]);
    }
}
```

```rust
/// The samples to push for one packet: as captured, or zeros when Windows flagged it silent.
pub fn packet_samples(data: &[f32], silent: bool, out: &mut Vec<f32>) {
    out.clear();
    if silent {
        out.resize(data.len(), 0.0);
    } else {
        out.extend_from_slice(data);
    }
}
```

- [ ] **Step 2: Run** → fail, then implement → pass.
- [ ] **Step 3: The source.** Before writing, open the `wasapi` crate's docs/examples for the version cargo
  resolved (`cargo doc -p wasapi --open`, and its `examples/record_application.rs`). The names below match the
  crate's API as of 0.2x. Adjust names, not the design, if a release renamed something.

```rust
//! System audio ("them") through WASAPI process loopback: everything the PC plays except Talaan's own process
//! tree, mixed by Windows (Windows 10 2004+). Not tied to an output device, so an output switch only needs the
//! new device's name.

use super::clock::hns_to_ns;
use super::com;
use crate::recorder::source::{AudioSource, SharedFeed};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

const RATE: u32 = 48_000;
const CHANNELS: u16 = 2;

struct Live {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

#[derive(Default)]
pub struct SystemTap {
    feed: Option<SharedFeed>,
    live: Option<Live>,
}

/// The default output's name, for the UI ("Speakers (Realtek(R) Audio)").
fn output_name() -> String {
    com::init();
    wasapi::get_default_device(&wasapi::Direction::Render).and_then(|d| d.get_friendlyname()).unwrap_or_else(|_| "System audio".into())
}

fn capture(feed: SharedFeed, stop: Arc<AtomicBool>, ready: mpsc::Sender<Result<(), String>>) {
    com::init();
    let setup = || -> Result<_, Box<dyn std::error::Error>> {
        let mut client = wasapi::AudioClient::new_application_loopback_client(std::process::id(), false)?;
        let format = wasapi::WaveFormat::new(32, 32, &wasapi::SampleType::Float, RATE as usize, CHANNELS as usize, None);
        let mode = wasapi::StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 200_000 };
        client.initialize_client(&format, &wasapi::Direction::Capture, &mode)?;
        let event = client.set_get_eventhandle()?;
        let capture = client.get_audiocaptureclient()?;
        client.start_stream()?;
        Ok((client, event, capture))
    };
    let (client, event, capture) = match setup() {
        Ok(v) => v,
        Err(e) => {
            let _ = ready.send(Err(format!("Couldn't start system audio capture: {e}")));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    let _mmcss = wasapi::ThreadPriority::new_proaudio(); // "Pro Audio" scheduling; dropped when the thread ends
    let mut bytes: Vec<u8> = vec![];
    let mut samples: Vec<f32> = Vec::with_capacity(48_000);
    while !stop.load(Ordering::Relaxed) {
        if event.wait_for_event(200).is_err() {
            continue;
        }
        loop {
            let frames = match capture.get_next_packet_size() {
                Ok(Some(n)) if n > 0 => n,
                _ => break,
            };
            bytes.resize(frames as usize * CHANNELS as usize * 4, 0);
            let Ok((read, info)) = capture.read_from_device(&mut bytes) else { break };
            let data: &[f32] = bytemuck_f32(&bytes[..read as usize * CHANNELS as usize * 4]);
            packet_samples(data, info.flags.silent, &mut samples);
            if let Ok(mut f) = feed.try_lock() {
                f.push(hns_to_ns(info.timestamp), RATE, CHANNELS, &samples);
            }
        }
    }
    let _ = client.stop_stream();
}

/// The capture buffer is f32 little-endian (we asked for it); view it as samples without copying.
fn bytemuck_f32(bytes: &[u8]) -> &[f32] {
    let (head, body, _) = unsafe { bytes.align_to::<f32>() };
    if head.is_empty() { body } else { &[] }
}

fn open(feed: &SharedFeed) -> Result<(Live, String), String> {
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let (f, s) = (feed.clone(), stop.clone());
    let thread = std::thread::Builder::new().name("talaan-loopback".into()).spawn(move || capture(f, s, tx)).map_err(|e| e.to_string())?;
    match rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(Ok(())) => Ok((Live { stop, thread }, output_name())),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            stop.store(true, Ordering::Relaxed);
            Err("System audio capture didn't start in time".into())
        }
    }
}

fn close(live: Live) {
    live.stop.store(true, Ordering::Relaxed);
    let _ = live.thread.join();
}

impl AudioSource for SystemTap {
    fn start(&mut self, feed: SharedFeed) -> Result<String, String> {
        self.feed = Some(feed.clone());
        let (live, name) = open(&feed)?;
        self.live = Some(live);
        Ok(name)
    }

    fn restart(&mut self) -> Result<String, String> {
        if let Some(live) = self.live.take() {
            close(live);
        }
        let feed = self.feed.clone().ok_or("System audio not started")?;
        let (live, name) = open(&feed)?;
        self.live = Some(live);
        Ok(name)
    }

    fn stop(&mut self) {
        if let Some(live) = self.live.take() {
            close(live);
        }
    }
}

impl Drop for SystemTap {
    fn drop(&mut self) {
        self.stop();
    }
}
```

  `bytes` is a `Vec<u8>` (alignment 1), so `align_to` can return a non-empty head. If that ever happens (check in
  the smoke test with an `eprintln!` on an empty slice), switch `bytes` to a `Vec<f32>` buffer and pass it as
  bytes through `bytemuck::cast_slice_mut` (add `bytemuck`).

- [ ] **Step 4: Smoke test** (ignored, run by hand). Add it to the bottom of `system_tap.rs`. It's the same shape
  as the Mac one, using the real engine:

```rust
    /// Records 5 s of system audio plus mic. Play something (a YouTube clip) while it runs.
    /// `cargo test system_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn system_smoke() {
        use crate::platform::host_now_ns;
        use crate::recorder::engine::Engine;
        use crate::recorder::mic::MicSource;
        use crate::storage::Storage;
        let dir = std::env::temp_dir().join("talaan-system-smoke");
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new(dir).unwrap();
        let mut e = Engine::default();
        let st = e
            .start(&storage, "other", "Call", Box::new(MicSource::default()), Box::new(SystemTap::default()), host_now_ns(), chrono::Local::now().fixed_offset())
            .unwrap();
        println!("mic: {} · system audio: {}", st.mic_name, st.system_audio);
        let mut peak = (0.0f32, 0.0f32);
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let (a, b) = e.levels();
            peak = (peak.0.max(a), peak.1.max(b));
        }
        let id = e.stop(&storage, host_now_ns()).unwrap();
        println!("wrote {} · peak me {:.2} · peak them {:.2}", storage.dir(&id).unwrap().display(), peak.0, peak.1);
    }
```

  Run with a YouTube clip playing. Expected: `system audio: Speakers (…)`, peak them > 0.05, and `them.wav` in
  the printed folder has the clip. Then **the exclusion check:** play a sound *from Talaan* (Play on an old
  meeting) while recording. It must not appear in `them.wav`.
- [ ] **Step 5: Notices.** Add `wasapi`, `winreg` and `windows` (MIT/Apache-2.0, verify) to `THIRD_PARTY_NOTICES.md`.
- [ ] **Step 6: Commit** `W1: system audio through WASAPI process loopback (excludes Talaan)`

### Task 10: Built-in speakers check

**Files:** `platform/windows/devices.rs`

Echo warning, same meaning as on the Mac: is call audio coming out of speakers the mic can hear? On Windows: the
default output's form factor is `Speakers` (`PKEY_AudioEndpoint_FormFactor` = 1) and it isn't USB or Bluetooth
(`PKEY_Device_EnumeratorName`). HDMI/DisplayPort monitors report form factor `DigitalAudioDisplayDevice` (9),
headphones 3, headsets 5, so they never match.

- [ ] **Step 1: Failing test** for the pure rule

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_wired_speakers_count() {
        assert!(builtin_speakers(1, "HDAUDIO")); // laptop speakers, or desktop speakers on the jack
        assert!(builtin_speakers(1, "INTELAUDIO"));
        assert!(!builtin_speakers(1, "USB")); // USB speakers / dock: still a risk, but not "this computer's"
        assert!(!builtin_speakers(1, "BTHENUM"));
        assert!(!builtin_speakers(3, "HDAUDIO")); // headphones on the jack
        assert!(!builtin_speakers(9, "HDAUDIO")); // monitor over HDMI
    }

    #[test]
    fn device_queries_do_not_fail() {
        eprintln!("on built-in speakers: {}", on_builtin_speakers());
    }
}
```

- [ ] **Step 2: Implement**

```rust
const FORM_FACTOR_SPEAKERS: u32 = 1;

/// Speakers wired to this computer (built in, or on its audio jack), not USB or Bluetooth.
pub fn builtin_speakers(form_factor: u32, enumerator: &str) -> bool {
    let external = ["USB", "BTHENUM", "BTHHFENUM", "BTHLEDEVICE"].iter().any(|e| enumerator.eq_ignore_ascii_case(e));
    form_factor == FORM_FACTOR_SPEAKERS && !external
}

pub fn on_builtin_speakers() -> bool {
    super::com::init();
    let Ok(device) = wasapi::get_default_device(&wasapi::Direction::Render) else { return false };
    let (Ok(form), Ok(enumerator)) = (device_u32(&device, &PKEY_AudioEndpoint_FormFactor), device_string(&device, &PKEY_Device_EnumeratorName)) else {
        return false;
    };
    builtin_speakers(form, &enumerator)
}

/// Windows loopback can't be blocked by a permission, so the Mac's "no call audio" watch doesn't run here.
pub fn others_playing() -> bool {
    false
}
```

  `device_u32` / `device_string` read the endpoint property store (`IMMDevice::OpenPropertyStore(STGM_READ)`,
  `GetValue`, PROPVARIANT → u32 / string). If the `wasapi::Device` in your version doesn't expose its
  `IMMDevice`, open the default render endpoint through `windows` directly (`IMMDeviceEnumerator`,
  `GetDefaultAudioEndpoint(eRender, eConsole)`). The `PKEY_*` constants are in
  `windows::Win32::Media::Audio` / `windows::Win32::Devices::FunctionDiscovery`.

- [ ] **Step 3: Run** → pass. **Manual:** laptop/desktop speakers → the warning shows in the recording popover;
  headphones → it goes away.
- [ ] **Step 4: Commit** `W1: built-in speakers check on Windows`

### Task 11: Device change watcher

**Files:** `platform/windows/devices.rs`

- [ ] **Step 1: Pure mapping** (failing test first)

```rust
    #[test]
    fn only_console_default_changes_matter() {
        assert_eq!(event_for(eRender, eConsole), Some(DeviceEvent::OutputChanged));
        assert_eq!(event_for(eCapture, eConsole), Some(DeviceEvent::InputChanged));
        assert_eq!(event_for(eRender, eCommunications), None); // fires alongside eConsole; avoid double restarts
        assert_eq!(event_for(eRender, eMultimedia), None);
    }
```

```rust
pub fn event_for(flow: EDataFlow, role: ERole) -> Option<DeviceEvent> {
    match (flow, role) {
        (f, r) if f == eRender && r == eConsole => Some(DeviceEvent::OutputChanged),
        (f, r) if f == eCapture && r == eConsole => Some(DeviceEvent::InputChanged),
        _ => None,
    }
}
```

- [ ] **Step 2: The watcher.** An `IMMNotificationClient` registered on an `IMMDeviceEnumerator`, unregistered on drop:

```rust
use windows::core::{implement, PCWSTR};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eMultimedia, eRender, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
    IMMNotificationClient_Impl, MMDeviceEnumerator, DEVICE_STATE,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use std::sync::Arc;

type OnChange = Arc<dyn Fn(DeviceEvent) + Send + Sync>;

#[implement(IMMNotificationClient)]
struct Client(OnChange);

impl IMMNotificationClient_Impl for Client_Impl {
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _id: &PCWSTR) -> windows::core::Result<()> {
        if let Some(e) = event_for(flow, role) {
            (self.0)(e);
        }
        Ok(())
    }
    fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> windows::core::Result<()> { Ok(()) }
    fn OnDeviceAdded(&self, _id: &PCWSTR) -> windows::core::Result<()> { Ok(()) }
    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> windows::core::Result<()> { Ok(()) }
    fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> windows::core::Result<()> { Ok(()) }
}

/// Listens for default input/output changes until dropped. The callback runs on a Windows audio thread.
pub struct DeviceWatcher {
    enumerator: IMMDeviceEnumerator,
    client: IMMNotificationClient,
}

// COM pointers from the multithreaded apartment; only used to unregister on drop.
unsafe impl Send for DeviceWatcher {}

impl DeviceWatcher {
    pub fn new(on_change: impl Fn(DeviceEvent) + Send + Sync + 'static) -> Result<Self, String> {
        super::com::init();
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
            let client: IMMNotificationClient = Client(Arc::new(on_change)).into();
            enumerator.RegisterEndpointNotificationCallback(&client).map_err(|e| e.to_string())?;
            Ok(DeviceWatcher { enumerator, client })
        }
    }
}

impl Drop for DeviceWatcher {
    fn drop(&mut self) {
        unsafe {
            let _ = self.enumerator.UnregisterEndpointNotificationCallback(&self.client);
        }
    }
}
```

  `windows` 0.62's `_Impl` trait signatures are generated. If the compiler disagrees on a parameter type
  (`&PCWSTR` vs `PCWSTR`, `windows_core::Ref`), follow the compiler's expected signature.

- [ ] **Step 3: Test** `a_watcher_can_be_added_and_removed` (same as the Mac test) → passes.
- [ ] **Step 4: Manual:** while recording, plug in or unplug headphones (or switch output in the taskbar
  volume flyout). Expected: one "Sound now plays through …" notice per switch and no gap beyond ~0.5 s in
  `them.wav`. Same for the mic: one "Switched to …" notice, and the recording continues. If cpal's own
  default-device follow and Talaan's restart fight (two notices, or a dropped stream), make
  `MicSource` open the default device by name (`host.default_input_device()` → then reopen *that* device)
  so only Talaan's restart moves it.
- [ ] **Step 5: Commit** `W1: follow default device changes on Windows`

### Task 12: Sleep auto-pause

**Files:** `platform/windows/power.rs`

- [ ] **Step 1: Pure mapping** (failing test)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suspend_pauses_and_resume_resumes() {
        assert_eq!(sleeping_for(PBT_APMSUSPEND), Some(true));
        assert_eq!(sleeping_for(PBT_APMRESUMEAUTOMATIC), Some(false));
        assert_eq!(sleeping_for(PBT_APMRESUMESUSPEND), None); // follows RESUMEAUTOMATIC; don't resume twice
        assert_eq!(sleeping_for(PBT_POWERSETTINGCHANGE), None);
    }

    #[test]
    fn a_watcher_can_be_added_and_removed() {
        drop(SleepWatcher::new(|_| {}));
    }
}
```

- [ ] **Step 2: Implement** with `PowerRegisterSuspendResumeNotification` (callback mode, no window needed)

```rust
//! Sleep and wake. Recording pauses itself while the PC sleeps, so closing the lid never leaves an hour of silence.

use std::ffi::c_void;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Power::{PowerRegisterSuspendResumeNotification, PowerUnregisterSuspendResumeNotification, DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, HPOWERNOTIFY};
use windows::Win32::UI::WindowsAndMessaging::{DEVICE_NOTIFY_CALLBACK, PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND};
#[cfg(test)]
use windows::Win32::UI::WindowsAndMessaging::{PBT_APMRESUMESUSPEND, PBT_POWERSETTINGCHANGE};

type OnChange = Box<dyn Fn(bool) + Send + Sync>;

pub fn sleeping_for(event: u32) -> Option<bool> {
    match event {
        PBT_APMSUSPEND => Some(true),
        PBT_APMRESUMEAUTOMATIC => Some(false),
        _ => None,
    }
}

unsafe extern "system" fn on_power(context: *const c_void, event: u32, _setting: *const c_void) -> u32 {
    let f = unsafe { &*(context as *const OnChange) };
    if let Some(sleeping) = sleeping_for(event) {
        f(sleeping);
    }
    0
}

pub struct SleepWatcher {
    handle: Option<HPOWERNOTIFY>,
    callback: *mut OnChange,
}

unsafe impl Send for SleepWatcher {}

impl SleepWatcher {
    /// `on_change(true)` before sleep, `on_change(false)` after wake.
    pub fn new(on_change: impl Fn(bool) + Send + Sync + 'static) -> Self {
        let callback: *mut OnChange = Box::into_raw(Box::new(Box::new(on_change)));
        let params = DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS { Callback: Some(on_power), Context: callback as *mut c_void };
        let mut handle = HPOWERNOTIFY::default();
        let ok = unsafe {
            PowerRegisterSuspendResumeNotification(DEVICE_NOTIFY_CALLBACK, HANDLE(&params as *const _ as *mut c_void), &mut handle as *mut _ as *mut *mut c_void)
        };
        SleepWatcher { handle: ok.is_ok().then_some(handle), callback }
    }
}

impl Drop for SleepWatcher {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            unsafe {
                let _ = PowerUnregisterSuspendResumeNotification(h);
            }
        }
        drop(unsafe { Box::from_raw(self.callback) });
    }
}
```

  The exact parameter types of `PowerRegisterSuspendResumeNotification` differ between `windows` releases
  (`HANDLE` vs `*const DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS`, out-pointer shape). Keep the design (callback +
  boxed context, unregister before freeing) and follow the compiler.

- [ ] **Step 3: Run** → pass. **Manual:** record, then Start › Power › Sleep, wait 30 s, wake. Expected: a "Paused"
  gap in the meeting, and recording continues after wake.
- [ ] **Step 4: Commit** `W1: auto-pause on sleep (Windows)`

### Task 13: Both tracks on one clock

**Files:** `src-tauri/src/recorder/mic.rs` (test only)

- [ ] **Step 1: An ignored test** proving cpal's mic stamps and `host_now_ns` agree (they must, or the tracks drift apart):

```rust
    /// The mic's buffer timestamps are on Talaan's host clock (within half a second of "now").
    /// `cargo test mic_clock_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mic_clock_smoke() {
        let (producer, mut consumer) = crate::recorder::feed::feed();
        let mut mic = MicSource::default();
        mic.start(std::sync::Arc::new(std::sync::Mutex::new(producer))).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        let mut buf = vec![];
        let h = consumer.pop(&mut buf).expect("a mic buffer");
        let now = crate::platform::host_now_ns();
        mic.stop();
        let diff = now.abs_diff(h.host_ns);
        eprintln!("mic stamp {} · now {} · diff {} ms", h.host_ns, now, diff / 1_000_000);
        assert!(diff < 500_000_000, "{diff}");
    }
```

- [ ] **Step 2: Run on Windows** (and later on the Mac). Expected: diff well under 500 ms.
- [ ] **Step 3: Alignment check** with the real recorder: in a quiet room, record 20 s while a YouTube clip
  plays through **speakers**. The mic hears the clip too. Open both WAVs (Audacity or the smoke's printout) and
  compare the offset of one sharp sound in `me.wav` vs `them.wav`. Expected: under ~50 ms (the Mac measured
  ~20 ms). Note the number for the Build Log.
- [ ] **Step 4: Commit** `W1: mic and system audio share the host clock (smoke)`

### Task 14: In the app, then the checkpoint

- [ ] **Step 1:** `npm run tauri dev`. Record from the toolbar: levels move for Me and Them, pause/resume and mark
  work, Stop transcribes (CPU) and the meeting has Me and Them lines. Force-quit (Task Manager) mid-recording,
  relaunch: Recover is offered and works.
- [ ] **Step 2:** Update `docs/WINDOWS-PORT.md` (what's done, the Windows commands: `scripts/build-llm-helper.ps1`,
  the smokes) and the `CLAUDE.md` Commands section (Windows lines next to the Mac ones). Update the Notion
  "Talaan Journey" Build Log (rule 7) with the W0–W1 entry: test counts, real-time factor, clock offset.
- [ ] **Step 3: Commit** `W1: docs for the Windows recorder`
- [ ] **🛑 Checkpoint (Miguel):** a real 5–10 minute Zoom or Teams call on the PC, with headphones. Expected: both
  sides in the transcript, Me/Them right, no echo lines. Also, optionally: unplug headphones mid-call, and sleep
  mid-call.

---

## Later (not in this plan)

- **GPU:** install the Vulkan SDK, build whisper-rs with `vulkan` and llama-cpp-2 with `vulkan`, run the
  pipeline smoke and the summary bake-off on CPU vs Vulkan on this PC (RTX 5070 Ti), and keep CPU as the
  fallback if a machine has no Vulkan driver. CUDA is out: NVIDIA-only and a huge runtime. Decide after the W1
  checkpoint, as part of W5 packaging. **Required with GPU:** automatic CPU fallback. If the GPU model load or
  the first encode fails, retry that meeting on CPU and remember it, instead of failing every call until the
  user finds the `useGpu` switch (today `transcriber/worker.rs` fails the meeting on a load error).
- W2 call detection, W3 shell, W4 calendar + tabs, W5 packaging: their own plans.
