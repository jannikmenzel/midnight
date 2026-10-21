use std::time::{Duration, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::desktop::settings_of;
use crate::error::{AppResult, Context};
use crate::i18n::tr;

const INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const RETRY: Duration = Duration::from_secs(15 * 60);
const TICK: Duration = Duration::from_secs(60);
const FAKE_UPDATE: &str = "MIDNIGHT_FAKE_UPDATE";
const AVAILABLE_EVENT: &str = "update-available";

#[derive(Default)]
pub struct UpdateState {
    running: tokio::sync::Mutex<()>,
    available: std::sync::Mutex<Option<Release>>,
}

impl UpdateState {
    fn available(&self) -> Option<Release> {
        self.available.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set_available(&self, release: Option<Release>) {
        *self.available.lock().unwrap_or_else(|e| e.into_inner()) = release;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Release {
    pub version: String,
    pub notes: Vec<NoteSection>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NoteSection {
    pub title: String,
    pub items: Vec<String>,
}

#[derive(Serialize)]
pub struct UpdateStatus {
    pub current: String,
    pub available: Option<Release>,
}

pub fn parse_notes(markdown: &str) -> Vec<NoteSection> {
    let mut sections: Vec<NoteSection> = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let title = trimmed.trim_start_matches('#').trim().to_string();
            sections.push(NoteSection { title, items: Vec::new() });
            continue;
        }
        let item = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* "));
        if item.is_none() && trimmed.is_empty() {
            continue;
        }
        if sections.is_empty() {
            sections.push(NoteSection { title: String::new(), items: Vec::new() });
        }
        let items = &mut sections.last_mut().expect("pushed above").items;
        if let Some(item) = item {
            items.push(item.trim().to_string());
        } else if let Some(last) = items.last_mut() {
            last.push(' ');
            last.push_str(trimmed);
        }
    }
    sections.retain(|s| !s.items.is_empty());
    sections
}

#[cfg(any(debug_assertions, test))]
fn latest_changelog_notes(changelog: &str) -> &str {
    changelog
        .split("\n## ")
        .skip(1)
        .map(|section| section.split_once('\n').map_or("", |(_, body)| body).trim())
        .find(|body| !body.is_empty())
        .unwrap_or_default()
}

fn is_new(announced: Option<&Release>, version: &str) -> bool {
    announced.is_none_or(|r| r.version != version)
}

#[cfg(debug_assertions)]
fn fake_version() -> Option<String> {
    std::env::var(FAKE_UPDATE).ok()
}

#[cfg(debug_assertions)]
async fn pause() -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(|| std::thread::sleep(Duration::from_secs(2))).await.ctx("Update")
}

#[cfg(debug_assertions)]
async fn fetch(_app: &AppHandle) -> AppResult<Option<Release>> {
    let Some(version) = fake_version() else {
        return Ok(None);
    };
    pause().await?;
    let changelog = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../CHANGELOG.md")).unwrap_or_default();
    Ok(Some(Release { version, notes: parse_notes(latest_changelog_notes(&changelog)) }))
}

#[cfg(debug_assertions)]
async fn download_and_install(_app: &AppHandle) -> AppResult<()> {
    if fake_version().is_none() {
        return Err(crate::error::msg(tr!("No update available", "Kein Update verfügbar")));
    }
    pause().await
}

#[cfg(not(debug_assertions))]
async fn fetch(app: &AppHandle) -> AppResult<Option<Release>> {
    use tauri_plugin_updater::UpdaterExt;
    let update = app.updater().ctx("Update")?.check().await.ctx("Update")?;
    Ok(update.map(|u| Release { notes: parse_notes(u.body.as_deref().unwrap_or_default()), version: u.version }))
}

#[cfg(not(debug_assertions))]
async fn download_and_install(app: &AppHandle) -> AppResult<()> {
    use tauri_plugin_updater::UpdaterExt;
    let Some(update) = app.updater().ctx("Update")?.check().await.ctx("Update")? else {
        return Err(crate::error::msg(tr!("No update available", "Kein Update verfügbar")));
    };
    update.download_and_install(|_, _| {}, || {}).await.ctx("Update")
}

async fn check(app: &AppHandle) -> AppResult<Option<Release>> {
    let state = app.state::<UpdateState>();
    let _running = state.running.lock().await;
    let Some(release) = fetch(app).await? else {
        state.set_available(None);
        return Ok(None);
    };
    if is_new(state.available().as_ref(), &release.version) {
        state.set_available(Some(release.clone()));
        let _ = app.emit(AVAILABLE_EVENT, &release);
    }
    Ok(Some(release))
}

fn next_check(now: SystemTime, succeeded: bool) -> SystemTime {
    now + if succeeded { INTERVAL } else { RETRY }
}

pub fn spawn(app: AppHandle) {
    if cfg!(debug_assertions) && std::env::var_os(FAKE_UPDATE).is_none() {
        return;
    }
    std::thread::spawn(move || {
        let mut due = SystemTime::UNIX_EPOCH;
        loop {
            let now = SystemTime::now();
            if now >= due && settings_of(&app).is_some_and(|s| s.check_updates) {
                let result = tauri::async_runtime::block_on(check(&app));
                if let Err(e) = &result {
                    eprintln!("[updater] {e}");
                }
                due = next_check(now, result.is_ok());
            }
            std::thread::sleep(TICK);
        }
    });
}

#[tauri::command]
pub fn update_status(app: AppHandle) -> UpdateStatus {
    UpdateStatus { current: app.package_info().version.to_string(), available: app.state::<UpdateState>().available() }
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> AppResult<Option<Release>> {
    check(&app).await
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> AppResult<()> {
    let state = app.state::<UpdateState>();
    let _running = state.running.lock().await;
    download_and_install(&app).await?;
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(title: &str, items: &[&str]) -> NoteSection {
        NoteSection { title: title.into(), items: items.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn notes_are_grouped_by_heading() {
        let notes = "### Added\n\n- List view in time tracking\n- Automatic updates\n\n### Changed\n\n- Forecast restructured\n";
        assert_eq!(
            parse_notes(notes),
            vec![section("Added", &["List view in time tracking", "Automatic updates"]), section("Changed", &["Forecast restructured"])]
        );
    }

    #[test]
    fn wrapped_lines_continue_the_previous_item() {
        let notes = "### Fixed\n- A long entry that\n  wraps onto the next line\n* Star bullet";
        assert_eq!(parse_notes(notes), vec![section("Fixed", &["A long entry that wraps onto the next line", "Star bullet"])]);
    }

    #[test]
    fn notes_without_headings_or_items_stay_usable() {
        assert_eq!(parse_notes("- Only an item"), vec![section("", &["Only an item"])]);
        assert_eq!(parse_notes("### Added\n\n### Changed\n- Something"), vec![section("Changed", &["Something"])], "empty sections are dropped");
        assert!(parse_notes("").is_empty());
        assert!(parse_notes("See CHANGELOG.md for details.").is_empty(), "plain text without items shows no notes");
    }

    #[test]
    fn fake_update_uses_the_first_filled_changelog_section() {
        let changelog = "# Changelog\n\nIntro\n\n## [Unreleased]\n\n## [0.1.2] - 2026-10-18\n\n### Added\n\n- List view\n\n## [0.1.1]\n\n- Older\n";
        assert_eq!(latest_changelog_notes(changelog), "### Added\n\n- List view");
        assert_eq!(latest_changelog_notes("# Changelog\n\n## [Unreleased]\n"), "");
    }

    #[test]
    fn failed_checks_are_retried_sooner() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert_eq!(next_check(now, true), now + INTERVAL);
        assert_eq!(next_check(now, false), now + RETRY);
        assert!(RETRY < INTERVAL);
    }

    #[test]
    fn each_version_is_announced_only_once() {
        let announced = Release { version: "0.2.0".into(), notes: Vec::new() };
        assert!(is_new(None, "0.2.0"));
        assert!(!is_new(Some(&announced), "0.2.0"));
        assert!(is_new(Some(&announced), "0.2.1"), "a newer release is announced again");
    }
}
