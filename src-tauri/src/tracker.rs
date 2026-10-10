use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::i18n::tr;
use crate::customers::get_customer;
use crate::db::{load_settings, AppState};
use crate::error::{msg, AppResult};
use crate::invoices::{load_invoice, upsert_invoice, Invoice, InvoiceInput, LineItem};
use crate::util::{fmt_money, iso, new_id, now_ts, parse_date, today_date};
use crate::activities;

#[derive(Debug, Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
    pub rate_cents: Option<i64>,
    pub fixed_price_cents: Option<i64>,
    pub currency: String,
    pub billable: bool,
    pub estimate_hours: Option<f64>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub status: String,
    pub total_seconds: i64,
    pub unbilled_seconds: i64,
    pub unbilled_cents: i64,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Entry {
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub date: String,
    pub start: String,
    pub stop: Option<String>,
    pub duration: i64,
    pub description: Option<String>,
    pub rate_cents: Option<i64>,
    pub billed: bool,
    pub invoice_id: Option<String>,
    pub invoice_number: Option<String>,
}

pub async fn list_projects(state: &AppState, include_completed: Option<bool>) -> AppResult<Vec<Project>> {
    let conn = state.conn()?;
    query_projects(&conn, include_completed.unwrap_or(false))
}

pub fn query_projects(conn: &Connection, include_completed: bool) -> AppResult<Vec<Project>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, p.description, p.customer_id, c.name, p.rate_cents, p.currency, p.billable,
                p.estimate_hours, p.status,
                COALESCE((SELECT SUM(duration) FROM tracker_entries e WHERE e.project_id = p.id), 0),
                COALESCE((SELECT SUM(duration) FROM tracker_entries e WHERE e.project_id = p.id AND e.billed = 0 AND e.stop IS NOT NULL), 0),
                CASE WHEN p.fixed_price_cents IS NOT NULL THEN
                  CASE WHEN EXISTS (SELECT 1 FROM tracker_entries e WHERE e.project_id = p.id AND e.billed = 0 AND e.stop IS NOT NULL)
                       THEN p.fixed_price_cents ELSE 0 END
                ELSE COALESCE((SELECT SUM(e.duration * COALESCE(e.rate_cents, p.rate_cents, 0) / 3600)
                          FROM tracker_entries e WHERE e.project_id = p.id AND e.billed = 0 AND e.stop IS NOT NULL), 0) END,
                (SELECT MIN(e.date) FROM tracker_entries e WHERE e.project_id = p.id),
                (SELECT MAX(e.date) FROM tracker_entries e WHERE e.project_id = p.id),
                p.fixed_price_cents, p.start_date, p.end_date
         FROM tracker_projects p LEFT JOIN customers c ON c.id = p.customer_id
         WHERE ?1 OR p.status = 'in_progress'
         ORDER BY p.status = 'completed',
                  COALESCE((SELECT MAX(e.start) FROM tracker_entries e WHERE e.project_id = p.id), p.created_at) DESC",
    )?;
    let rows = stmt.query_map(params![include_completed], |r| {
        Ok(Project {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            customer_id: r.get(3)?,
            customer_name: r.get(4)?,
            rate_cents: r.get(5)?,
            currency: r.get(6)?,
            billable: r.get(7)?,
            estimate_hours: r.get(8)?,
            status: r.get(9)?,
            total_seconds: r.get(10)?,
            unbilled_seconds: r.get(11)?,
            unbilled_cents: r.get(12)?,
            first_date: r.get(13)?,
            last_date: r.get(14)?,
            fixed_price_cents: r.get(15)?,
            start_date: r.get(16)?,
            end_date: r.get(17)?,
        })
    })?;
    let mut projects: Vec<Project> = rows.collect::<Result<_, _>>()?;
    for p in projects.iter_mut() {
        if let (Some(price), true) = (p.fixed_price_cents, p.unbilled_cents > 0) {
            p.unbilled_cents = price - fixed_price_billed(conn, &p.id, price)?;
        }
    }
    Ok(projects)
}

pub fn fixed_price_billed(conn: &Connection, project_id: &str, price: i64) -> AppResult<i64> {
    let untracked: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM tracker_entries WHERE project_id = ?1 AND billed = 1 AND invoice_id IS NULL)",
        params![project_id],
        |r| r.get(0),
    )?;
    if untracked {
        return Ok(price);
    }
    let mut stmt = conn.prepare(
        "SELECT i.subtotal_cents - i.discount_cents,
                EXISTS (SELECT 1 FROM tracker_entries o WHERE o.invoice_id = i.id AND o.project_id <> ?1)
         FROM invoices i
         WHERE i.status <> 'canceled'
           AND i.id IN (SELECT invoice_id FROM tracker_entries WHERE project_id = ?1 AND billed = 1)",
    )?;
    let billed = stmt
        .query_map(params![project_id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, bool>(1)?)))?
        .map(|row| row.map(|(net, shared)| if shared { price } else { net }))
        .sum::<Result<i64, _>>()?;
    Ok(billed.clamp(0, price))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectInput {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub customer_id: Option<String>,
    pub rate_cents: Option<i64>,
    #[serde(default)]
    pub fixed_price_cents: Option<i64>,
    pub currency: Option<String>,
    pub billable: bool,
    pub estimate_hours: Option<f64>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
    pub status: Option<String>,
}

