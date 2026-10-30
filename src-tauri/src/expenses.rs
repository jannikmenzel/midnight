use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::i18n::tr;
use crate::{activities, vault};
use crate::db::{load_settings, AppState};
use crate::error::{msg, AppResult};
use crate::extract::extract;
use crate::files::{extract_text, remove_file_quietly, store_file};
use crate::util::{fmt_money, new_id, now_ts, parse_date};

#[derive(Debug, Serialize)]
pub struct Expense {
    pub id: String,
    pub date: String,
    pub name: String,
    pub description: Option<String>,
    pub amount_cents: i64,
    pub currency: String,
    pub category_slug: Option<String>,
    pub category_name: Option<String>,
    pub category_color: Option<String>,
    pub note: Option<String>,
    pub receipt_count: i64,
    pub created_at: String,
    pub expense_recurring_id: Option<String>,
    pub deductible_percent: i64,
}

const SELECT: &str = "SELECT e.id, e.date, e.name, e.description, e.amount_cents, e.currency, e.category_slug, c.name, c.color,
    e.note, (SELECT COUNT(*) FROM receipts r WHERE r.expense_id = e.id), e.created_at, e.expense_recurring_id, e.deductible_percent
  FROM expenses e LEFT JOIN categories c ON c.slug = e.category_slug";

fn map_row(r: &rusqlite::Row) -> rusqlite::Result<Expense> {
    Ok(Expense {
        id: r.get(0)?,
        date: r.get(1)?,
        name: r.get(2)?,
        description: r.get(3)?,
        amount_cents: r.get(4)?,
        currency: r.get(5)?,
        category_slug: r.get(6)?,
        category_name: r.get(7)?,
        category_color: r.get(8)?,
        note: r.get(9)?,
        receipt_count: r.get(10)?,
        created_at: r.get(11)?,
        expense_recurring_id: r.get(12)?,
        deductible_percent: r.get(13)?,
    })
}

pub fn load_expense(conn: &Connection, id: &str) -> AppResult<Expense> {
    conn.query_row(&format!("{SELECT} WHERE e.id = ?1"), params![id], map_row)
        .optional()?
        .ok_or_else(|| msg(tr!("Expense not found", "Ausgabe nicht gefunden")))
}

pub async fn get_expense(state: &AppState, id: String) -> AppResult<Expense> {
    let conn = state.conn()?;
    load_expense(&conn, &id)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExpenseInput {
    pub id: Option<String>,
    pub date: String,
    pub name: String,
    pub description: Option<String>,
    pub amount_cents: i64,
    pub currency: Option<String>,
    pub category_slug: Option<String>,
    pub note: Option<String>,
    #[schemars(description = "Deductible share in percent, 0 to 100. Without it, a new expense gets the suggestion of its category (business meals: the value from the settings, otherwise 100) and an existing expense keeps its share.")]
    pub deductible_percent: Option<i64>,
}

fn suggested_deductible_percent(conn: &Connection, category: Option<&str>) -> AppResult<i64> {
    Ok(if category == Some("meals") { load_settings(conn)?.limits.meals_deductible_percent.clamp(0, 100) } else { 100 })
}

pub fn upsert_expense(conn: &Connection, input: &ExpenseInput) -> AppResult<String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(msg(tr!("Please describe what the expense was for", "Bitte gib an, wofür die Ausgabe war")));
    }
    if parse_date(&input.date).is_none() {
        return Err(msg(tr!("Invalid date", "Ungültiges Datum")));
    }
    if input.amount_cents <= 0 {
        return Err(msg(tr!("The amount must be greater than 0", "Der Betrag muss größer als 0 sein")));
    }
    if input.deductible_percent.is_some_and(|p| !(0..=100).contains(&p)) {
        return Err(msg(tr!("The deductible share must be between 0 and 100", "Der abziehbare Anteil muss zwischen 0 und 100 liegen")));
    }
    let id = input.id.clone().unwrap_or_else(new_id);
    let currency = input.currency.clone().filter(|c| c.len() == 3).unwrap_or_else(|| "EUR".into());
    let category = input.category_slug.clone().filter(|s| !s.is_empty());
    let blank = |v: &Option<String>| v.clone().filter(|s| !s.trim().is_empty());
    let percent = match input.deductible_percent {
        Some(percent) => percent,
        None => {
            let stored: Option<i64> = conn.query_row("SELECT deductible_percent FROM expenses WHERE id = ?1", params![id], |r| r.get(0)).optional()?;
            match stored {
                Some(percent) => percent,
                None => suggested_deductible_percent(conn, category.as_deref())?,
            }
        }
    };
    conn.execute(
        "INSERT INTO expenses (id, date, name, description, amount_cents, currency, category_slug, note, deductible_percent)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?10)
         ON CONFLICT(id) DO UPDATE SET date = excluded.date, name = excluded.name, description = excluded.description,
            amount_cents = excluded.amount_cents, currency = excluded.currency, category_slug = excluded.category_slug,
            note = excluded.note, deductible_percent = excluded.deductible_percent, updated_at = ?9",
        params![id, input.date, name, blank(&input.description), input.amount_cents, currency, category, blank(&input.note), now_ts(), percent],
    )?;
    Ok(id)
}

