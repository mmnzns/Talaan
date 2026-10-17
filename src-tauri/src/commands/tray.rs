//! The menu bar: Talaan's icon (with a timer while recording), the popover under it, and the main window
//! hiding instead of quitting so call detection keeps running.

use super::CmdResult;
use crate::i18n::tr;
use crate::recorder::engine::Status;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Rect, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub const TRAY_ID: &str = "talaan";
pub const POPOVER: &str = "tray";
/// Sent to the popover with the height it may use (logical px), between the tray icon and the far screen edge.
pub const TRAY_ROOM: &str = "tray-room";
const POPOVER_SIZE: (f64, f64) = (356.0, 470.0);

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let open = MenuItem::with_id(app, "open", tr("tray.open"), true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", tr("tray.settings"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", tr("tray.quit"), true, None::<&str>)?;
    Menu::with_items(app, &[&open, &settings, &PredefinedMenuItem::separator(app)?, &quit])
}

/// The screens resolve "Auto" to a language and report it here (every window, whenever it changes). The menu bar menu
/// is rebuilt in that language; notices and notifications read it when they are sent.
#[tauri::command]
pub fn i18n_set_language(app: AppHandle, lang: String) {
    let lang = crate::i18n::Lang::parse(&lang);
    if crate::i18n::current() == lang {
        return;
    }
    crate::i18n::set(lang);
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(&app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    // macOS tints a black template glyph; Windows needs the coloured icon to read on light and dark taskbars.
    #[cfg(target_os = "macos")]
    let builder = TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::image::Image::from_bytes(include_bytes!("../../icons/tray@2x.png"))?)
        .icon_as_template(true);
    #[cfg(not(target_os = "macos"))]
    let builder = TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::image::Image::from_bytes(include_bytes!("../../icons/32x32.png"))?);
    builder
        .tooltip("Talaan")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_main(app),
            "settings" => {
                show_main(app);
                let _ = tauri::Emitter::emit(app, "open-settings", ());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = e {
                toggle_popover(tray.app_handle(), Some(rect));
            }
        })
        .build(app)?;
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Main window close: hide it, keep Talaan in the menu bar.
pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    match (window.label(), event) {
        ("main", WindowEvent::CloseRequested { api, .. }) => {
            api.prevent_close();
            let _ = window.hide();
        }
        // The popover closes like a menu when you click elsewhere.
        (POPOVER, WindowEvent::Focused(false)) => {
            let _ = window.hide();
        }
        _ => {}
    }
}

fn popover(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    if let Some(w) = app.get_webview_window(POPOVER) {
        return Some(w);
    }
    WebviewWindowBuilder::new(app, POPOVER, WebviewUrl::App("index.html#/tray".into()))
        .title("Talaan")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .visible(false)
        .inner_size(POPOVER_SIZE.0, POPOVER_SIZE.1)
        .build()
        .ok()
}

/// Opens the popover under the menu bar icon (or closes it if open).
pub fn toggle_popover(app: &AppHandle, icon: Option<Rect>) {
    let Some(w) = popover(app) else { return };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        return;
    }
    let rect = icon.or_else(|| app.tray_by_id(TRAY_ID).and_then(|t| t.rect().ok().flatten()));
    // macOS: exactly as before W3 (the tray rect is physical px, and tao's monitor lookup uses points).
    #[cfg(target_os = "macos")]
    {
        if let (Some(rect), Ok(scale)) = (rect, w.scale_factor()) {
            let pos = rect.position.to_logical::<f64>(scale);
            let size = rect.size.to_logical::<f64>(scale);
            let x = pos.x + size.width / 2.0 - POPOVER_SIZE.0 / 2.0;
            let _ = w.set_position(tauri::LogicalPosition::new(x.max(8.0), pos.y + size.height + 4.0));
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // The recording popover sizes itself to its content (window_fit) but never taller than this, so its Stop
        // footer can't end up behind the taskbar.
        if let Some(room) = room_for_window(&w, rect) {
            let _ = tauri::Emitter::emit(app, TRAY_ROOM, room);
        }
        place_near_icon_window(&w, POPOVER_SIZE, rect);
    }
    let _ = w.show();
    let _ = w.set_focus();
}

