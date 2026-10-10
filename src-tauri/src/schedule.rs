use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};

use crate::i18n::tr;
use crate::error::{msg, AppResult};
use crate::util::{add_months_anchored, iso, last_day_of_month, parse_date, today_date};
pub const FREQUENCIES: &[&str] =
    &["weekly", "biweekly", "monthly_date", "monthly_last_day", "quarterly", "semi_annual", "annual", "custom"];

pub fn next_date(frequency: &str, current: NaiveDate, interval_days: Option<i64>, anchor_day: u32) -> NaiveDate {
    match frequency {
        "weekly" => current + Duration::days(7),
        "biweekly" => current + Duration::days(14),
        "monthly_date" => add_months_anchored(current, 1, anchor_day),
        "monthly_last_day" => last_day_of_month(add_months_anchored(current, 1, 1)),
        "quarterly" => add_months_anchored(current, 3, anchor_day),
        "semi_annual" => add_months_anchored(current, 6, anchor_day),
        "annual" => add_months_anchored(current, 12, anchor_day),
        _ => current + Duration::days(interval_days.unwrap_or(30).max(1)),
    }
}

pub fn series_finished(end_type: &str, end_count: Option<i64>, end_date: Option<NaiveDate>, generated: i64, next: NaiveDate) -> bool {
    match end_type {
        "after_count" => end_count.is_some_and(|c| generated >= c),
        "on_date" => end_date.is_some_and(|e| next > e),
        _ => false,
    }
}

pub fn validate_schedule(frequency: &str, interval: Option<i64>, end_type: &str, end_date: Option<&str>, end_count: Option<i64>) -> AppResult<()> {
    if !FREQUENCIES.contains(&frequency) {
        return Err(msg(tr!("Unknown frequency", "Unbekannte Frequenz")));
    }
    if frequency == "custom" && interval.is_none_or(|n| n < 1) {
        return Err(msg(tr!("Please enter the number of days between repetitions", "Bitte gib die Anzahl Tage zwischen den Wiederholungen an")));
    }
    match end_type {
        "never" => Ok(()),
        "on_date" if end_date.and_then(parse_date).is_some() => Ok(()),
        "on_date" => Err(msg(tr!("Invalid end date", "Ungültiges Enddatum"))),
        "after_count" if end_count.is_some_and(|n| n >= 1) => Ok(()),
        "after_count" => Err(msg(tr!("The number must be at least 1", "Die Anzahl muss mindestens 1 sein"))),
        _ => Err(msg(tr!("Invalid end", "Ungültiges Ende"))),
    }
}

type ScheduleRow = (String, String, Option<i64>, Option<i64>, String, Option<String>, Option<i64>, i64, String);

pub fn set_series_status(conn: &Connection, table: &str, counter: &str, id: &str, status: &str) -> AppResult<()> {
    if !["active", "paused", "canceled"].contains(&status) {
        return Err(msg(tr!("Invalid status", "Ungültiger Status")));
    }
    let row: Option<ScheduleRow> = conn
        .query_row(
            &format!(
                "SELECT status, frequency, frequency_interval, anchor_day, end_type, end_date, end_count, {counter}, next_scheduled_at
                 FROM {table} WHERE id = ?1"
            ),
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)),
        )
        .optional()?;
    let Some((current, frequency, interval, anchor, end_type, end_date, end_count, generated, mut next)) = row else {
        return Err(msg(tr!("Series not found", "Serie nicht gefunden")));
    };
    if current == "completed" {
        return Err(msg(tr!("This series is already completed", "Diese Serie ist bereits abgeschlossen")));
    }
    let mut status = status;
    if let (true, Some(mut date)) = (status == "active" && current != "active", parse_date(&next)) {
        let anchor = anchor.unwrap_or(date.day() as i64) as u32;
        let today = today_date();
        while date < today {
            date = next_date(&frequency, date, interval, anchor);
        }
        if series_finished(&end_type, end_count, end_date.as_deref().and_then(parse_date), generated, date) {
            status = "completed";
        }
        next = iso(date);
    }
    conn.execute(&format!("UPDATE {table} SET status = ?2, next_scheduled_at = ?3 WHERE id = ?1"), params![id, status, next])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn schedules() {
        assert_eq!(next_date("weekly", d(2026, 1, 1), None, 1), d(2026, 1, 8));
        assert_eq!(next_date("monthly_date", d(2026, 1, 31), None, 31), d(2026, 2, 28));
        assert_eq!(next_date("monthly_date", d(2026, 2, 28), None, 31), d(2026, 3, 31));
        assert_eq!(next_date("monthly_last_day", d(2026, 1, 31), None, 31), d(2026, 2, 28));
        assert_eq!(next_date("quarterly", d(2026, 11, 30), None, 30), d(2027, 2, 28));
        assert_eq!(next_date("custom", d(2026, 1, 1), Some(10), 1), d(2026, 1, 11));
    }

    #[test]
    fn rejects_invalid_schedules() {
        assert!(validate_schedule("monthly_date", None, "never", None, None).is_ok());
        assert!(validate_schedule("hourly", None, "never", None, None).is_err());
        assert!(validate_schedule("custom", Some(0), "never", None, None).is_err());
        assert!(validate_schedule("weekly", None, "on_date", Some("2026-02-31"), None).is_err());
        assert!(validate_schedule("weekly", None, "after_count", None, Some(0)).is_err());
    }
}
