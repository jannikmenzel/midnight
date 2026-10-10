use std::path::{Path, PathBuf};

use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::i18n::tr;
use crate::db::AppState;
use crate::error::{msg, AppResult, Context};
use crate::util::{content_type_for, new_id, safe_file_name};

pub struct StoredFile {
    pub path: String,
    pub name: String,
    pub size: i64,
    pub content_type: String,
}

pub fn store_file(files_dir: &Path, sub: &str, src: &str) -> AppResult<StoredFile> {
    let src_path = PathBuf::from(src);
    let name = src_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| msg(tr!("Invalid file path: {src}", "Ungültiger Dateipfad: {src}")))?;
    let bytes = std::fs::read(&src_path).ctx(&tr!("Reading file {name}", "Datei {name} lesen"))?;
    store_bytes(files_dir, sub, &name, &bytes)
}

pub fn store_bytes(files_dir: &Path, sub: &str, name: &str, bytes: &[u8]) -> AppResult<StoredFile> {
    let safe = safe_file_name(name);
    let short_id: String = new_id().chars().take(8).collect();
    let dest = files_dir.join(sub).join(format!("{short_id}-{safe}"));
    std::fs::write(&dest, bytes)?;
    Ok(StoredFile {
        path: dest.to_string_lossy().to_string(),
        name: name.to_string(),
        size: bytes.len() as i64,
        content_type: content_type_for(name).to_string(),
    })
}

pub fn extract_text(path: &str) -> Option<String> {
    let lower = path.to_lowercase();
    let text = if lower.ends_with(".pdf") {
        let bytes = std::fs::read(path).ok()?;
        std::panic::catch_unwind(move || pdf_extract::extract_text_from_mem(&bytes).ok())
            .ok()
            .flatten()?
    } else if lower.ends_with(".txt") || lower.ends_with(".md") || lower.ends_with(".csv") || lower.ends_with(".xml") {
        std::fs::read_to_string(path).ok()?
    } else {
        return None;
    };
    let trimmed = text.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

pub fn remove_file_quietly(path: &str) {
    let _ = std::fs::remove_file(path);
}

#[tauri::command]
pub async fn open_path(app: AppHandle, state: State<'_, AppState>, path: String) -> AppResult<()> {
    let canonical = std::fs::canonicalize(&path).ctx(&tr!("File not found", "Datei nicht gefunden"))?;
    let root = std::fs::canonicalize(&state.files_dir).ctx(&tr!("Files folder is missing", "Dateiordner fehlt"))?;
    if !canonical.starts_with(&root) {
        return Err(msg(tr!("Path is outside the app data folder", "Pfad liegt außerhalb des App-Datenordners")));
    }
    app.opener()
        .open_path(canonical.to_string_lossy().to_string(), None::<&str>)
        .ctx(&tr!("Opening file", "Datei öffnen"))
}

#[tauri::command]
pub async fn reveal_latex_dir(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let dir = crate::templates::ensure(&state.files_dir)?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .ctx(&tr!("Opening folder", "Ordner öffnen"))
}

#[tauri::command]
pub async fn reveal_data_dir(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    app.opener()
        .open_path(state.files_dir.to_string_lossy().to_string(), None::<&str>)
        .ctx(&tr!("Opening folder", "Ordner öffnen"))
}