/// Timer next to the icon while recording ("● 12:04", "❚❚ 12:04" when paused); nothing when idle.
/// Only macOS shows tray text; elsewhere the timer goes in the tooltip.
pub fn update(app: &AppHandle, status: &Status) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    #[cfg(target_os = "macos")]
    {
        let title = match status.state {
            "recording" => Some(format!("● {}", clock(status.elapsed_ms))),
            "paused" => Some(format!("❚❚ {}", clock(status.elapsed_ms))),
            _ => None,
        };
        let _ = tray.set_title(title);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = tray.set_tooltip(Some(tooltip(status.state, status.elapsed_ms)));
}

/// The tray tooltip ("Talaan · Recording 12:04"), the Windows stand-in for the menu bar timer.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn tooltip(state: &str, elapsed_ms: u64) -> String {
    tooltip_in(crate::i18n::current(), state, elapsed_ms)
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn tooltip_in(lang: crate::i18n::Lang, state: &str, elapsed_ms: u64) -> String {
    let key = match state {
        "recording" => "tray.tooltip_recording",
        "paused" => "tray.tooltip_paused",
        _ => return "Talaan".to_string(),
    };
    crate::i18n::tr_in(lang, key, &[("time", &clock(elapsed_ms))])
}

fn clock(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) }
}

/// A floating window sizes itself to its content. The call prompt also places itself next to Talaan's tray
/// icon (away from the corner where the OS shows its own notifications). On the Mac the popover keeps its place
/// under the menu bar; elsewhere it grows upward from the taskbar, so it is placed again for its new size.
#[tauri::command]
pub fn window_fit(window: tauri::WebviewWindow, width: f64, height: f64) {
    let _ = window.set_size(tauri::LogicalSize::new(width, height));
    let replace = window.label() == super::calls::PROMPT_WINDOW || (cfg!(not(target_os = "macos")) && window.label() == POPOVER);
    if replace {
        place_near_icon_window(&window, (width, height), None);
    }
}

/// The height the popover window may use (logical px), or None when there's no tray icon to measure from or on
/// the Mac (where the popover keeps its own limit).
#[tauri::command]
pub fn tray_room(window: tauri::WebviewWindow) -> Option<f64> {
    #[cfg(target_os = "macos")]
    {
        let _ = window;
        None
    }
    #[cfg(not(target_os = "macos"))]
    room_for_window(&window, None)
}

/// Room for a window next to the tray icon: from the icon to the far edge of the screen, less the gaps
/// `place_near_icon` keeps (8 px from the icon above a taskbar, 4 px below a top bar, 8 px from the edge).
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn room_near_icon(icon: (f64, f64, f64, f64), screen: (f64, f64, f64, f64)) -> f64 {
    let (_, sy, _, sh) = screen;
    let (_, iy, _, ih) = icon;
    let room = if iy + ih / 2.0 > sy + sh / 2.0 { iy - 8.0 - (sy + 8.0) } else { sy + sh - 8.0 - (iy + ih + 4.0) };
    room.max(240.0)
}

/// Where a floating window of size `win` goes, in logical pixels. `icon` is the tray icon (x, y, w, h) and
/// `screen` the (x, y, w, h) of its monitor. Next to a tray in the bottom half of the screen (the Windows
/// taskbar) the window opens above the icon; next to a top menu bar below it. Centred on the icon and kept on
/// screen. Without an icon: top right on macOS, bottom right above the taskbar on Windows. Windows only in use;
/// macOS has its own placement (`place_under_icon`).
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn place_near_icon(icon: Option<(f64, f64, f64, f64)>, win: (f64, f64), screen: (f64, f64, f64, f64)) -> (f64, f64) {
    let (sx, sy, sw, sh) = screen;
    let (x, y) = match icon {
        Some((ix, iy, iw, ih)) => {
            let y = if iy + ih / 2.0 > sy + sh / 2.0 { iy - win.1 - 8.0 } else { iy + ih + 4.0 };
            (ix + iw / 2.0 - win.0 / 2.0, y)
        }
        None if cfg!(target_os = "windows") => (sx + sw - win.0 - 12.0, sy + sh - win.1 - 56.0),
        None => (sx + sw - win.0 - 12.0, sy + 34.0),
    };
    (x.clamp(sx + 8.0, (sx + sw - win.0 - 8.0).max(sx + 8.0)), y)
}