pub async fn save_project(state: &AppState, project: ProjectInput) -> AppResult<String> {
    let p = project;
    if p.name.trim().is_empty() {
        return Err(msg(tr!("Project name is missing", "Projektname fehlt")));
    }
    let conn = state.conn()?;
    let customer = p.customer_id.filter(|s| !s.is_empty());
    let id = p.id.clone().unwrap_or_else(new_id);
    let rate = if p.fixed_price_cents.is_some() { None } else { p.rate_cents };
    conn.execute(
        "INSERT INTO tracker_projects (id, name, description, customer_id, rate_cents, currency, billable, estimate_hours, status,
            fixed_price_cents, start_date, end_date)
         VALUES (?1, ?2, ?3, ?4, ?5, COALESCE(?6, 'EUR'), ?7, ?8, COALESCE(?9, 'in_progress'), ?10, ?11, ?12)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, description = excluded.description,
            customer_id = excluded.customer_id, rate_cents = excluded.rate_cents, currency = excluded.currency,
            billable = excluded.billable, estimate_hours = excluded.estimate_hours, status = excluded.status,
            fixed_price_cents = excluded.fixed_price_cents, start_date = excluded.start_date, end_date = excluded.end_date",
        params![id, p.name.trim(), p.description, customer, rate, p.currency, p.billable, p.estimate_hours, p.status, p.fixed_price_cents,
            p.start_date.filter(|d| !d.is_empty()), p.end_date.filter(|d| !d.is_empty())],
    )?;
    conn.execute(
        "UPDATE tracker_projects
         SET start_date = (SELECT MIN(date) FROM tracker_entries WHERE project_id = ?1),
             end_date = (SELECT MAX(date) FROM tracker_entries WHERE project_id = ?1)
         WHERE id = ?1 AND status = 'completed' AND EXISTS (SELECT 1 FROM tracker_entries WHERE project_id = ?1)",
        params![id],
    )?;
    if p.fixed_price_cents.is_some() {
        conn.execute("UPDATE tracker_entries SET rate_cents = NULL WHERE project_id = ?1 AND billed = 0", params![id])?;
    }
    if p.id.is_none() {
        activities::log(&conn, "tracker_project_created", &tr!("Project \"{}\" created", "Projekt „{}“ angelegt", p.name.trim()), None)?;
    }
    Ok(id)
}

pub async fn delete_project(state: &AppState, id: String) -> AppResult<()> {
    state.conn()?.execute("DELETE FROM tracker_projects WHERE id = ?1", params![id])?;
    Ok(())
}

const ENTRY_SELECT: &str = "SELECT e.id, e.project_id, p.name, e.date, e.start, e.stop, e.duration, e.description,
    e.rate_cents, e.billed, e.invoice_id, i.invoice_number
  FROM tracker_entries e JOIN tracker_projects p ON p.id = e.project_id LEFT JOIN invoices i ON i.id = e.invoice_id";

fn map_entry(r: &rusqlite::Row) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: r.get(0)?,
        project_id: r.get(1)?,
        project_name: r.get(2)?,
        date: r.get(3)?,
        start: r.get(4)?,
        stop: r.get(5)?,
        duration: r.get(6)?,
        description: r.get(7)?,
        rate_cents: r.get(8)?,
        billed: r.get(9)?,
        invoice_id: r.get(10)?,
        invoice_number: r.get(11)?,
    })
}

