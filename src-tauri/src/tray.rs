use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};

use crate::error::AppResult;
use crate::tracker::query_projects;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayProject {
    pub id: String,
    pub name: String,
    pub customer_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayRunning {
    pub project_id: String,
    pub project_name: String,
    pub start: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrayModel {
    pub projects: Vec<TrayProject>,
    pub running: Option<TrayRunning>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TrayAction {
    Start(String),
    Stop,
    Open,
    Quit,
}

pub fn tray_model(conn: &Connection) -> AppResult<TrayModel> {
    let projects = query_projects(conn, false)?
        .into_iter()
        .map(|p| TrayProject { id: p.id, name: p.name, customer_name: p.customer_name })
        .collect();
    let running = conn
        .query_row(
            "SELECT e.project_id, p.name, e.start FROM tracker_entries e
             JOIN tracker_projects p ON p.id = e.project_id
             WHERE e.stop IS NULL ORDER BY e.start DESC LIMIT 1",
            [],
            |r| Ok(TrayRunning { project_id: r.get(0)?, project_name: r.get(1)?, start: r.get(2)? }),
        )
        .optional()?;
    Ok(TrayModel { projects, running })
}

pub fn elapsed_secs(start: &str, now: DateTime<Utc>) -> i64 {
    DateTime::parse_from_rfc3339(start).map(|s| (now - s.with_timezone(&Utc)).num_seconds().max(0)).unwrap_or(0)
}

pub fn fmt_elapsed(secs: i64) -> String {
    let mins = secs.max(0) / 60;
    format!("{}:{:02}", mins / 60, mins % 60)
}

pub fn tray_title(model: &TrayModel, now: DateTime<Utc>) -> Option<String> {
    model.running.as_ref().map(|r| fmt_elapsed(elapsed_secs(&r.start, now)))
}

pub fn action_id(action: &TrayAction) -> String {
    match action {
        TrayAction::Start(id) => format!("start:{id}"),
        TrayAction::Stop => "stop".into(),
        TrayAction::Open => "open".into(),
        TrayAction::Quit => "quit".into(),
    }
}

pub fn parse_action(id: &str) -> Option<TrayAction> {
    match id {
        "stop" => Some(TrayAction::Stop),
        "open" => Some(TrayAction::Open),
        "quit" => Some(TrayAction::Quit),
        _ => id.strip_prefix("start:").filter(|p| !p.is_empty()).map(|p| TrayAction::Start(p.to_string())),
    }
}

#[cfg(target_os = "macos")]
mod menu_bar {
    use std::time::Duration;

    use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
    use tauri::{ActivationPolicy, AppHandle, Emitter, Manager, PhysicalPosition, Rect, WebviewUrl, WebviewWindowBuilder, Wry};

    use super::*;
    use crate::db::{lock, AppState, Db};
    use crate::i18n::{pick, tr};

    const TRAY_ID: &str = "midnight-tray";
    const POLL: Duration = Duration::from_secs(5);
    const CHANGED_EVENT: &str = "tracker-changed";
    const PANEL: &str = "menubar";
    const PANEL_SIZE: (f64, f64) = (340.0, 460.0);

    static PANEL_BLUR_HIDDEN_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

    fn mark_blur_hide(at: Option<std::time::Instant>) {
        if let Ok(mut last) = PANEL_BLUR_HIDDEN_AT.lock() {
            *last = at;
        }
    }

    fn build_menu(app: &AppHandle, model: &TrayModel) -> tauri::Result<Menu<Wry>> {
        let menu = Menu::new(app)?;
        if let Some(r) = &model.running {
            let label = tr!("Stop – {}", "Stoppen – {}", r.project_name);
            menu.append(&MenuItem::with_id(app, action_id(&TrayAction::Stop), label, true, None::<&str>)?)?;
            menu.append(&PredefinedMenuItem::separator(app)?)?;
        }
        if model.projects.is_empty() {
            menu.append(&MenuItem::new(app, pick("No active projects", "Keine aktiven Projekte"), false, None::<&str>)?)?;
        } else {
            menu.append(&MenuItem::new(app, pick("Start timer", "Zeiterfassung starten"), false, None::<&str>)?)?;
            for p in &model.projects {
                let label = match &p.customer_name {
                    Some(c) => format!("{} · {c}", p.name),
                    None => p.name.clone(),
                };
                let running = model.running.as_ref().is_some_and(|r| r.project_id == p.id);
                let id = action_id(&TrayAction::Start(p.id.clone()));
                menu.append(&CheckMenuItem::with_id(app, id, label, !running, running, None::<&str>)?)?;
            }
        }
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&MenuItem::with_id(app, action_id(&TrayAction::Open), pick("Open Midnight", "Midnight öffnen"), true, None::<&str>)?)?;
        menu.append(&MenuItem::with_id(app, action_id(&TrayAction::Quit), pick("Quit Midnight", "Midnight beenden"), true, None::<&str>)?)?;
        Ok(menu)
    }

    #[derive(PartialEq)]
    struct View {
        model: TrayModel,
        visible: bool,
        show_time: bool,
        german: bool,
    }

    fn current_view(db: &Db) -> Option<View> {
        let conn = lock(db).ok()?;
        let read = || -> crate::error::AppResult<View> {
            let settings = crate::db::load_settings(&conn)?;
            Ok(View {
                model: tray_model(&conn)?,
                visible: settings.menu_bar_icon,
                show_time: settings.menu_bar_show_time,
                german: crate::i18n::is_de(),
            })
        };
        read().map_err(|e| eprintln!("[tray] {e}")).ok()
    }

    fn render(app: &AppHandle, tray: &TrayIcon, view: &View, last: Option<&View>) {
        if last.is_none_or(|l| l.visible != view.visible) {
            let _ = tray.set_visible(view.visible);
        }
        if last.is_none_or(|l| l.model != view.model || l.german != view.german) {
            match build_menu(app, &view.model) {
                Ok(menu) => {
                    let _ = tray.set_menu(Some(menu));
                }
                Err(e) => eprintln!("[tray] {e}"),
            }
        }
        let title = if view.show_time { tray_title(&view.model, Utc::now()) } else { None };
        let _ = tray.set_title(title);
    }

    pub fn refresh(app: &AppHandle) {
        let (Some(tray), Some(state)) = (app.tray_by_id(TRAY_ID), app.try_state::<AppState>()) else { return };
        if let Some(view) = current_view(&state.db) {
            render(app, &tray, &view, None);
        }
    }

    pub fn show_main(app: &AppHandle) {
        hide_panel(app);
        let _ = app.set_activation_policy(ActivationPolicy::Regular);
        let _ = app.show();
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }

    pub fn hide_to_menu_bar(app: &AppHandle) {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.hide();
        }
        let _ = app.set_activation_policy(ActivationPolicy::Accessory);
    }

    pub fn hide_panel(app: &AppHandle) {
        if let Some(panel) = app.get_webview_window(PANEL) {
            let _ = panel.hide();
        }
    }

    fn build_panel(app: &AppHandle) -> tauri::Result<()> {
        let panel = WebviewWindowBuilder::new(app, PANEL, WebviewUrl::App("index.html".into()))
            .title("Midnight")
            .inner_size(PANEL_SIZE.0, PANEL_SIZE.1)
            .decorations(false)
            .transparent(true)
            .shadow(true)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible_on_all_workspaces(true)
            .focused(false)
            .visible(false)
            .build()?;
        let handle = panel.clone();
        panel.on_window_event(move |event| {
            if let tauri::WindowEvent::Focused(false) = event {
                if handle.is_visible().unwrap_or(false) {
                    mark_blur_hide(Some(std::time::Instant::now()));
                    let _ = handle.hide();
                }
            }
        });
        Ok(())
    }

    fn toggle_panel(app: &AppHandle, icon: Rect) {
        let Some(panel) = app.get_webview_window(PANEL) else { return };
        if panel.is_visible().unwrap_or(false) {
            let _ = panel.hide();
            return;
        }
        let since_blur_hide = PANEL_BLUR_HIDDEN_AT.lock().ok().and_then(|mut last| last.take()).map(|at| at.elapsed());
        if !crate::desktop::panel_may_open(since_blur_hide) {
            return;
        }
        let scale = panel.scale_factor().unwrap_or(2.0);
        let pos = icon.position.to_physical::<f64>(scale);
        let size = icon.size.to_physical::<f64>(scale);
        let width = PANEL_SIZE.0 * scale;
        let screen = app
            .monitor_from_point(pos.x, pos.y)
            .ok()
            .flatten()
            .map(|m| (m.position().x as f64, m.size().width as f64))
            .unwrap_or((0.0, f64::MAX));
        let (x, y) = crate::desktop::panel_position((pos.x, pos.y, size.width, size.height), width, screen, 6.0 * scale);
        let _ = panel.set_position(PhysicalPosition::new(x, y));
        let _ = panel.show();
        let _ = panel.set_focus();
        let _ = panel.emit("menubar-shown", ());
    }

    fn handle(app: &AppHandle, action: TrayAction) {
        let timer_action = match action {
            TrayAction::Open => return show_main(app),
            TrayAction::Quit => return app.exit(0),
            TrayAction::Start(project_id) => Some(project_id),
            TrayAction::Stop => None,
        };
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();
            let result = match timer_action {
                Some(project_id) => crate::tracker::start_timer(&state, project_id, None).await.map(|_| ()),
                None => crate::tracker::stop_timer(&state).await.map(|_| ()),
            };
            if let Err(e) = result {
                eprintln!("[tray] {e}");
            }
            refresh(&app);
            let _ = app.emit(CHANGED_EVENT, ());
        });
    }

    pub fn init(app: &AppHandle, db: Db) -> tauri::Result<()> {
        let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
        let tray = TrayIconBuilder::with_id(TRAY_ID)
            .icon(icon)
            .icon_as_template(true)
            .tooltip("Midnight")
            .show_menu_on_left_click(false)
            .on_menu_event(|app, event| {
                if let Some(action) = parse_action(event.id().as_ref()) {
                    handle(app, action);
                }
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = event {
                    toggle_panel(tray.app_handle(), rect);
                }
            })
            .build(app)?;
        build_panel(app)?;
        let mut last = current_view(&db);
        if let Some(view) = &last {
            render(app, &tray, view, None);
        }

        let app = app.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(POLL);
            let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
            let Some(view) = current_view(&db) else { continue };
            render(&app, &tray, &view, last.as_ref());
            last = Some(view);
        });
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub use menu_bar::{hide_panel, hide_to_menu_bar, init, refresh, show_main};

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ts: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(ts).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn model_lists_active_projects_and_the_running_timer() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO customers (id, name) VALUES ('c', 'Climbing Park');
             INSERT INTO tracker_projects (id, name, customer_id, status, created_at) VALUES
               ('web', 'Website', 'c', 'in_progress', '2026-01-01T00:00:00Z'),
               ('app', 'App', NULL, 'in_progress', '2026-01-02T00:00:00Z'),
               ('old', 'Old', NULL, 'completed', '2026-01-03T00:00:00Z');
             INSERT INTO tracker_entries (id, project_id, date, start) VALUES ('r', 'web', '2026-10-06', '2026-10-06T08:00:00Z');",
        )
        .unwrap();
        let m = tray_model(&conn).unwrap();
        let names: Vec<_> = m.projects.iter().map(|p| (p.name.as_str(), p.customer_name.as_deref())).collect();
        assert_eq!(names, [("Website", Some("Climbing Park")), ("App", None)], "completed projects are hidden");
        assert_eq!(
            m.running,
            Some(TrayRunning { project_id: "web".into(), project_name: "Website".into(), start: "2026-10-06T08:00:00Z".into() })
        );
        assert_eq!(tray_title(&m, at("2026-10-06T09:42:30Z")).as_deref(), Some("1:42"));
    }

    #[test]
    fn idle_model_has_no_title() {
        let conn = crate::db::open_in_memory().unwrap();
        let m = tray_model(&conn).unwrap();
        assert_eq!(m, TrayModel::default());
        assert_eq!(tray_title(&m, Utc::now()), None);
    }

    #[test]
    fn elapsed_time_formats_as_hours_and_minutes() {
        assert_eq!(fmt_elapsed(0), "0:00");
        assert_eq!(fmt_elapsed(59), "0:00");
        assert_eq!(fmt_elapsed(61 * 60), "1:01");
        assert_eq!(fmt_elapsed(10 * 3600 + 5 * 60), "10:05");
        assert_eq!(fmt_elapsed(-5), "0:00");
        assert_eq!(elapsed_secs("2026-10-06T10:00:00+02:00", at("2026-10-06T08:30:00Z")), 1800);
        assert_eq!(elapsed_secs("garbage", Utc::now()), 0);
        assert_eq!(elapsed_secs("2026-10-06T10:00:00Z", at("2026-10-06T09:00:00Z")), 0, "clock skew never goes negative");
    }

    #[test]
    fn menu_ids_round_trip() {
        for action in [TrayAction::Start("abc-123".into()), TrayAction::Stop, TrayAction::Open, TrayAction::Quit] {
            assert_eq!(parse_action(&action_id(&action)), Some(action));
        }
        assert_eq!(parse_action("start:"), None);
        assert_eq!(parse_action("nope"), None);
    }
}