/// Physical position for a window of logical size `win` next to a tray icon, on one monitor. Works in
/// monitor-local logical units, so a monitor at any origin and scale (mixed DPI) comes out right:
/// `icon_phys` (x, y, w, h), `mon_pos` and `mon_size` are physical px, `scale` the monitor's scale factor.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn place_on_monitor(
    icon_phys: Option<(f64, f64, f64, f64)>,
    win: (f64, f64),
    mon_pos: (f64, f64),
    mon_size: (f64, f64),
    scale: f64,
) -> (f64, f64) {
    let local_icon = icon_phys.map(|(x, y, w, h)| ((x - mon_pos.0) / scale, (y - mon_pos.1) / scale, w / scale, h / scale));
    let screen = (0.0, 0.0, mon_size.0 / scale, mon_size.1 / scale);
    let (x, y) = place_near_icon(local_icon, win, screen);
    (mon_pos.0 + x * scale, mon_pos.1 + y * scale)
}

/// Moves `window` next to Talaan's tray icon. `icon` is the icon's rect if the caller has one.
#[cfg(target_os = "macos")]
pub fn place_near_icon_window(window: &tauri::WebviewWindow, win: (f64, f64), _icon: Option<Rect>) {
    place_under_icon(window, win.0);
}

/// The tray icon (physical x, y, w, h) and its monitor (physical position, size, scale factor).
#[cfg(not(target_os = "macos"))]
type IconOnMonitor = (Option<(f64, f64, f64, f64)>, (f64, f64), (f64, f64), f64);

#[cfg(not(target_os = "macos"))]
fn icon_on_monitor(window: &tauri::WebviewWindow, icon: Option<Rect>) -> Option<IconOnMonitor> {
    let app = window.app_handle();
    let icon = icon.or_else(|| app.tray_by_id(TRAY_ID).and_then(|t| t.rect().ok().flatten()));
    let physical = icon.map(|r| (r.position.to_physical::<f64>(1.0), r.size.to_physical::<f64>(1.0)));
    let monitor = physical
        .and_then(|(p, _)| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten())?;
    let icon = physical.map(|(p, s)| (p.x, p.y, s.width, s.height));
    let mon_pos = (monitor.position().x as f64, monitor.position().y as f64);
    let mon_size = (monitor.size().width as f64, monitor.size().height as f64);
    Some((icon, mon_pos, mon_size, monitor.scale_factor()))
}