pub async fn save_expense(state: &AppState, input: ExpenseInput) -> AppResult<Expense> {
    let conn = state.conn()?;
    let is_new = input.id.is_none();
    let id = upsert_expense(&conn, &input)?;
    conn.execute(
        "UPDATE documents SET date = (SELECT date FROM expenses WHERE id = ?1)
         WHERE receipt_id IN (SELECT id FROM receipts WHERE expense_id = ?1)",
        params![id],
    )?;
    let expense = load_expense(&conn, &id)?;
    if is_new {
        activities::log(
            &conn,
            "expense_created",
            &tr!("Expense \"{}\" recorded ({})", "Ausgabe „{}“ erfasst ({})", expense.name, fmt_money(expense.amount_cents, &expense.currency)),
            None,
        )?;
    }
    Ok(expense)
}

pub async fn delete_expense(state: &AppState, id: String) -> AppResult<()> {
    let conn = state.conn()?;
    let paths: Vec<String> = {
        let mut stmt = conn.prepare("SELECT path FROM receipts WHERE expense_id = ?1")?;
        let rows = stmt.query_map(params![id], |r| r.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    conn.execute("DELETE FROM expenses WHERE id = ?1", params![id])?;
    vault::prune_tags(&conn)?;
    paths.iter().for_each(|p| remove_file_quietly(p));
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct Receipt {
    pub id: String,
    pub expense_id: String,
    pub name: String,
    pub path: String,
    pub content_type: Option<String>,
    pub size: i64,
    pub created_at: String,
}

pub async fn list_receipts(state: &AppState, expense_id: String) -> AppResult<Vec<Receipt>> {
    let conn = state.conn()?;
    let mut stmt = conn.prepare(
        "SELECT id, expense_id, name, path, content_type, size, created_at FROM receipts WHERE expense_id = ?1 ORDER BY created_at",
    )?;
    let rows = stmt.query_map(params![expense_id], |r| {
        Ok(Receipt {
            id: r.get(0)?,
            expense_id: r.get(1)?,
            name: r.get(2)?,
            path: r.get(3)?,
            content_type: r.get(4)?,
            size: r.get(5)?,
            created_at: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub async fn add_receipts(state: &AppState, expense_id: String, paths: Vec<String>) -> AppResult<usize> {
    let stored = paths
        .iter()
        .map(|p| store_file(&state.files_dir, "receipts", p))
        .collect::<AppResult<Vec<_>>>()?;
    let conn = state.conn()?;
    for f in &stored {
        let receipt_id = new_id();
        conn.execute(
            "INSERT INTO receipts (id, expense_id, name, path, content_type, size) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![receipt_id, expense_id, f.name, f.path, f.content_type, f.size],
        )?;
        vault::sync_receipt(&conn, &receipt_id)?;
    }
    Ok(stored.len())
}

pub async fn delete_receipt(state: &AppState, id: String) -> AppResult<()> {
    let conn = state.conn()?;
    let path: String = conn.query_row("SELECT path FROM receipts WHERE id = ?1", params![id], |r| r.get(0))?;
    conn.execute("DELETE FROM receipts WHERE id = ?1", params![id])?;
    vault::prune_tags(&conn)?;
    remove_file_quietly(&path);
    Ok(())
}

#[derive(Debug, Serialize, Default)]
pub struct ReceiptDraft {
    pub name: Option<String>,
    pub amount_cents: Option<i64>,
    pub currency: Option<String>,
    pub date: Option<String>,
}

pub async fn extract_receipt(path: String) -> AppResult<ReceiptDraft> {
    let Some(text) = extract_text(&path) else { return Ok(ReceiptDraft::default()) };
    let found = extract(&text);
    Ok(ReceiptDraft { name: found.vendor, amount_cents: found.amount_cents, currency: found.currency, date: found.date })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(amount: i64) -> ExpenseInput {
        ExpenseInput {
            id: None,
            date: "2026-10-03".into(),
            name: "Figma".into(),
            description: Some("  ".into()),
            amount_cents: amount,
            currency: None,
            category_slug: Some("software".into()),
            note: None,
            deductible_percent: None,
        }
    }

    #[test]
    fn saves_and_validates_expenses() {
        let conn = crate::db::open_in_memory().unwrap();
        let id = upsert_expense(&conn, &input(1440)).unwrap();
        let e = load_expense(&conn, &id).unwrap();
        assert_eq!(e.amount_cents, 1440);
        assert_eq!(e.category_name.as_deref(), Some("Software & subscriptions"));
        assert_eq!(e.description, None, "blank text is stored as NULL");
        assert!(upsert_expense(&conn, &input(0)).is_err());
        assert!(upsert_expense(&conn, &input(-500)).is_err());
    }

    #[test]
    fn stores_the_given_deductible_share() {
        let conn = crate::db::open_in_memory().unwrap();
        for percent in [0, 50, 100] {
            let id = upsert_expense(&conn, &ExpenseInput { deductible_percent: Some(percent), ..input(1000) }).unwrap();
            assert_eq!(load_expense(&conn, &id).unwrap().deductible_percent, percent);
        }
        for percent in [-1, 101] {
            assert!(upsert_expense(&conn, &ExpenseInput { deductible_percent: Some(percent), ..input(1000) }).is_err());
        }
    }

    #[test]
    fn a_missing_share_uses_the_suggestion_of_the_category() {
        let conn = crate::db::open_in_memory().unwrap();
        let mut settings = crate::db::load_settings(&conn).unwrap();
        settings.limits.meals_deductible_percent = 60;
        crate::db::store_settings(&conn, &settings).unwrap();

        let software = upsert_expense(&conn, &input(1000)).unwrap();
        assert_eq!(load_expense(&conn, &software).unwrap().deductible_percent, 100);
        let meals = upsert_expense(&conn, &ExpenseInput { category_slug: Some("meals".into()), ..input(1000) }).unwrap();
        assert_eq!(load_expense(&conn, &meals).unwrap().deductible_percent, 60);
    }

    #[test]
    fn updating_without_a_share_keeps_the_stored_one() {
        let conn = crate::db::open_in_memory().unwrap();
        let id = upsert_expense(&conn, &ExpenseInput { deductible_percent: Some(25), ..input(1000) }).unwrap();
        upsert_expense(&conn, &ExpenseInput { id: Some(id.clone()), category_slug: Some("meals".into()), ..input(2000) }).unwrap();
        let e = load_expense(&conn, &id).unwrap();
        assert_eq!((e.amount_cents, e.deductible_percent), (2000, 25));
    }
}
