# W3. Windows shell — task plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax.

> Part of the Windows port (docs/WINDOWS-PORT.md, phases W0–W5). Model: Sonnet for the tasks, Opus for review.
> The "menu bar" / "system tray" string table and Windows copy were built on a separate branch
> (`claude/optimistic-leavitt-332ac0`, plus translations in progress). They are merged later, not redone here.

**Goal:** Talaan behaves like a proper Windows tray app: one instance, a visible tray icon with the recording
time, its popover and call prompts next to the tray (bottom of the screen), the Windows title bar, Ctrl+, for
Settings, and "Summary ready" toasts that open the meeting.

**Architecture:** Tray placement becomes one pure function used by the popover and the prompt windows. The
toast lives in `platform/windows/notify.rs` behind the existing `setup`/`post` contract. Frontend OS checks go
in one small module (`src/lib/os.ts`) so they can be folded into the string-table module when branches merge.

**Tech stack:** Tauri 2 (`tauri-plugin-single-instance`), `tauri-winrt-notification` (Windows toasts), React.

---

## Verified facts (2026-10-15)

- `src-tauri/icons/tray@2x.png` is a black template glyph (macOS tints it). On Windows' dark taskbar it would be
  invisible. `icons/32x32.png` is the gold "ta" on ink, readable on light and dark taskbars.
- `tray.set_title()` shows text only on macOS. Windows needs the tooltip for the timer.
- The Windows tray is at the bottom (usually bottom right): `toggle_popover` and `place_under_icon` in
  `commands/tray.rs` put windows *below* the icon, which on Windows is off screen.
- `titleBarStyle: Overlay` and `hiddenTitle` are macOS-only, so Windows gets the native title bar. But
  `src/screens/Shell/Toolbar.tsx:30` still adds `pl-[84px]` (room for the macOS traffic lights) in Tauri.
- `src/screens/Shell/MainWindow.tsx:20` opens Settings on `metaKey + ","` only.
- `src/data/defaultSettings.ts:5` already has an `onWindows` userAgent check (W2).
- No single-instance guard: on Windows, launching Talaan twice starts a second process with a second tray icon
  and a second calls loop.

## Design (binding)

- **Single instance:** `tauri-plugin-single-instance` (registered first in `lib.rs`). A second launch shows and
  focuses the main window of the running one, then exits. Harmless on macOS (the OS already prevents it).
- **Tray icon:** macOS keeps `tray@2x.png` as a template. Windows uses `icons/32x32.png`, no template.
- **Tray timer on Windows:** the tooltip shows "Talaan · Recording 12:04" / "Talaan · Paused 12:04" / "Talaan".
  macOS keeps `set_title`.
