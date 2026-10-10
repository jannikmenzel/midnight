
use crate::db::{self, load_settings, store_settings, AppState, Settings};
use crate::error::AppResult;

pub async fn reset_all_data(state: &AppState) -> AppResult<()> {
    let conn = state.conn()?;
    db::reset(&conn)?;
    if state.files_dir.exists() {
        std::fs::remove_dir_all(&state.files_dir)?;
    }
    db::ensure_file_dirs(&state.files_dir)?;
    crate::i18n::set_language(&Settings::default().language);
    Ok(())
}

pub async fn get_settings(state: &AppState) -> AppResult<Settings> {
    let conn = state.conn()?;
    load_settings(&conn)
}

pub async fn save_settings(state: &AppState, settings: Settings) -> AppResult<Settings> {
    let conn = state.conn()?;
    store_settings(&conn, &settings)?;
    crate::i18n::set_language(&settings.language);
    db::localize_default_categories(&conn)?;
    crate::tracker::check_weekly_limits(&conn)?;
    Ok(settings)
}

pub async fn data_location(state: &AppState) -> AppResult<String> {
    Ok(state.files_dir.parent().unwrap_or(&state.files_dir).to_string_lossy().to_string())
}
