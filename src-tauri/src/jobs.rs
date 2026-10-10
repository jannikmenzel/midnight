use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::i18n::tr;
use crate::db::{load_settings, lock, AppState, Db};
use crate::error::AppResult;
use crate::{activities, expense_recurring, invoices, tracker, vault};

const INTERVAL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Serialize, Default)]
pub struct JobReport {
    pub overdue: Vec<String>,
    pub expenses_created: Vec<String>,
}

pub fn run_once(db: &Db, files_dir: &Path) -> AppResult<JobReport> {
    let conn = lock(db)?;
    let notify = load_settings(&conn)?.notify_overdue;
    let overdue = invoices::mark_overdue(&conn).and_then(|numbers| {
        if notify {
            for number in &numbers {
                activities::log(&conn, "invoice_overdue", &tr!("Invoice {number} is overdue", "Rechnung {number} ist überfällig"), None)?;
            }
        }
        Ok(numbers)
    });
    let expenses_created = expense_recurring::run_due(&conn);
    if let Err(e) = tracker::check_weekly_limits(&conn) {
        eprintln!("[jobs] weekly limit: {e}");
    }
    if let Err(e) = vault::backfill(&conn, files_dir) {
        eprintln!("[jobs] vault sync: {e}");
    }
    Ok(JobReport { overdue: overdue?, expenses_created: expenses_created? })
}

pub fn spawn(db: Db, files_dir: PathBuf) {
    std::thread::spawn(move || loop {
        if let Err(e) = run_once(&db, &files_dir) {
            eprintln!("[jobs] {e}");
        }
        std::thread::sleep(INTERVAL);
    });
}

pub async fn run_jobs_now(state: &AppState) -> AppResult<JobReport> {
    run_once(&state.db, &state.files_dir)
}
