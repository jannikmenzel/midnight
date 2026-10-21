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
mod i18n;
mod invoices;
mod jobs;
mod latex;
mod ledger;
mod mcp;
mod mcp_setup;
mod migrations;
mod products;
mod reports;
mod schedule;
mod search;
mod settings;
mod tax;
mod templates;
mod tracker;
mod tray;
mod units;
mod updater;
mod util;
mod vat;
mod vault;
mod forecast;
mod places;

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
    let about = AboutMetadata { name: Some(info.name.clone()), version: Some(info.version.to_string()), ..Default::default() };
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
                    &MenuItem::with_id(app, APP_QUIT, i18n::pick("Quit Midnight", "Midnight beenden"), true, Some("CmdOrCtrl+Q"))?,
                    &MenuItem::with_id(app, APP_QUIT_ALL, i18n::pick("Quit Completely", "Vollständig beenden"), true, Some("Alt+CmdOrCtrl+Q"))?,
                ],
            )?,
            &Submenu::with_items(app, i18n::pick("File", "Ablage"), true, &[&P::close_window(app, None)?])?,
            &Submenu::with_items(
                app,
                i18n::pick("Edit", "Bearbeiten"),
                true,
                &[&P::undo(app, None)?, &P::redo(app, None)?, &P::separator(app)?, &P::cut(app, None)?, &P::copy(app, None)?, &P::paste(app, None)?, &P::select_all(app, None)?],
            )?,
            &Submenu::with_items(app, i18n::pick("View", "Darstellung"), true, &[&P::fullscreen(app, None)?])?,
            &Submenu::with_items(
                app,
                i18n::pick("Window", "Fenster"),
                true,
                &[&P::minimize(app, None)?, &P::maximize(app, None)?, &P::separator(app)?, &P::close_window(app, None)?],
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
    tauri::Builder::default()
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
            if webview.label() == "main" && payload.event() == tauri::webview::PageLoadEvent::Finished {
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
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::data_location,
            commands::reset_all_data,
            commands::mcp_setup,
            commands::mcp_install_claude_desktop,
            desktop::refresh_menu_bar,
            desktop::open_main_window,
            desktop::hide_menu_bar_panel,
            desktop::quit_app,
            desktop::autostart_enabled,
            desktop::set_autostart,
            updater::update_status,
            updater::check_update,
            updater::install_update,
            files::open_path,
            files::reveal_data_dir,
            files::reveal_latex_dir,
            commands::list_ledger,
            commands::export_ledger_csv,
            commands::get_expense,
            commands::save_expense,
            commands::delete_expense,
            commands::list_receipts,
            commands::add_receipts,
            commands::delete_receipt,
            commands::extract_receipt,
            commands::list_categories,
            commands::save_category,
            commands::delete_category,
            commands::list_customers,
            commands::save_customer,
            commands::delete_customer,
            commands::list_projects,
            commands::save_project,
            commands::delete_project,
            commands::list_entries,
            commands::running_timer,
            commands::weekly_hours,
            commands::start_timer,
            commands::stop_timer,
            commands::save_entry,
            commands::set_home_office,
            commands::delete_entry,
            commands::invoice_lines_from_tracker,
            commands::invoice_unbilled_for_customer,
            commands::export_timesheet,
            commands::get_tax_report,
            commands::default_tax_params,
            commands::tax_years,
            commands::save_tax_input,
            commands::list_invoices,
            commands::get_invoice,
            commands::next_invoice_number,
            commands::save_invoice,
            commands::set_invoice_status,
            commands::delete_invoice,
            commands::duplicate_invoice,
            commands::list_units,
            commands::render_invoice_pdf,
            commands::render_invoice_timesheet,
            commands::export_invoice_pdf,
            commands::export_invoice_xrechnung,
            commands::latex_files,
            commands::latex_read,
            commands::latex_write,
            commands::latex_reset,
            commands::latex_import,
            commands::latex_delete,
            commands::latex_preview,
            commands::list_products,
            commands::save_product,
            commands::touch_product,
            commands::delete_product,
            commands::list_expense_recurring,
            commands::save_expense_recurring,
            commands::set_expense_recurring_status,
            commands::delete_expense_recurring,
            commands::vault_upload,
            commands::list_documents,
            commands::update_document,
            commands::rename_document,
            commands::delete_document,
            commands::list_tags,
            commands::tax_forecast,
            commands::list_countries,
            commands::suggest_places,
            commands::report_summary,
            commands::report_monthly,
            commands::report_categories,
            commands::report_analysis,
            commands::global_search,
            commands::list_activities,
            commands::mark_activities_read,
            commands::clear_activities,
            commands::run_jobs_now,
        ])
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
