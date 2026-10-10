use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::db::{load_settings, AppState, Settings};
use crate::error::{AppResult, Context};

pub const BACKGROUND_ARG: &str = "--background";

pub fn starts_in_background(args: &[String], settings: &Settings) -> bool {
    cfg!(target_os = "macos") && settings.menu_bar_icon && args.iter().any(|a| a == BACKGROUND_ARG)
}

pub fn keeps_running_on_close(settings: &Settings) -> bool {
    cfg!(target_os = "macos") && settings.menu_bar_icon && settings.menu_bar_keep_running
}

pub fn panel_position(icon: (f64, f64, f64, f64), panel_width: f64, screen: (f64, f64), gap: f64) -> (f64, f64) {
    let (icon_x, icon_y, icon_w, icon_h) = icon;
    let (screen_x, screen_w) = screen;
    let x = icon_x + icon_w / 2.0 - panel_width / 2.0;
    let max_x = screen_x + screen_w - panel_width - gap;
    (x.min(max_x).max(screen_x + gap).round(), (icon_y + icon_h + gap).round())
}

pub const PANEL_REOPEN_GUARD: std::time::Duration = std::time::Duration::from_millis(300);

pub fn panel_may_open(since_blur_hide: Option<std::time::Duration>) -> bool {
    since_blur_hide.is_none_or(|d| d >= PANEL_REOPEN_GUARD)
}

pub fn settings_of(app: &AppHandle) -> Option<Settings> {
    let state = app.try_state::<AppState>()?;
    let conn = state.conn().ok()?;
    load_settings(&conn).ok()
}

#[tauri::command]
pub fn refresh_menu_bar(app: AppHandle) {
    #[cfg(target_os = "macos")]
    crate::tray::refresh(&app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[tauri::command]
pub fn open_main_window(app: AppHandle) {
    #[cfg(target_os = "macos")]
    crate::tray::show_main(&app);
    #[cfg(not(target_os = "macos"))]
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[tauri::command]
pub fn hide_menu_bar_panel(app: AppHandle) {
    #[cfg(target_os = "macos")]
    crate::tray::hide_panel(&app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn autostart_enabled(app: AppHandle) -> AppResult<bool> {
    app.autolaunch().is_enabled().ctx("Autostart")
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> AppResult<bool> {
    let launcher = app.autolaunch();
    if enabled { launcher.enable() } else { launcher.disable() }.ctx("Autostart")?;
    launcher.is_enabled().ctx("Autostart")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn background_start_needs_the_flag_and_the_menu_bar_icon() {
        let on = Settings::default();
        let off = Settings { menu_bar_icon: false, ..Settings::default() };
        assert_eq!(starts_in_background(&args(&["midnight", BACKGROUND_ARG]), &on), cfg!(target_os = "macos"));
        assert!(!starts_in_background(&args(&["midnight"]), &on));
        assert!(!starts_in_background(&args(&["midnight", BACKGROUND_ARG]), &off), "never start invisible without a way back");
    }

    #[test]
    fn closing_keeps_running_only_when_enabled() {
        assert_eq!(keeps_running_on_close(&Settings::default()), cfg!(target_os = "macos"));
        assert!(!keeps_running_on_close(&Settings { menu_bar_keep_running: false, ..Settings::default() }));
        assert!(!keeps_running_on_close(&Settings { menu_bar_icon: false, ..Settings::default() }), "without the icon there is no way back, so quit");
    }

    #[test]
    fn icon_click_right_after_the_panel_closed_does_not_reopen_it() {
        use std::time::Duration;
        assert!(panel_may_open(None), "first click opens");
        assert!(!panel_may_open(Some(Duration::from_millis(40))), "the closing click");
        assert!(panel_may_open(Some(Duration::from_secs(2))), "a later click opens again");
    }

    #[test]
    fn panel_sits_centered_below_the_icon_and_stays_on_screen() {
        let screen = (0.0, 3024.0);
        assert_eq!(panel_position((1500.0, 0.0, 44.0, 48.0), 640.0, screen, 8.0), (1202.0, 56.0));
        assert_eq!(panel_position((2990.0, 0.0, 44.0, 48.0), 640.0, screen, 8.0), (2376.0, 56.0), "pushed left at the right edge");
        assert_eq!(panel_position((10.0, 0.0, 44.0, 48.0), 640.0, screen, 8.0), (8.0, 56.0), "pushed right at the left edge");
        assert_eq!(panel_position((3500.0, 0.0, 44.0, 48.0), 640.0, (3024.0, 1920.0), 8.0), (3202.0, 56.0), "second screen");
    }
}