/// Windows: on the icon's monitor, above the taskbar.
#[cfg(not(target_os = "macos"))]
pub fn place_near_icon_window(window: &tauri::WebviewWindow, win: (f64, f64), icon: Option<Rect>) {
    let Some((icon, mon_pos, mon_size, scale)) = icon_on_monitor(window, icon) else { return };
    let (x, y) = place_on_monitor(icon, win, mon_pos, mon_size, scale);
    let _ = window.set_position(tauri::PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// Windows: the popover's room on the icon's monitor, in that monitor's logical px.
#[cfg(not(target_os = "macos"))]
fn room_for_window(window: &tauri::WebviewWindow, icon: Option<Rect>) -> Option<f64> {
    let (icon, mon_pos, mon_size, scale) = icon_on_monitor(window, icon)?;
    Some(room_on_monitor(icon?, mon_pos, mon_size, scale))
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn room_on_monitor(icon_phys: (f64, f64, f64, f64), mon_pos: (f64, f64), mon_size: (f64, f64), scale: f64) -> f64 {
    let (x, y, w, h) = icon_phys;
    let icon = ((x - mon_pos.0) / scale, (y - mon_pos.1) / scale, w / scale, h / scale);
    room_near_icon(icon, (0.0, 0.0, mon_size.0 / scale, mon_size.1 / scale))
}

/// macOS (unchanged from before W3): centers a window under the menu bar icon, kept on screen. Falls back to
/// the top right.
#[cfg(target_os = "macos")]
pub fn place_under_icon(window: &tauri::WebviewWindow, width: f64) {
    let Ok(Some(m)) = window.current_monitor().or_else(|_| window.primary_monitor()) else { return };
    let scale = m.scale_factor();
    let screen_w = m.size().width as f64 / scale;
    let icon = window.app_handle().tray_by_id(TRAY_ID).and_then(|t| t.rect().ok().flatten());
    let (x, y) = match icon {
        Some(r) => {
            let pos = r.position.to_logical::<f64>(scale);
            let size = r.size.to_logical::<f64>(scale);
            (pos.x + size.width / 2.0 - width / 2.0, pos.y + size.height + 4.0)
        }
        None => (screen_w - width - 12.0, 34.0),
    };
    let _ = window.set_position(tauri::LogicalPosition::new(x.clamp(8.0, (screen_w - width - 8.0).max(8.0)), y));
}

/// `async`: off the main thread. Opening the popover the first time builds its webview, and on Windows building a
/// webview inside a synchronous command waits forever (WebView2 can't finish creating it while its own request
/// handler, the command, is still running). Talaan v1.0.2 froze that way: every window's commands stopped
/// answering and Quit did nothing. Never build a window from a synchronous command or anything it calls.
#[tauri::command(async)]
pub fn tray_toggle(app: AppHandle) {
    toggle_popover(&app, None);
}

#[tauri::command]
pub fn tray_hide(app: AppHandle) {
    if let Some(w) = app.get_webview_window(POPOVER) {
        let _ = w.hide();
    }
}

#[tauri::command]
pub fn app_show_main(app: AppHandle) {
    show_main(&app);
}

#[tauri::command]
pub fn app_quit(app: AppHandle) -> CmdResult<()> {
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_menu_bar_clock_reads_like_the_app() {
        assert_eq!(super::clock(65_000), "01:05");
        assert_eq!(super::clock(3_725_000), "1:02:05");
    }

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
    fn the_popover_may_use_the_height_between_the_icon_and_the_far_edge() {
        // 1366x768 laptop, taskbar 48 px at the bottom: icon at y 728. Room from 16 px below the top to 8 px above it.
        assert_eq!(super::room_near_icon((1300.0, 728.0, 24.0, 24.0), (0.0, 0.0, 1366.0, 768.0)), 728.0 - 16.0);
        // A top menu bar or taskbar: from 4 px under the icon to 8 px above the bottom.
        assert_eq!(super::room_near_icon((1000.0, 0.0, 30.0, 24.0), (0.0, 0.0, 1366.0, 768.0)), 768.0 - 8.0 - 28.0);
        // A popover as tall as the room, placed above the icon, stays on screen.
        let room = super::room_near_icon((1300.0, 728.0, 24.0, 24.0), (0.0, 0.0, 1366.0, 768.0));
        let (_, y) = super::place_near_icon(Some((1300.0, 728.0, 24.0, 24.0)), (356.0, room), (0.0, 0.0, 1366.0, 768.0));
        assert_eq!(y, 8.0);
        // On a 150% monitor the room is in its logical px.
        assert_eq!(super::room_on_monitor((1950.0, 1092.0, 36.0, 36.0), (0.0, 0.0), (2049.0, 1152.0), 1.5), 728.0 - 16.0);
    }

    #[test]
    fn a_second_monitor_with_a_different_scale_places_in_its_own_units() {
        // Monitor at physical (1920, 0), 2880x1620 at 1.5 (1920x1080 logical); icon near its bottom right.
        let (x, y) = super::place_on_monitor(Some((1920.0 + 2700.0, 1560.0, 36.0, 36.0)), (356.0, 470.0), (1920.0, 0.0), (2880.0, 1620.0), 1.5);
        // Same as the 1920x1080 case: logical (1556, 562), back to physical on this monitor.
        assert_eq!(x, 1920.0 + 1556.0 * 1.5);
        assert_eq!(y, 562.0 * 1.5);
    }

    #[test]
    fn the_tray_tooltip_carries_the_timer() {
        use crate::i18n::Lang;
        assert_eq!(super::tooltip_in(Lang::En, "recording", 65_000), "Talaan · Recording 01:05");
        assert_eq!(super::tooltip_in(Lang::En, "paused", 65_000), "Talaan · Paused 01:05");
        assert_eq!(super::tooltip_in(Lang::Fr, "recording", 65_000), "Talaan · Enregistrement 01:05");
        assert_eq!(super::tooltip_in(Lang::Es, "paused", 65_000), "Talaan · En pausa 01:05");
        assert_eq!(super::tooltip_in(Lang::En, "idle", 0), "Talaan");
    }
}
