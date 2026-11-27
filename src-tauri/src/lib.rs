mod activities;
mod categories;
mod commands;
mod customers;
mod db;
mod desktop;
mod einvoice;
mod error;
mod expense_recurring;
mod expenses;
mod extract;
mod files;
mod forecast;
mod i18n;
mod invoices;
mod jobs;
mod latex;
mod ledger;
mod mcp;
mod mcp_setup;
mod migrations;
mod places;
mod products;
mod reports;
mod schedule;
mod search;
mod security;
mod settings;
mod tax;
mod tax_export;
mod templates;
mod tracker;
#[cfg(any(target_os = "macos", test))]
mod tray;
mod units;
mod updater;
mod util;
mod vat;
mod vault;

use tauri::Manager;

pub fn run_mcp() {
    if let Err(e) = mcp::serve_stdio() {
        eprintln!("[mcp] {e}");
        std::process::exit(1);
    }
}

#[cfg(target_os = "macos")]
fn app_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem as P, Submenu};
    let info = app.package_info();
    let about = AboutMetadata {
        name: Some(info.name.clone()),
        version: Some(info.version.to_string()),
        ..Default::default()
    };
    Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                &info.name,
                true,
                &[
                    &P::about(app, None, Some(about))?,
                    &P::separator(app)?,
                    &P::services(app, None)?,
                    &P::separator(app)?,
                    &P::hide(app, None)?,
                    &P::hide_others(app, None)?,
                    &P::separator(app)?,
                    &MenuItem::with_id(
                        app,
                        APP_QUIT,
                        i18n::pick("Quit Midnight", "Midnight beenden"),
                        true,
                        Some("CmdOrCtrl+Q"),
                    )?,
                    &MenuItem::with_id(
                        app,
                        APP_QUIT_ALL,
                        i18n::pick("Quit Completely", "Vollständig beenden"),
                        true,
                        Some("Alt+CmdOrCtrl+Q"),
                    )?,
                ],
            )?,
            &Submenu::with_items(
                app,
                i18n::pick("File", "Ablage"),
                true,
                &[&P::close_window(app, None)?],
            )?,
            &Submenu::with_items(
                app,
                i18n::pick("Edit", "Bearbeiten"),
                true,
                &[
                    &P::undo(app, None)?,
                    &P::redo(app, None)?,
                    &P::separator(app)?,
                    &P::cut(app, None)?,
                    &P::copy(app, None)?,
                    &P::paste(app, None)?,
                    &P::select_all(app, None)?,
                ],
            )?,
            &Submenu::with_items(
                app,
                i18n::pick("View", "Darstellung"),
                true,
                &[&P::fullscreen(app, None)?],
            )?,
            &Submenu::with_items(
                app,
                i18n::pick("Window", "Fenster"),
                true,
                &[
                    &P::minimize(app, None)?,
                    &P::maximize(app, None)?,
                    &P::separator(app)?,
                    &P::close_window(app, None)?,
                ],
            )?,
        ],
    )
}

#[cfg(target_os = "macos")]
const APP_QUIT: &str = "app-quit";
#[cfg(target_os = "macos")]
const APP_QUIT_ALL: &str = "app-quit-all";

fn close_or_quit(app: &tauri::AppHandle) {
    let keep = desktop::settings_of(app).is_some_and(|s| desktop::keeps_running_on_close(&s));
    #[cfg(target_os = "macos")]
    if keep {
        return tray::hide_to_menu_bar(app);
    }
    let _ = keep;
    app.exit(0);
}

static MAIN_REVEAL: std::sync::Mutex<(bool, bool)> = std::sync::Mutex::new((false, false));

fn reveal_main(app: &tauri::AppHandle, loaded: bool, requested: bool) {
    let mut state = MAIN_REVEAL.lock().unwrap_or_else(|e| e.into_inner());
    state.0 |= loaded;
    state.1 |= requested;
    if state.0 && state.1 {
        state.1 = false;
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
        }
    }
}

pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        desktop::open_main_window(app.clone())
    }));
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![desktop::BACKGROUND_ARG]),
        ))
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let conn = db::open(&data_dir.join("midnight.db"))?;
            let settings = db::load_settings(&conn)?;
            i18n::set_language(&settings.language);
            db::localize_default_categories(&conn)?;
            templates::init(&data_dir.join("files"), db::legacy_latex_dir(&conn)?)?;
            let state = db::AppState::new(conn, data_dir.join("files"))?;
            jobs::spawn(state.db.clone(), state.files_dir.clone());
            #[cfg(target_os = "macos")]
            {
                tray::init(app.handle(), state.db.clone())?;
                app.set_menu(app_menu(app.handle())?)?;
            }
            app.manage(state);
            app.manage(updater::UpdateState::default());
            updater::spawn(app.handle().clone());
            let args: Vec<String> = std::env::args().collect();
            if desktop::starts_in_background(&args, &settings) {
                #[cfg(target_os = "macos")]
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            } else {
                reveal_main(app.handle(), false, true);
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    reveal_main(&handle, true, false);
                });
            }
            Ok(())
        })
        .on_page_load(|webview, payload| {
            if webview.label() == "main"
                && payload.event() == tauri::webview::PageLoadEvent::Finished
            {
                reveal_main(webview.app_handle(), true, false);
            }
        })
        .on_menu_event(|app, event| {
            #[cfg(target_os = "macos")]
            match event.id().as_ref() {
                APP_QUIT => close_or_quit(app),
                APP_QUIT_ALL => app.exit(0),
                _ => {}
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    close_or_quit(window.app_handle());
                }
            }
        })
        .invoke_handler(commands::handler())
        .build(tauri::generate_context!())
        .expect("error while running Midnight")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                tray::show_main(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}