- **Placement:** `fn place_near_icon(icon: Option<(x, y, w, h)>, win: (w, h), screen: (x, y, w, h)) -> (x, y)`,
  pure, in logical pixels.
  - If the icon's centre is in the bottom half of its screen, put the window **above** the icon (y = icon.y - h - 8).
  - Otherwise put it below (y = icon.y + icon.h + 4, today's behaviour).
  - Centre x on the icon, clamped inside the screen with 8 px margins.
  - No icon: top right on macOS (today's fallback, `34` from the top), bottom right above the taskbar on Windows
    (`screen.h - h - 56`).
  - Both `toggle_popover` and `place_under_icon` use it. Rename `place_under_icon` → `place_near_icon_window`
    and update the callers in `commands/calls.rs`.
- **Title bar:** the 84 px inset applies on macOS only. Windows keeps the native title bar. When the app theme
  changes (light/dark), set the native window theme too (`getCurrentWindow().setTheme(...)`), so the title bar
  matches. That's a no-op on macOS.
- **Settings shortcut:** Cmd+, on macOS, Ctrl+, on Windows.
- **Toasts:** `tauri-winrt-notification`.
  - `setup(on_click)` stores the callback.
  - `post(id, title, body)` shows a toast whose activation calls `on_click(id)` while Talaan runs. It's always
    running, since closing the window keeps it in the tray.
  - The app id: the bundle identifier `com.craftconceptsdigital.talaan` when installed. For a dev build (exe
    under a `target` folder), use the PowerShell app id the crate provides, because Windows only shows toasts for
    an app id with a Start-menu shortcut. Make this a pure `fn app_id(exe: &Path) -> &str`, tested.
  - **W5 must confirm** the NSIS shortcut carries that AppUserModelID.
- `src/lib/os.ts`: `export const onWindows` / `onMac` (userAgent), used by Toolbar, MainWindow, theme and
  `defaultSettings.ts` (move W2's check there). One place, folded into `platform.ts` at merge time.

## File map

- `src-tauri/Cargo.toml`: `tauri-plugin-single-instance` (all desktop); `tauri-winrt-notification` (Windows target)
- `src-tauri/src/lib.rs`: register single instance
- `src-tauri/src/commands/tray.rs`: icon per OS, tooltip timer, `place_near_icon` + tests
- `src-tauri/src/commands/calls.rs`: use the new placement fn
- `src-tauri/src/platform/windows/notify.rs`: toasts
- `src/lib/os.ts` (new), `src/screens/Shell/Toolbar.tsx`, `src/screens/Shell/MainWindow.tsx`, `src/lib/theme.ts`, `src/data/defaultSettings.ts`
- `THIRD_PARTY_NOTICES.md`, `docs/WINDOWS-PORT.md`

Rules: TDD for pure logic, commit per task (`W3:`), never push, commit messages end with
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`, no BOM, stage only your own files.
**macOS behaviour must not change.** Every Mac path stays as it is, behind a `cfg` or `onMac`.
Run `cargo test --workspace`, `npm test` **and `npm run build`** before each commit.

---

### Task 1: Single instance

- [ ] Add `tauri-plugin-single-instance = "2"` under `[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]`.
  Register it **first** in `lib.rs`:
  `.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| commands::tray::show_main(app)))`.
  Check the plugin's API for the version cargo resolves (closure signature).
- [ ] Check (manual, do it): build with `cargo build --manifest-path src-tauri/Cargo.toml`. Start the debug exe
  `src-tauri/target/debug/talaan.exe` twice from PowerShell (`Start-Process`). Expected: one `talaan.exe` process
  remains after ~5 s (`Get-Process talaan`). Stop it afterwards. (The frontend isn't needed for this check; the
  window may be blank without the dev server.)
- [ ] Notices entry (MIT/Apache-2.0, verify). Commit `W3: one Talaan at a time (single instance)`.

### Task 2: Tray on Windows (icon, timer, placement)

- [ ] **Failing tests** in `commands/tray.rs`:

```rust
    #[test]
    fn near_a_bottom_tray_windows_open_above_the_icon() {
        // 1920x1080 screen, tray icon at the bottom right (taskbar 48 px).
        let (x, y) = super::place_near_icon(Some((1800.0, 1040.0, 24.0, 24.0)), (356.0, 470.0), (0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(y, 1040.0 - 470.0 - 8.0);
        assert_eq!(x, 1920.0 - 356.0 - 8.0); // centred on the icon would overflow; clamped
    }

    #[test]
    fn near_a_top_menu_bar_windows_open_below_the_icon() {
        let (x, y) = super::place_near_icon(Some((1000.0, 0.0, 30.0, 24.0)), (356.0, 470.0), (0.0, 0.0, 1728.0, 1117.0));
        assert_eq!(y, 28.0);
        assert_eq!(x, 1015.0 - 178.0);
    }

    #[test]
    fn windows_stay_on_screen_on_a_second_monitor() {
        let (x, _) = super::place_near_icon(Some((1925.0, 1040.0, 24.0, 24.0)), (356.0, 470.0), (1920.0, 0.0, 2560.0, 1440.0));
        assert_eq!(x, 1920.0 + 8.0);
    }

    #[test]
    fn without_an_icon_windows_use_the_platform_corner() {
        let (_, y) = super::place_near_icon(None, (380.0, 148.0), (0.0, 0.0, 1920.0, 1080.0));
        if cfg!(target_os = "windows") { assert_eq!(y, 1080.0 - 148.0 - 56.0) } else { assert_eq!(y, 34.0) }
    }

    #[test]
    fn the_tray_tooltip_carries_the_timer() {
        assert_eq!(super::tooltip("recording", 65_000), "Talaan · Recording 01:05");
        assert_eq!(super::tooltip("paused", 65_000), "Talaan · Paused 01:05");
        assert_eq!(super::tooltip("idle", 0), "Talaan");
    }
```

- [ ] Implement:
  - `place_near_icon(icon: Option<(f64,f64,f64,f64)>, win: (f64,f64), screen: (f64,f64,f64,f64)) -> (f64,f64)`.
    The screen tuple is (x, y, w, h) in logical px, from the monitor of the icon (or the window's current
    monitor): `monitor.position()` / `size()` divided by the scale factor.
  - `toggle_popover` and the old `place_under_icon` (renamed `place_near_icon_window`) both compute it.
    Update `commands/calls.rs`.
  - Tray icon: `#[cfg(target_os = "macos")]` keeps `tray@2x.png` + `icon_as_template(true)`;
    `#[cfg(not(target_os = "macos"))]` uses `../../icons/32x32.png` without template.
  - `update()`: macOS keeps `set_title`; elsewhere `set_tooltip(Some(tooltip(state, elapsed)))`.
    `tooltip` is pure and reuses `clock`.
- [ ] Commit `W3: tray on Windows: visible icon, timer in the tooltip, windows open above the taskbar`.

### Task 3: Title bar, Ctrl+, and window theme

- [ ] Create `src/lib/os.ts`:

```ts
// Which OS the app is running on, from the webview's user agent (WebView2 says "Windows NT", WKWebView doesn't).
// The one place for OS checks in the frontend; fold into the platform string table when branches merge.
const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
export const onWindows = /windows/i.test(ua);
export const onMac = !onWindows && /mac/i.test(ua);
```

  plus `src/lib/os.test.ts` that only checks both are booleans and not both true.
  Move `defaultSettings.ts`'s own check to import `onWindows` from it.
- [ ] `Toolbar.tsx:30`: `inTauri() && onMac && "pl-[84px]"`.
  `MainWindow.tsx:20`: `if ((onWindows ? e.ctrlKey : e.metaKey) && e.key === ",")`.
  Find any other `metaKey` shortcut in `src/` and treat it the same way.
- [ ] `src/lib/theme.ts`: wherever the resolved theme (light/dark) is applied to `<html>`, also call
  `getCurrentWindow().setTheme(resolved)` from `@tauri-apps/api/window` when running in Tauri (guarded; errors
  ignored). Apply it to every open Talaan window: the theme is applied per window already, since each window
  loads the app. Read `theme.ts` and follow its pattern. Check that `core:window:allow-set-theme` (or the
  matching permission) is granted in `src-tauri/capabilities/*.json`; add it if not.
- [ ] `npm test`, `npm run build`. Commit `W3: Windows title bar, Ctrl+, for Settings, native theme follows the app`.

### Task 4: "Summary ready" toasts

- [ ] Add `tauri-winrt-notification` (latest) under the Windows target deps. Verify the license and add notices.
- [ ] **Failing test** in `platform/windows/notify.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn installed_builds_use_talaan_s_own_app_id() {
        assert_eq!(app_id(Path::new(r"C:\Users\a\AppData\Local\Talaan\talaan.exe")), "com.craftconceptsdigital.talaan");
        assert_eq!(app_id(Path::new(r"C:\Program Files\Talaan\Talaan.exe")), "com.craftconceptsdigital.talaan");
    }

    #[test]
    fn dev_builds_borrow_an_app_id_windows_already_knows() {
        assert_eq!(app_id(Path::new(r"C:\Dev\Talaan\src-tauri\target\debug\talaan.exe")), tauri_winrt_notification::Toast::POWERSHELL_APP_ID);
    }
}
```

- [ ] Implement:

```rust
//! "Summary ready" notifications as Windows toasts. Clicking one opens the meeting (Talaan keeps running in the
//! tray, so the click reaches this process). Windows shows toasts only for an app id with a Start-menu shortcut:
//! the installer's (Talaan's bundle id) when installed, PowerShell's for dev builds.

use std::path::Path;
use std::sync::OnceLock;
use tauri_winrt_notification::Toast;

type OnClick = Box<dyn Fn(String) + Send + Sync>;
static ON_CLICK: OnceLock<OnClick> = OnceLock::new();

const APP_ID: &str = "com.craftconceptsdigital.talaan";

pub fn app_id(exe: &Path) -> &'static str {
    let dev = exe.components().any(|c| c.as_os_str().eq_ignore_ascii_case("target"));
    if dev { Toast::POWERSHELL_APP_ID } else { APP_ID }
}

pub fn setup(on_click: impl Fn(String) + Send + Sync + 'static) {
    let _ = ON_CLICK.set(Box::new(on_click));
}

pub fn post(id: &str, title: &str, body: &str) {
    let exe = std::env::current_exe().unwrap_or_default();
    let id = id.to_string();
    let _ = Toast::new(app_id(&exe))
        .title(title)
        .text1(body)
        .on_activated(move |_action| {
            if let Some(f) = ON_CLICK.get() {
                f(id.clone());
            }
            Ok(())
        })
        .show();
}
```

  Follow the crate's real API for the version you get (`on_activated`'s closure signature changed between
  releases). Toasts must never fail the caller: errors are ignored.
- [ ] Manual check (do it): an ignored test `toast_smoke` that posts one toast with a dev app id and sleeps 5 s.
  Run it, and report whether `Show()` returned Ok (you can't see the screen; that's fine).
- [ ] Commit `W3: Summary ready toasts on Windows`.

### Task 5: Docs

- [ ] `docs/WINDOWS-PORT.md` Progress: W3 done. Add a "Check by hand before release" list for Miguel:
  - the tray icon is visible on a light and a dark taskbar;
  - the popover opens above the tray;
  - a second launch focuses the first window;
  - the toast appears and opens the meeting (installed build);
  - the title bar follows dark mode;
  - Ctrl+, opens Settings.
  Commit `W3: docs`.