pub async fn list_entries(
    state: &AppState,
    project_id: Option<String>,
    from: Option<String>,
    to: Option<String>,
) -> AppResult<Vec<Entry>> {
    let conn = state.conn()?;
    let mut stmt = conn.prepare(&format!(
        "{ENTRY_SELECT} WHERE (?1 IS NULL OR e.project_id = ?1) AND (?2 IS NULL OR e.date >= ?2) AND (?3 IS NULL OR e.date <= ?3)
         ORDER BY e.start DESC LIMIT 1000"
    ))?;
    let rows = stmt.query_map(params![project_id, from, to], map_entry)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn running_entry(conn: &Connection) -> AppResult<Option<Entry>> {
    Ok(conn.query_row(&format!("{ENTRY_SELECT} WHERE e.stop IS NULL"), [], map_entry).optional()?)
}

pub async fn running_timer(state: &AppState) -> AppResult<Option<Entry>> {
    let conn = state.conn()?;
    running_entry(&conn)
}

fn local_date_of(ts: &str) -> AppResult<String> {
    let dt = DateTime::parse_from_rfc3339(ts).map_err(|_| msg(tr!("Invalid timestamp: {ts}", "Ungültiger Zeitpunkt: {ts}")))?;
    Ok(dt.with_timezone(&Local).format("%Y-%m-%d").to_string())
}

fn seconds_between(start: &str, stop: &str) -> AppResult<i64> {
    let a = DateTime::parse_from_rfc3339(start).map_err(|_| msg(tr!("Invalid start time", "Ungültige Startzeit")))?;
    let b = DateTime::parse_from_rfc3339(stop).map_err(|_| msg(tr!("Invalid end time", "Ungültige Endzeit")))?;
    let secs = (b - a).num_seconds();
    if secs < 0 {
        return Err(msg(tr!("The end is before the start", "Das Ende liegt vor dem Start")));
    }
    Ok(secs)
}

const ROUNDING_MARKS: [i64; 9] = [0, 10, 15, 20, 30, 40, 45, 50, 60];

pub fn round_duration(secs: i64) -> i64 {
    if secs <= 0 {
        return 0;
    }
    let hours = secs / 3600;
    let rest = (secs % 3600) as f64 / 60.0;
    let mark = ROUNDING_MARKS
        .iter()
        .copied()
        .min_by(|a, b| {
            let (da, db) = ((*a as f64 - rest).abs(), (*b as f64 - rest).abs());
            da.partial_cmp(&db).unwrap().then(b.cmp(a))
        })
        .unwrap();
    ((hours * 60 + mark) * 60).max(600)
}

fn rounded_span(start: &str, stop: &str) -> AppResult<(String, i64)> {
    let duration = round_duration(seconds_between(start, stop)?);
    let begin = DateTime::parse_from_rfc3339(start).map_err(|_| msg(tr!("Invalid start time", "Ungültige Startzeit")))?;
    let end = (begin + Duration::seconds(duration)).with_timezone(&Utc);
    Ok((end.format("%Y-%m-%dT%H:%M:%SZ").to_string(), duration))
}

fn stop_running(conn: &Connection) -> AppResult<Option<String>> {
    let Some(entry) = running_entry(conn)? else { return Ok(None) };
    let (stop, duration) = rounded_span(&entry.start, &now_ts())?;
    conn.execute(
        "UPDATE tracker_entries SET stop = ?2, duration = ?3 WHERE id = ?1",
        params![entry.id, stop, duration],
    )?;
    Ok(Some(entry.id))
}

pub async fn start_timer(state: &AppState, project_id: String, description: Option<String>) -> AppResult<Entry> {
    let conn = state.conn()?;
    stop_running(&conn)?;
    let rate: Option<i64> = conn.query_row(
        "SELECT rate_cents FROM tracker_projects WHERE id = ?1",
        params![project_id],
        |r| r.get(0),
    )?;
    let id = new_id();
    let start = now_ts();
    conn.execute(
        "INSERT INTO tracker_entries (id, project_id, date, start, description, rate_cents) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, project_id, local_date_of(&start)?, start, description, rate],
    )?;
    Ok(conn.query_row(&format!("{ENTRY_SELECT} WHERE e.id = ?1"), params![id], map_entry)?)
}

pub async fn stop_timer(state: &AppState) -> AppResult<Option<String>> {
    let conn = state.conn()?;
    let stopped = stop_running(&conn)?;
    if stopped.is_some() {
        activities::log(&conn, "tracker_entry_created", &tr!("Time entry saved", "Zeiteintrag gespeichert"), None)?;
        check_weekly_limits(&conn)?;
    }
    Ok(stopped)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EntryInput {
    pub id: Option<String>,
    pub project_id: String,
    pub start: String,
    pub stop: String,
    pub description: Option<String>,
}

pub async fn save_entry(state: &AppState, entry: EntryInput) -> AppResult<String> {
    let (stop, duration) = rounded_span(&entry.start, &entry.stop)?;
    let date = local_date_of(&entry.start)?;
    let conn = state.conn()?;
    let rate: Option<i64> = conn.query_row(
        "SELECT rate_cents FROM tracker_projects WHERE id = ?1",
        params![entry.project_id],
        |r| r.get(0),
    )?;
    let id = entry.id.clone().unwrap_or_else(new_id);
    conn.execute(
        "INSERT INTO tracker_entries (id, project_id, date, start, stop, duration, description, rate_cents)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET project_id = excluded.project_id, date = excluded.date, start = excluded.start,
            stop = excluded.stop, duration = excluded.duration, description = excluded.description",
        params![id, entry.project_id, date, entry.start, stop, duration, entry.description, rate],
    )?;
    check_weekly_limits(&conn)?;
    Ok(id)
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct WeeklyHours {
    pub week_start: String,
    pub week_end: String,
    pub limit_hours: f64,
    pub tracked_seconds: i64,
    pub past_exceeded: Vec<ExceededWeek>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ExceededWeek {
    pub week_start: String,
    pub week_end: String,
    pub tracked_seconds: i64,
}

pub fn week_bounds(today: NaiveDate, starts_on_monday: bool) -> (NaiveDate, NaiveDate) {
    let weekday = today.weekday();
    let offset = if starts_on_monday { weekday.num_days_from_monday() } else { weekday.num_days_from_sunday() };
    let start = today - Duration::days(offset.into());
    (start, start + Duration::days(6))
}

fn week_usage(conn: &Connection) -> AppResult<WeeklyHours> {
    let settings = load_settings(conn)?;
    let (start, end) = week_bounds(today_date(), settings.week_starts_on_monday);
    let (week_start, week_end) = (start.format("%Y-%m-%d").to_string(), end.format("%Y-%m-%d").to_string());
    let tracked_seconds = conn.query_row(
        "SELECT COALESCE(SUM(duration), 0) FROM tracker_entries WHERE stop IS NOT NULL AND date BETWEEN ?1 AND ?2",
        params![week_start, week_end],
        |r| r.get(0),
    )?;
    let limit_hours = settings.weekly_hours_limit.max(0.0);
    let past_exceeded = if limit_hours > 0.0 {
        exceeded_weeks(conn, limit_hours, settings.week_starts_on_monday)?
            .into_values()
            .rev()
            .filter(|(week, _)| *week < start)
            .map(|(week, seconds)| ExceededWeek { week_start: iso(week), week_end: iso(week + Duration::days(6)), tracked_seconds: seconds })
            .collect()
    } else {
        Vec::new()
    };
    Ok(WeeklyHours { week_start, week_end, limit_hours, tracked_seconds, past_exceeded })
}

pub async fn weekly_hours(state: &AppState) -> AppResult<WeeklyHours> {
    let conn = state.conn()?;
    week_usage(&conn)
}

fn hours_label(hours: f64) -> String {
    let text = if hours.fract() == 0.0 { format!("{hours:.0}") } else { format!("{hours}") };
    if crate::i18n::is_de() { text.replace('.', ",") } else { text }
}

const WARNED_KEY: &str = "weekly_limit_warned";

fn hours_minutes(seconds: i64) -> String {
    format!("{}:{:02}", seconds / 3600, seconds % 3600 / 60)
}

fn week_label(start: NaiveDate) -> String {
    start.format(if crate::i18n::is_de() { "%d.%m.%Y" } else { "%d/%m/%Y" }).to_string()
}

fn exceeded_weeks(conn: &Connection, limit_hours: f64, starts_on_monday: bool) -> AppResult<BTreeMap<String, (NaiveDate, i64)>> {
    let mut totals: BTreeMap<NaiveDate, i64> = BTreeMap::new();
    let mut stmt = conn.prepare("SELECT date, SUM(duration) FROM tracker_entries WHERE stop IS NOT NULL GROUP BY date")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    for row in rows {
        let (day, seconds) = row?;
        if let Some(day) = parse_date(&day) {
            *totals.entry(week_bounds(day, starts_on_monday).0).or_default() += seconds;
        }
    }
    if let Some(entry) = running_entry(conn)? {
        if let Some(day) = parse_date(&entry.date) {
            *totals.entry(week_bounds(day, starts_on_monday).0).or_default() += seconds_between(&entry.start, &now_ts()).unwrap_or(0);
        }
    }
    let limit = (limit_hours * 3600.0).round() as i64;
    Ok(totals.into_iter().filter(|&(_, seconds)| seconds > limit).map(|(start, seconds)| (iso(start), (start, seconds))).collect())
}

pub fn check_weekly_limits(conn: &Connection) -> AppResult<()> {
    let settings = load_settings(conn)?;
    let limit_hours = settings.weekly_hours_limit;
    let exceeded = if limit_hours > 0.0 { exceeded_weeks(conn, limit_hours, settings.week_starts_on_monday)? } else { BTreeMap::new() };
    let raw: Option<String> = conn.query_row("SELECT value FROM settings WHERE key = ?1", params![WARNED_KEY], |r| r.get(0)).optional()?;
    let previous: BTreeMap<String, f64> = raw.and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default();
    let fresh: Vec<(NaiveDate, i64)> =
        exceeded.iter().filter(|(week, _)| previous.get(*week) != Some(&limit_hours)).map(|(_, &value)| value).collect();
    let warned: BTreeMap<&String, f64> = exceeded.keys().map(|week| (week, limit_hours)).collect();
    if warned.len() != previous.len() || !fresh.is_empty() {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![WARNED_KEY, serde_json::to_string(&warned)?],
        )?;
    }
    let limit = hours_label(limit_hours);
    let message = match fresh.as_slice() {
        [] => return Ok(()),
        [(start, seconds)] => {
            let (week, total) = (week_label(*start), hours_minutes(*seconds));
            tr!(
                "Weekly limit of {limit} h exceeded in the week of {week} ({total} h)",
                "Wochenlimit von {limit} h in der Woche ab {week} überschritten ({total} h)"
            )
        }
        weeks => {
            let n = weeks.len();
            let list = weeks.iter().rev().take(5).map(|(start, _)| week_label(*start)).collect::<Vec<_>>().join(", ");
            let more = if n > 5 { ", …" } else { "" };
            tr!(
                "Weekly limit of {limit} h exceeded in {n} weeks (weeks of {list}{more})",
                "Wochenlimit von {limit} h in {n} Wochen überschritten (Wochen ab {list}{more})"
            )
        }
    };
    let weeks: Vec<String> = fresh.iter().map(|(start, _)| iso(*start)).collect();
    activities::log(conn, "weekly_limit_exceeded", &message, Some(serde_json::json!({ "weeks": weeks })))
}

pub async fn delete_entry(state: &AppState, id: String) -> AppResult<()> {
    let conn = state.conn()?;
    conn.execute("DELETE FROM tracker_entries WHERE id = ?1", params![id])?;
    check_weekly_limits(&conn)
}

#[derive(Debug, Serialize)]
pub struct TrackerInvoiceDraft {
    pub customer_id: Option<String>,
    pub currency: String,
    pub lines: Vec<LineItem>,
    pub entry_ids: Vec<String>,
}

fn short_period(first: &str, last: &str) -> String {
    let (Some(a), Some(b)) = (crate::util::parse_date(first), crate::util::parse_date(last)) else {
        return format!("{} – {}", crate::i18n::fmt_date(first), crate::i18n::fmt_date(last));
    };
    let (day, full) = if !crate::i18n::is_de() { ("%d/%m", "%d/%m/%y") } else { ("%d.%m.", "%d.%m.%y") };
    if a == b {
        a.format(full).to_string()
    } else if a.format("%Y").to_string() == b.format("%Y").to_string() {
        format!("{}–{}", a.format(day), b.format(full))
    } else {
        format!("{}–{}", a.format(full), b.format(full))
    }
}

pub fn draft_from_project(conn: &Connection, project_id: &str) -> AppResult<TrackerInvoiceDraft> {
    let (name, customer_id, currency, project_rate, fixed): (String, Option<String>, String, Option<i64>, Option<i64>) = conn.query_row(
        "SELECT name, customer_id, currency, rate_cents, fixed_price_cents FROM tracker_projects WHERE id = ?1",
        params![project_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    let mut stmt = conn.prepare(
        "SELECT id, date, duration, rate_cents FROM tracker_entries
         WHERE project_id = ?1 AND billed = 0 AND stop IS NOT NULL ORDER BY date",
    )?;
    let entries: Vec<(String, String, i64, Option<i64>)> = stmt
        .query_map(params![project_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    if entries.is_empty() {
        return Err(msg(tr!("No unbilled time entries for this project", "Keine offenen Zeiteinträge für dieses Projekt")));
    }
    let period = short_period(&entries.first().unwrap().1, &entries.last().unwrap().1);

    if let Some(price) = fixed {
        let billed = fixed_price_billed(conn, project_id, price)?;
        let remaining = price - billed;
        let line = if billed == 0 {
            LineItem { name: format!("{name} ({period})"), quantity: 1.0, price_cents: price, unit: Some(tr!("pcs", "Stk.")) }
        } else if remaining > 0 {
            LineItem { name: tr!("{name} – remaining amount of fixed price ({period})", "{name} – Restbetrag Festpreis ({period})"), quantity: 1.0, price_cents: remaining, unit: Some(tr!("pcs", "Stk.")) }
        } else {
            LineItem { name: tr!("{name} – follow-up work, included in fixed price ({period})", "{name} – Nacharbeiten, im Festpreis enthalten ({period})"), quantity: 1.0, price_cents: 0, unit: Some(tr!("pcs", "Stk.")) }
        };
        return Ok(TrackerInvoiceDraft { customer_id, currency, lines: vec![line], entry_ids: entries.into_iter().map(|e| e.0).collect() });
    }

    let mut by_rate: BTreeMap<i64, i64> = BTreeMap::new();
    for (_, _, duration, rate) in &entries {
        *by_rate.entry(rate.or(project_rate).unwrap_or(0)).or_insert(0) += duration;
    }
    let lines = by_rate
        .into_iter()
        .map(|(rate, seconds)| LineItem {
            name: format!("{name} ({period})"),
            quantity: seconds as f64 / 3600.0,
            price_cents: rate,
            unit: Some(tr!("hrs", "Std.")),
        })
        .collect();
    Ok(TrackerInvoiceDraft {
        customer_id,
        currency,
        lines,
        entry_ids: entries.into_iter().map(|e| e.0).collect(),
    })
}

pub async fn invoice_lines_from_tracker(state: &AppState, project_id: String) -> AppResult<TrackerInvoiceDraft> {
    let conn = state.conn()?;
    draft_from_project(&conn, &project_id)
}

const TIMESHEET_SELECT: &str = "SELECT e.date, e.start, e.duration, COALESCE(NULLIF(TRIM(e.description), ''), p.name),
    c.name
  FROM tracker_entries e JOIN tracker_projects p ON p.id = e.project_id LEFT JOIN customers c ON c.id = p.customer_id
  WHERE e.stop IS NOT NULL";

fn timesheet_rows(conn: &Connection, filter: &str, args: &[&dyn rusqlite::ToSql]) -> AppResult<Vec<crate::latex::TimesheetEntry>> {
    let mut stmt = conn.prepare(&format!("{TIMESHEET_SELECT} AND {filter} ORDER BY e.start"))?;
    let rows = stmt.query_map(args, |r| {
        Ok(crate::latex::TimesheetEntry { date: r.get(0)?, start: r.get(1)?, duration: r.get(2)?, description: r.get(3)?, customer: r.get::<_, Option<String>>(4)?.unwrap_or_else(|| tr!("No customer", "Ohne Kunde")) })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn timesheet_in_range(conn: &Connection, from: &str, to: &str, customer_id: Option<&str>) -> AppResult<Vec<crate::latex::TimesheetEntry>> {
    timesheet_rows(conn, "e.date BETWEEN ?1 AND ?2 AND (?3 IS NULL OR p.customer_id = ?3)", &[&from, &to, &customer_id])
}

pub fn timesheet_of_invoice(conn: &Connection, invoice_id: &str) -> AppResult<Vec<crate::latex::TimesheetEntry>> {
    timesheet_rows(conn, "e.invoice_id = ?1", &[&invoice_id])
}

fn range_label(from: chrono::NaiveDate, to: chrono::NaiveDate) -> String {
    if from.year() == to.year() && (from.month(), from.day()) == (1, 1) && (to.month(), to.day()) == (12, 31) {
        tr!("Fiscal year {}", "Wirtschaftsjahr {}", from.year())
    } else {
        format!("{} – {}", crate::i18n::fmt_date(&crate::util::iso(from)), crate::i18n::fmt_date(&crate::util::iso(to)))
    }
}

pub async fn export_timesheet(state: &AppState, from: String, to: String, customer_id: Option<String>, target: String) -> AppResult<usize> {
    let (Some(start), Some(end)) = (crate::util::parse_date(&from), crate::util::parse_date(&to)) else {
        return Err(msg(tr!("Invalid period", "Ungültiger Zeitraum")));
    };
    if end < start {
        return Err(msg(tr!("The end is before the start", "Das Ende liegt vor dem Beginn")));
    }
    let (entries, settings) = {
        let conn = state.conn()?;
        (timesheet_in_range(&conn, &from, &to, customer_id.as_deref().filter(|c| !c.is_empty()))?, load_settings(&conn)?)
    };
    if entries.is_empty() {
        return Err(msg(tr!("There are no completed time entries in this period", "In diesem Zeitraum gibt es keine abgeschlossenen Zeiteinträge")));
    }
    let tc = crate::latex::toolchain(&state.files_dir).map_err(msg)?;
    crate::latex::render_timesheet(&tc, &entries, &range_label(start, end), start, &settings, std::path::Path::new(&target))?;
    Ok(entries.len())
}

pub fn invoice_unbilled(conn: &Connection, customer_id: &str) -> AppResult<String> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT p.id FROM tracker_projects p JOIN tracker_entries e ON e.project_id = p.id
         WHERE p.customer_id = ?1 AND p.billable = 1 AND e.billed = 0 AND e.stop IS NOT NULL
         ORDER BY p.name COLLATE NOCASE",
    )?;
    let project_ids: Vec<String> = stmt.query_map(params![customer_id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    if project_ids.is_empty() {
        return Err(msg(tr!("There is no unbilled time for this customer", "Für diesen Kunden gibt es keine offenen Zeiten")));
    }

    let drafts = project_ids.iter().map(|id| draft_from_project(conn, id)).collect::<AppResult<Vec<_>>>()?;
    let currency = drafts[0].currency.clone();
    if drafts.iter().any(|d| d.currency != currency) {
        return Err(msg(tr!("This customer's projects use different currencies – please bill each project separately", "Die Projekte dieses Kunden haben unterschiedliche Währungen – bitte pro Projekt abrechnen")));
    }

    let settings = load_settings(conn)?;
    let customer = get_customer(conn, customer_id)?;
    let issue = today_date();
    let terms = customer.default_payment_terms.unwrap_or(settings.default_payment_terms).max(0);
    let input = InvoiceInput {
        customer_id: Some(customer_id.to_string()),
        issue_date: issue.format("%Y-%m-%d").to_string(),
        due_date: (issue + chrono::Duration::days(terms)).format("%Y-%m-%d").to_string(),
        currency,
        line_items: drafts.iter().flat_map(|d| d.lines.clone()).collect(),
        vat_rate: if settings.small_business { 0.0 } else { settings.default_vat_rate },
        note: Some(settings.invoice_note.clone()).filter(|n| !n.trim().is_empty()),
        status: Some("draft".into()),
        tracker_entry_ids: drafts.into_iter().flat_map(|d| d.entry_ids).collect(),
        ..Default::default()
    };

    let tx = conn.unchecked_transaction()?;
    let id = upsert_invoice(&tx, &settings, &input)?;
    let invoice = load_invoice(&tx, &id)?;
    activities::log(
        &tx,
        "invoice_created",
        &tr!(
            "Invoice {} created from unbilled time for {} ({})",
            "Rechnung {} aus offenen Zeiten für {} erstellt ({})",
            invoice.invoice_number,
            customer.name,
            fmt_money(invoice.amount_cents, &invoice.currency)
        ),
        Some(serde_json::json!({ "invoice_id": id })),
    )?;
    tx.commit()?;
    Ok(id)
}

pub async fn invoice_unbilled_for_customer(state: &AppState, customer_id: String) -> AppResult<Invoice> {
    let conn = state.conn()?;
    let id = invoice_unbilled(&conn, &customer_id)?;
    crate::invoices::timesheet_after_create(&conn, &state.files_dir, &id);
    load_invoice(&conn, &id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn week_bounds_follow_the_configured_first_day() {
        let wed = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        assert_eq!(week_bounds(wed, true), (d("2026-10-05"), d("2026-10-11")));
        assert_eq!(week_bounds(wed, false), (d("2026-10-04"), d("2026-10-10")));
        assert_eq!(week_bounds(d("2026-10-11"), true).0, d("2026-10-05"), "Sunday belongs to the week that began on Monday");
    }

    #[test]
    fn weekly_limit_warns_for_past_weeks_once_and_rearms() {
        let conn = crate::db::open_in_memory().unwrap();
        let mut settings = load_settings(&conn).unwrap();
        settings.weekly_hours_limit = 2.0;
        crate::db::store_settings(&conn, &settings).unwrap();
        conn.execute("INSERT INTO tracker_projects (id, name) VALUES ('p', 'P')", []).unwrap();
        let add = |id: &str, date: &str, seconds: i64| {
            conn.execute(
                "INSERT INTO tracker_entries (id, project_id, date, start, stop, duration) VALUES (?1, 'p', ?2, '2026-01-01T09:00:00Z', '2026-01-01T10:00:00Z', ?3)",
                params![id, date, seconds],
            )
            .unwrap();
        };
        let warnings = |conn: &Connection| -> Vec<String> {
            let mut stmt = conn.prepare("SELECT message FROM activities WHERE type = 'weekly_limit_exceeded' ORDER BY rowid").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
        };

        add("a", "2026-03-02", 5400);
        add("b", "2026-03-08", 3600);
        add("c", "2026-03-09", 3600);
        check_weekly_limits(&conn).unwrap();
        assert_eq!(warnings(&conn).len(), 1, "Mon 02.03. + Sun 08.03. = 2.5 h in one past week");
        assert!(warnings(&conn)[0].contains("02/03/2026"), "{:?}", warnings(&conn));

        check_weekly_limits(&conn).unwrap();
        assert_eq!(warnings(&conn).len(), 1, "no repeat for an already reported week");

        add("d", "2026-03-10", 5400);
        add("e", "2025-12-01", 9000);
        check_weekly_limits(&conn).unwrap();
        assert_eq!(warnings(&conn).len(), 2, "newly exceeded weeks are reported together");
        assert!(warnings(&conn)[1].contains("2 weeks"), "{:?}", warnings(&conn));

        conn.execute("DELETE FROM tracker_entries WHERE id = 'b'", []).unwrap();
        check_weekly_limits(&conn).unwrap();
        add("b2", "2026-03-08", 3600);
        check_weekly_limits(&conn).unwrap();
        assert_eq!(warnings(&conn).len(), 3, "a week that dropped below and is exceeded again warns again");

        settings.weekly_hours_limit = 2.25;
        crate::db::store_settings(&conn, &settings).unwrap();
        check_weekly_limits(&conn).unwrap();
        assert_eq!(warnings(&conn).len(), 4, "a changed limit re-evaluates all weeks");
        assert!(warnings(&conn)[3].contains("3 weeks"), "{:?}", warnings(&conn));

        let past: Vec<String> = week_usage(&conn).unwrap().past_exceeded.into_iter().map(|w| w.week_start).collect();
        assert_eq!(past, ["2026-03-09", "2026-03-02", "2025-12-01"], "past weeks over the limit, newest first");
    }

    #[test]
    fn projects_sort_by_most_recent_work() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO tracker_projects (id, name, status, created_at) VALUES
               ('long', 'Lang, aber alt', 'completed', '2026-01-01T00:00:00Z'),
               ('recent', 'Kurz, aber neu', 'completed', '2026-01-01T00:00:00Z'),
               ('running', 'Running', 'in_progress', '2026-01-01T00:00:00Z');
             INSERT INTO tracker_entries (id, project_id, date, start, stop, duration) VALUES
               ('a', 'long', '2026-03-10', '2026-03-10T09:00:00+01:00', '2026-03-10T19:00:00+01:00', 36000),
               ('b', 'long', '2026-03-24', '2026-03-24T09:00:00+01:00', '2026-03-24T19:00:00+01:00', 36000),
               ('c', 'recent', '2026-09-09', '2026-09-09T09:00:00+02:00', '2026-09-09T10:00:00+02:00', 3600);",
        )
        .unwrap();
        let ps = query_projects(&conn, true).unwrap();
        let ids: Vec<&str> = ps.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["running", "recent", "long"], "running first, then latest work, not longest");
        assert_eq!((ps[2].first_date.as_deref(), ps[2].last_date.as_deref()), (Some("2026-03-10"), Some("2026-03-24")));
    }

    #[test]
    fn fixed_price_projects_bill_once_at_the_fixed_price() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO customers (id, name) VALUES ('c', 'Climbing Park');
             INSERT INTO tracker_projects (id, name, customer_id, fixed_price_cents, currency, billable) VALUES ('m', 'Joomla Migration', 'c', 350000, 'EUR', 1);
             INSERT INTO tracker_entries (id, project_id, date, start, stop, duration) VALUES
               ('1', 'm', '2026-06-01', '2026-06-01T09:00:00+02:00', '2026-06-01T15:00:00+02:00', 21600),
               ('2', 'm', '2026-06-24', '2026-06-24T09:00:00+02:00', '2026-06-24T13:00:00+02:00', 14400);",
        )
        .unwrap();
        let p = &query_projects(&conn, true).unwrap()[0];
        assert_eq!((p.unbilled_cents, p.rate_cents), (350000, None), "open amount is the fixed price, not hours × rate");

        let d = draft_from_project(&conn, "m").unwrap();
        assert_eq!(d.lines.len(), 1);
        assert_eq!((d.lines[0].name.as_str(), d.lines[0].quantity, d.lines[0].price_cents), ("Joomla Migration (01/06–24/06/26)", 1.0, 350000));

        conn.execute_batch(
            "UPDATE tracker_entries SET billed = 1;
             INSERT INTO tracker_entries (id, project_id, date, start, stop, duration) VALUES ('3', 'm', '2026-07-02', '2026-07-02T09:00:00+02:00', '2026-07-02T10:00:00+02:00', 3600);",
        )
        .unwrap();
        assert_eq!(query_projects(&conn, true).unwrap()[0].unbilled_cents, 0, "billed once, nothing more is owed");
        assert_eq!(draft_from_project(&conn, "m").unwrap().lines[0].price_cents, 0, "later time is included, not billed twice");
    }

    #[test]
    fn fixed_price_keeps_the_rest_open_after_a_partial_invoice() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO customers (id, name) VALUES ('c', 'Climbing Park');
             INSERT INTO tracker_projects (id, name, customer_id, fixed_price_cents, currency, billable, status) VALUES ('m', 'Joomla Migration', 'c', 350000, 'EUR', 1, 'in_progress');
             INSERT INTO invoices (id, invoice_number, customer_id, status, issue_date, due_date, subtotal_cents, discount_cents, amount_cents) VALUES
               ('i1', 'RE-1', 'c', 'unpaid', '2026-06-30', '2026-07-14', 110000, 10000, 119000);
             INSERT INTO tracker_entries (id, project_id, date, start, stop, duration, billed, invoice_id) VALUES
               ('1', 'm', '2026-06-01', '2026-06-01T09:00:00+02:00', '2026-06-01T15:00:00+02:00', 21600, 1, 'i1');
             INSERT INTO tracker_entries (id, project_id, date, start, stop, duration) VALUES
               ('2', 'm', '2026-07-02', '2026-07-02T09:00:00+02:00', '2026-07-02T10:00:00+02:00', 3600);",
        )
        .unwrap();
        assert_eq!(fixed_price_billed(&conn, "m", 350000).unwrap(), 100000, "net after discount");
        assert_eq!(query_projects(&conn, false).unwrap()[0].unbilled_cents, 250000);
        assert_eq!(draft_from_project(&conn, "m").unwrap().lines[0].price_cents, 250000);

        let today = NaiveDate::from_ymd_opt(2026, 7, 5).unwrap();
        let items = crate::forecast::collect_items(&conn, today).unwrap().items;
        let project: i64 = items.iter().filter(|i| i.kind == "project").map(|i| i.amount_cents).sum();
        assert_eq!(project, 297500, "remaining 2500 € plus 19 % VAT");

        conn.execute("UPDATE invoices SET status = 'canceled'", []).unwrap();
        assert_eq!(fixed_price_billed(&conn, "m", 350000).unwrap(), 0, "canceled invoices do not count");
    }

    #[test]
    fn completing_replaces_the_planned_duration() {
        let dir = std::env::temp_dir().join(format!("midnight-project-{}", new_id()));
        let state = AppState::new(crate::db::open_in_memory().unwrap(), dir.clone()).unwrap();
        let input = |status: &str| ProjectInput {
            id: Some("p".into()),
            name: "Website".into(),
            description: None,
            customer_id: None,
            rate_cents: Some(9000),
            fixed_price_cents: None,
            currency: None,
            billable: true,
            estimate_hours: None,
            start_date: Some("2026-11-01".into()),
            end_date: Some("2026-12-31".into()),
            status: Some(status.into()),
        };
        tauri::async_runtime::block_on(save_project(&state, input("in_progress"))).unwrap();
        state
            .conn()
            .unwrap()
            .execute_batch(
                "INSERT INTO tracker_entries (id, project_id, date, start, stop, duration) VALUES
                   ('1', 'p', '2026-10-02', '2026-10-02T09:00:00+02:00', '2026-10-02T10:00:00+02:00', 3600),
                   ('2', 'p', '2026-10-20', '2026-10-20T09:00:00+02:00', '2026-10-20T10:00:00+02:00', 3600);",
            )
            .unwrap();
        let dates = || {
            let p = query_projects(&state.conn().unwrap(), true).unwrap().remove(0);
            (p.start_date, p.end_date)
        };
        assert_eq!(dates(), (Some("2026-11-01".into()), Some("2026-12-31".into())), "planned while running");
        tauri::async_runtime::block_on(save_project(&state, input("completed"))).unwrap();
        assert_eq!(dates(), (Some("2026-10-02".into()), Some("2026-10-20".into())), "actual once completed");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn range_label_names_full_years() {
        let d = |s: &str| crate::util::parse_date(s).unwrap();
        assert_eq!(range_label(d("2026-01-01"), d("2026-12-31")), "Fiscal year 2026");
        assert_eq!(range_label(d("2026-08-01"), d("2026-09-30")), "01/08/2026 – 30/09/2026");
    }

    #[test]
    fn periods_use_the_short_invoice_style() {
        assert_eq!(short_period("2026-08-28", "2026-08-28"), "28/08/26");
        assert_eq!(short_period("2026-08-30", "2026-09-09"), "30/08–09/09/26");
        assert_eq!(short_period("2025-12-29", "2026-01-05"), "29/12/25–05/01/26");
    }

    #[test]
    fn durations_round_to_ten_minute_steps_and_quarter_hours() {
        let m = |min: i64| round_duration(min * 60) / 60;
        assert_eq!(m(7), 10);
        assert_eq!(m(12), 10);
        assert_eq!(m(13), 15);
        assert_eq!(m(17), 15);
        assert_eq!(m(18), 20);
        assert_eq!(m(26), 30);
        assert_eq!(m(34), 30);
        assert_eq!(m(43), 45);
        assert_eq!(m(48), 50);
        assert_eq!(m(56), 60);
        assert_eq!(m(67), 70, "works past the hour: 1:07 → 1:10");
        assert_eq!(m(2), 10, "short entries count as at least 10 minutes");
        assert_eq!(round_duration(0), 0);
        let (stop, d) = rounded_span("2026-10-04T09:00:00+02:00", "2026-10-04T10:07:00+02:00").unwrap();
        assert_eq!((stop.as_str(), d), ("2026-10-04T08:10:00Z", 4200), "end moves to start + rounded duration");
    }

    #[test]
    fn unbilled_time_of_a_customer_becomes_one_draft_invoice() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO customers (id, name) VALUES ('c', 'Acme GmbH'), ('other', 'Other Ltd');
             INSERT INTO tracker_projects (id, name, customer_id, rate_cents, currency, billable) VALUES
               ('web', 'Website', 'c', 9000, 'EUR', 1),
               ('app', 'App', 'c', 10000, 'EUR', 1),
               ('intern', 'Intern', 'c', 9000, 'EUR', 0),
               ('fremd', 'Fremd', 'other', 9000, 'EUR', 1);
             INSERT INTO tracker_entries (id, project_id, date, start, stop, duration, billed) VALUES
               ('w1', 'web', '2026-09-01', '2026-09-01T09:00:00+02:00', '2026-09-01T11:00:00+02:00', 7200, 0),
               ('w2', 'web', '2026-09-02', '2026-09-02T09:00:00+02:00', '2026-09-02T10:00:00+02:00', 3600, 0),
               ('wb', 'web', '2026-08-01', '2026-08-01T09:00:00+02:00', '2026-08-01T10:00:00+02:00', 3600, 1),
               ('a1', 'app', '2026-09-03', '2026-09-03T09:00:00+02:00', '2026-09-03T09:30:00+02:00', 1800, 0),
               ('run', 'app', '2026-09-04', '2026-09-04T09:00:00+02:00', NULL, 0, 0),
               ('i1', 'intern', '2026-09-03', '2026-09-03T09:00:00+02:00', '2026-09-03T10:00:00+02:00', 3600, 0),
               ('f1', 'fremd', '2026-09-03', '2026-09-03T09:00:00+02:00', '2026-09-03T10:00:00+02:00', 3600, 0);",
        )
        .unwrap();

        let id = invoice_unbilled(&conn, "c").unwrap();
        let inv = load_invoice(&conn, &id).unwrap();
        assert_eq!(inv.status, "draft");
        assert_eq!(inv.customer_id.as_deref(), Some("c"));
        assert_eq!(inv.subtotal_cents, 32000);
        assert_eq!(inv.line_items.len(), 2);

        let billed: Vec<String> = conn
            .prepare("SELECT id FROM tracker_entries WHERE invoice_id = ?1 ORDER BY id")
            .unwrap()
            .query_map(params![id], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(billed, ["a1", "w1", "w2"], "only finished, unbilled entries of billable projects of this customer");

        assert!(invoice_unbilled(&conn, "c").is_err(), "nothing left to bill");
    }
}
