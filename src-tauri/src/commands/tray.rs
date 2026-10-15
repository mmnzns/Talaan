//! The menu bar: Talaan's icon (with a timer while recording), the popover under it, and the main window
//! hiding instead of quitting so call detection keeps running.

use super::CmdResult;
use crate::recorder::engine::Status;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Rect, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub const TRAY_ID: &str = "talaan";
pub const POPOVER: &str = "tray";
const POPOVER_SIZE: (f64, f64) = (356.0, 470.0);

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Talaan", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Talaan", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &settings, &PredefinedMenuItem::separator(app)?, &quit])?;
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
    // macOS: with no known icon position the popover stays where it was, as before.
    let rect = icon.or_else(|| app.tray_by_id(TRAY_ID).and_then(|t| t.rect().ok().flatten()));
    if rect.is_some() || cfg!(not(target_os = "macos")) {
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
    match state {
        "recording" => format!("Talaan · Recording {}", clock(elapsed_ms)),
        "paused" => format!("Talaan · Paused {}", clock(elapsed_ms)),
        _ => "Talaan".to_string(),
    }
}

fn clock(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) }
}

/// A floating window sizes itself to its content. The call prompt also places itself next to Talaan's tray
/// icon (away from the corner where the OS shows its own notifications); the popover keeps its place.
#[tauri::command]
pub fn window_fit(window: tauri::WebviewWindow, width: f64, height: f64) {
    let _ = window.set_size(tauri::LogicalSize::new(width, height));
    if window.label() == super::calls::PROMPT_WINDOW {
        place_near_icon_window(&window, (width, height), None);
    }
}

/// Where a floating window of size `win` goes, in logical pixels. `icon` is the tray icon (x, y, w, h) and
/// `screen` the (x, y, w, h) of its monitor. Next to a tray in the bottom half of the screen (the Windows
/// taskbar) the window opens above the icon; next to a top menu bar (macOS) below it. Centred on the icon and
/// kept on screen. Without an icon: top right on macOS, bottom right above the taskbar on Windows.
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

/// Moves `window` next to the tray icon (`icon`, or the tray's own rect), on the icon's monitor.
pub fn place_near_icon_window(window: &tauri::WebviewWindow, win: (f64, f64), icon: Option<Rect>) {
    let app = window.app_handle();
    let icon = icon.or_else(|| app.tray_by_id(TRAY_ID).and_then(|t| t.rect().ok().flatten()));
    let physical = icon.map(|r| (r.position.to_physical::<f64>(1.0), r.size.to_physical::<f64>(1.0)));
    let monitor = physical
        .and_then(|(p, _)| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(m) = monitor else { return };
    let scale = m.scale_factor();
    let screen = (
        m.position().x as f64 / scale,
        m.position().y as f64 / scale,
        m.size().width as f64 / scale,
        m.size().height as f64 / scale,
    );
    let icon = physical.map(|(p, s)| (p.x / scale, p.y / scale, s.width / scale, s.height / scale));
    let (x, y) = place_near_icon(icon, win, screen);
    let _ = window.set_position(tauri::LogicalPosition::new(x, y));
}

#[tauri::command]
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
    fn the_tray_tooltip_carries_the_timer() {
        assert_eq!(super::tooltip("recording", 65_000), "Talaan · Recording 01:05");
        assert_eq!(super::tooltip("paused", 65_000), "Talaan · Paused 01:05");
        assert_eq!(super::tooltip("idle", 0), "Talaan");
    }
}
