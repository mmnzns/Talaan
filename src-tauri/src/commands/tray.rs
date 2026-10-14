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
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::image::Image::from_bytes(include_bytes!("../../icons/tray@2x.png"))?)
        .icon_as_template(true)
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
    if let (Some(rect), Ok(scale)) = (rect, w.scale_factor()) {
        let pos = rect.position.to_logical::<f64>(scale);
        let size = rect.size.to_logical::<f64>(scale);
        let x = pos.x + size.width / 2.0 - POPOVER_SIZE.0 / 2.0;
        let _ = w.set_position(tauri::LogicalPosition::new(x.max(8.0), pos.y + size.height + 4.0));
    }
    let _ = w.show();
    let _ = w.set_focus();
}

/// Timer next to the icon while recording ("● 12:04", "❚❚ 12:04" when paused); nothing when idle.
pub fn update(app: &AppHandle, status: &Status) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let title = match status.state {
        "recording" => Some(format!("● {}", clock(status.elapsed_ms))),
        "paused" => Some(format!("❚❚ {}", clock(status.elapsed_ms))),
        _ => None,
    };
    let _ = tray.set_title(title);
}

fn clock(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) }
}

/// A floating window sizes itself to its content. The call prompt also places itself under Talaan's menu bar
/// icon (away from the corner where macOS shows its own notifications); the popover keeps its place.
#[tauri::command]
pub fn window_fit(window: tauri::WebviewWindow, width: f64, height: f64) {
    let _ = window.set_size(tauri::LogicalSize::new(width, height));
    if window.label() == super::calls::PROMPT_WINDOW {
        place_under_icon(&window, width);
    }
}

/// Centers a window under the menu bar icon, kept on screen. Falls back to the top right.
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
}
