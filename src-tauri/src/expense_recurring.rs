use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::activities;
use crate::db::AppState;
use crate::error::{msg, AppResult};
use crate::expenses::{load_expense, upsert_expense, ExpenseInput};
use crate::i18n::tr;
use crate::schedule::{next_date, series_finished, set_series_status, validate_schedule};
use crate::util::{fmt_money, iso, parse_date, today_date};

#[derive(Debug, Serialize)]
pub struct ExpenseRecurring {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub amount_cents: i64,
    pub currency: String,
    pub category_slug: Option<String>,
    pub category_name: Option<String>,
    pub category_color: Option<String>,
    pub frequency: String,
    pub frequency_interval: Option<i64>,
    pub end_type: String,
    pub end_date: Option<String>,
    pub end_count: Option<i64>,
    pub status: String,
    pub expenses_generated: i64,
    pub next_scheduled_at: String,
    pub last_generated_at: Option<String>,
}

pub async fn list_expense_recurring(state: &AppState) -> AppResult<Vec<ExpenseRecurring>> {
    let conn = state.conn()?;
    let mut stmt = conn.prepare(
        "SELECT r.id, r.name, r.description, r.amount_cents, r.currency, r.category_slug, c.name, c.color, r.frequency,
                r.frequency_interval, r.end_type, r.end_date, r.end_count, r.status, r.expenses_generated,
                r.next_scheduled_at, r.last_generated_at
         FROM expense_recurring r LEFT JOIN categories c ON c.slug = r.category_slug
         ORDER BY r.status, r.next_scheduled_at",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ExpenseRecurring {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            amount_cents: r.get(3)?,
            currency: r.get(4)?,
            category_slug: r.get(5)?,
            category_name: r.get(6)?,
            category_color: r.get(7)?,
            frequency: r.get(8)?,
            frequency_interval: r.get(9)?,
            end_type: r.get(10)?,
            end_date: r.get(11)?,
            end_count: r.get(12)?,
            status: r.get(13)?,
            expenses_generated: r.get(14)?,
            next_scheduled_at: r.get(15)?,
            last_generated_at: r.get(16)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExpenseRecurringInput {
    #[schemars(description = "The expense that becomes the first one of the series and the template for the next ones.")]
    pub expense_id: String,
    pub frequency: String,
    pub frequency_interval: Option<i64>,
    #[schemars(description = "never, on_date or after_count")]
    pub end_type: String,
    pub end_date: Option<String>,
    #[schemars(description = "Number of expenses including the first one.")]
    pub end_count: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ExpenseRecurringCreated {
    pub id: String,
    pub caught_up: usize,
}

pub fn create_series(conn: &Connection, input: &ExpenseRecurringInput) -> AppResult<String> {
    validate_schedule(&input.frequency, input.frequency_interval, &input.end_type, input.end_date.as_deref(), input.end_count)?;
    let src = load_expense(conn, &input.expense_id)?;
    let existing: Option<String> =
        conn.query_row("SELECT expense_recurring_id FROM expenses WHERE id = ?1", params![src.id], |r| r.get(0)).optional()?.flatten();
    if existing.is_some() {
        return Err(msg(tr!("This expense already belongs to a series", "Diese Ausgabe gehört bereits zu einer Serie")));
    }
    let first = parse_date(&src.date).ok_or_else(|| msg(tr!("Invalid date", "Ungültiges Datum")))?;
    let anchor = first.day();
    let next = next_date(&input.frequency, first, input.frequency_interval, anchor);
    let id = crate::util::new_id();
    conn.execute(
        "INSERT INTO expense_recurring (id, name, description, amount_cents, currency, category_slug, note, frequency,
            frequency_interval, anchor_day, end_type, end_date, end_count, expenses_generated, next_scheduled_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 1, ?14)",
        params![
            id, src.name, src.description, src.amount_cents, src.currency, src.category_slug, src.note, input.frequency,
            input.frequency_interval.filter(|_| input.frequency == "custom"), anchor, input.end_type,
            input.end_date.as_ref().filter(|_| input.end_type == "on_date"),
            input.end_count.filter(|_| input.end_type == "after_count"), iso(next)
        ],
    )?;
    conn.execute("UPDATE expenses SET expense_recurring_id = ?2 WHERE id = ?1", params![src.id, id])?;
    if input.end_type == "after_count" && input.end_count == Some(1) {
        conn.execute("UPDATE expense_recurring SET status = 'completed' WHERE id = ?1", params![id])?;
    }
    activities::log(
        conn,
        "expense_series_started",
        &tr!("Recurring expense \"{}\" set up", "Wiederkehrende Ausgabe „{}“ eingerichtet", src.name),
        None,
    )?;
    Ok(id)
}

pub async fn save_expense_recurring(state: &AppState, input: ExpenseRecurringInput) -> AppResult<ExpenseRecurringCreated> {
    let conn = state.conn()?;
    let tx = conn.unchecked_transaction()?;
    let id = create_series(&tx, &input)?;
    let caught_up = run_due_on(&tx, today_date())?.len();
    tx.commit()?;
    Ok(ExpenseRecurringCreated { id, caught_up })
}

pub async fn set_expense_recurring_status(state: &AppState, id: String, status: String) -> AppResult<()> {
    let conn = state.conn()?;
    set_series_status(&conn, "expense_recurring", "expenses_generated", &id, &status)
}

pub async fn delete_expense_recurring(state: &AppState, id: String) -> AppResult<()> {
    state.conn()?.execute("DELETE FROM expense_recurring WHERE id = ?1", params![id])?;
    Ok(())
}

struct Series {
    id: String,
    name: String,
    description: Option<String>,
    amount_cents: i64,
    currency: String,
    category_slug: Option<String>,
    note: Option<String>,
    frequency: String,
    interval: Option<i64>,
    anchor: Option<i64>,
    end_type: String,
    end_date: Option<String>,
    end_count: Option<i64>,
    generated: i64,
    next: String,
}

fn active_series(conn: &Connection, due_by: Option<NaiveDate>) -> AppResult<Vec<Series>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, amount_cents, currency, category_slug, note, frequency, frequency_interval,
                anchor_day, end_type, end_date, end_count, expenses_generated, next_scheduled_at
         FROM expense_recurring WHERE status = 'active' AND (?1 IS NULL OR next_scheduled_at <= ?1)",
    )?;
    let rows = stmt.query_map(params![due_by.map(iso)], |r| {
        Ok(Series {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            amount_cents: r.get(3)?,
            currency: r.get(4)?,
            category_slug: r.get(5)?,
            note: r.get(6)?,
            frequency: r.get(7)?,
            interval: r.get(8)?,
            anchor: r.get(9)?,
            end_type: r.get(10)?,
            end_date: r.get(11)?,
            end_count: r.get(12)?,
            generated: r.get(13)?,
            next: r.get(14)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn walk(s: &Series, until: NaiveDate, mut f: impl FnMut(NaiveDate) -> AppResult<()>) -> AppResult<(NaiveDate, i64, bool)> {
    let Some(mut next) = parse_date(&s.next) else { return Ok((until, s.generated, true)) };
    let anchor = s.anchor.unwrap_or(next.day() as i64) as u32;
    let end_date = s.end_date.as_deref().and_then(parse_date);
    let mut generated = s.generated;
    for _ in 0..1000 {
        if series_finished(&s.end_type, s.end_count, end_date, generated, next) {
            return Ok((next, generated, true));
        }
        if next > until {
            break;
        }
        f(next)?;
        generated += 1;
        next = next_date(&s.frequency, next, s.interval, anchor);
    }
    let done = series_finished(&s.end_type, s.end_count, end_date, generated, next);
    Ok((next, generated, done))
}

fn run_due_on(conn: &Connection, today: NaiveDate) -> AppResult<Vec<String>> {
    let mut created = Vec::new();
    for s in active_series(conn, Some(today))? {
        let mut booked = 0;
        let (next, generated, done) = walk(&s, today, |date| {
            let input = ExpenseInput {
                id: None,
                date: iso(date),
                name: s.name.clone(),
                description: s.description.clone(),
                amount_cents: s.amount_cents,
                currency: Some(s.currency.clone()),
                category_slug: s.category_slug.clone(),
                note: s.note.clone(),
            };
            let id = upsert_expense(conn, &input)?;
            conn.execute("UPDATE expenses SET expense_recurring_id = ?2 WHERE id = ?1", params![id, s.id])?;
            booked += 1;
            created.push(s.name.clone());
            Ok(())
        })?;
        conn.execute(
            "UPDATE expense_recurring SET expenses_generated = ?2, next_scheduled_at = ?3, status = ?4,
                last_generated_at = CASE WHEN ?5 > 0 THEN ?6 ELSE last_generated_at END
             WHERE id = ?1",
            params![s.id, generated, iso(next), if done { "completed" } else { "active" }, booked, iso(today)],
        )?;
        if booked > 0 {
            let amount = fmt_money(s.amount_cents * booked, &s.currency);
            let message = if booked == 1 {
                tr!("Recurring expense \"{}\" booked ({})", "Wiederkehrende Ausgabe „{}“ gebucht ({})", s.name, amount)
            } else {
                tr!("Recurring expense \"{}\" booked {} times ({})", "Wiederkehrende Ausgabe „{}“ {}× gebucht ({})", s.name, booked, amount)
            };
            activities::log(conn, "recurring_expense_booked", &message, None)?;
        }
    }
    Ok(created)
}

pub fn run_due(conn: &Connection) -> AppResult<Vec<String>> {
    run_due_on(conn, today_date())
}

#[derive(Debug, Clone)]
pub struct ExpectedExpense {
    pub date: NaiveDate,
    pub name: String,
    pub category_slug: Option<String>,
    pub amount_cents: i64,
}

pub fn expected(conn: &Connection, today: NaiveDate, horizon: NaiveDate) -> AppResult<Vec<ExpectedExpense>> {
    let mut out = Vec::new();
    for s in active_series(conn, None)? {
        walk(&s, horizon, |date| {
            if date > today {
                out.push(ExpectedExpense { date, name: s.name.clone(), category_slug: s.category_slug.clone(), amount_cents: s.amount_cents });
            }
            Ok(())
        })?;
    }
    out.sort_by_key(|e| e.date);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn input(frequency: &str, end_type: &str) -> ExpenseRecurringInput {
        ExpenseRecurringInput {
            expense_id: "e1".into(),
            frequency: frequency.into(),
            frequency_interval: None,
            end_type: end_type.into(),
            end_date: None,
            end_count: None,
        }
    }

    fn setup(date: &str) -> Connection {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO expenses (id, date, name, amount_cents, currency, category_slug) VALUES ('e1', ?1, 'Adobe CC', 6599, 'EUR', 'software')",
            params![date],
        )
        .unwrap();
        conn
    }

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn series_starts_after_the_source_expense_and_links_it() {
        let conn = setup("2026-01-31");
        let id = create_series(&conn, &input("monthly_date", "never")).unwrap();
        let (next, generated): (String, i64) =
            conn.query_row("SELECT next_scheduled_at, expenses_generated FROM expense_recurring WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((next.as_str(), generated), ("2026-02-28", 1), "anchored to the 31st, clamped in February");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM expenses WHERE expense_recurring_id IS NOT NULL"), 1);
        assert!(create_series(&conn, &input("monthly_date", "never")).is_err(), "an expense joins at most one series");
    }

    #[test]
    fn books_missed_dates_and_keeps_the_anchor_day() {
        let conn = setup("2026-01-31");
        create_series(&conn, &input("monthly_date", "never")).unwrap();
        let created = run_due_on(&conn, d(2026, 4, 30)).unwrap();
        assert_eq!(created.len(), 3);
        let dates: Vec<String> = conn
            .prepare("SELECT date FROM expenses WHERE id != 'e1' ORDER BY date")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(dates, ["2026-02-28", "2026-03-31", "2026-04-30"]);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM expenses WHERE category_slug = 'software' AND amount_cents = 6599 AND expense_recurring_id IS NOT NULL"), 4);
        assert!(run_due_on(&conn, d(2026, 4, 30)).unwrap().is_empty(), "running twice books nothing twice");
    }

    #[test]
    fn ends_after_count_and_on_date() {
        let conn = setup("2026-01-01");
        let mut by_count = input("weekly", "after_count");
        by_count.end_count = Some(3);
        let id = create_series(&conn, &by_count).unwrap();
        run_due_on(&conn, d(2026, 6, 1)).unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM expenses"), 3, "3 including the first one");
        let status: String = conn.query_row("SELECT status FROM expense_recurring WHERE id = ?1", params![id], |r| r.get(0)).unwrap();
        assert_eq!(status, "completed");

        let conn = setup("2026-01-01");
        let mut by_date = input("monthly_date", "on_date");
        by_date.end_date = Some("2026-03-15".into());
        create_series(&conn, &by_date).unwrap();
        run_due_on(&conn, d(2026, 12, 31)).unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM expenses"), 3, "Jan, Feb, Mar 1st – April is after the end date");
    }

    #[test]
    fn paused_series_book_nothing() {
        let conn = setup("2026-01-01");
        let id = create_series(&conn, &input("monthly_date", "never")).unwrap();
        conn.execute("UPDATE expense_recurring SET status = 'paused' WHERE id = ?1", params![id]).unwrap();
        assert!(run_due_on(&conn, d(2026, 12, 31)).unwrap().is_empty());
        assert!(expected(&conn, d(2026, 6, 1), d(2026, 12, 31)).unwrap().is_empty());
    }

    #[test]
    fn resuming_books_nothing_for_the_paused_weeks() {
        let today = today_date();
        let conn = setup(&iso(today - chrono::Duration::days(69)));
        let id = create_series(&conn, &input("weekly", "never")).unwrap();
        conn.execute("UPDATE expense_recurring SET status = 'paused' WHERE id = ?1", params![id]).unwrap();
        set_series_status(&conn, "expense_recurring", "expenses_generated", &id, "active").unwrap();
        assert!(run_due_on(&conn, today).unwrap().is_empty(), "missed weeks are skipped, not booked");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM expenses"), 1);
    }

    #[test]
    fn rejects_invalid_schedules() {
        let conn = setup("2026-01-01");
        assert!(create_series(&conn, &input("hourly", "never")).is_err());
        assert!(create_series(&conn, &input("custom", "never")).is_err(), "custom needs an interval");
        assert!(create_series(&conn, &input("weekly", "on_date")).is_err(), "on_date needs a date");
        assert!(create_series(&conn, &input("weekly", "after_count")).is_err(), "after_count needs a count");
        assert!(create_series(&conn, &input("weekly", "sometime")).is_err());
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM expense_recurring"), 0);
    }

    #[test]
    fn deleting_a_series_keeps_its_expenses() {
        let conn = setup("2026-01-01");
        let id = create_series(&conn, &input("monthly_date", "never")).unwrap();
        run_due_on(&conn, d(2026, 3, 1)).unwrap();
        conn.execute("DELETE FROM expense_recurring WHERE id = ?1", params![id]).unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM expenses"), 3);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM expenses WHERE expense_recurring_id IS NOT NULL"), 0);
    }

    #[test]
    fn expected_lists_future_dates_until_the_horizon() {
        let conn = setup("2026-09-15");
        create_series(&conn, &input("monthly_date", "never")).unwrap();
        let future = expected(&conn, d(2026, 10, 6), d(2026, 12, 31)).unwrap();
        let dates: Vec<String> = future.iter().map(|e| iso(e.date)).collect();
        assert_eq!(dates, ["2026-10-15", "2026-11-15", "2026-12-15"]);
        assert!(future.iter().all(|e| e.amount_cents == 6599 && e.category_slug.as_deref() == Some("software")));
    }
}
