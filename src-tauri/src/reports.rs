use chrono::{Datelike, Duration, Months, NaiveDate};
use rusqlite::{params, Connection};
use serde::Serialize;

use crate::i18n::tr;
use crate::db::{load_settings, AppState};
use crate::error::{msg, AppResult};
use crate::util::{iso, parse_date, today_date};


fn revenue(conn: &Connection, from: &str, to: &str) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(amount_cents), 0) FROM invoices WHERE status = 'paid' AND paid_at BETWEEN ?1 AND ?2",
        params![from, to],
        |r| r.get(0),
    )?)
}

fn expenses(conn: &Connection, from: &str, to: &str) -> AppResult<i64> {
    Ok(conn.query_row("SELECT COALESCE(SUM(amount_cents), 0) FROM expenses WHERE date BETWEEN ?1 AND ?2", params![from, to], |r| r.get(0))?)
}

fn month_start(d: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year(), d.month(), 1).expect("valid")
}

fn year_bounds(year: i32) -> (String, String) {
    (format!("{year}-01-01"), format!("{year}-12-31"))
}

fn previous_period(from: NaiveDate, to: NaiveDate) -> (NaiveDate, NaiveDate) {
    let span = (to - from).num_days() + 1;
    let prev_to = from - Duration::days(1);
    (prev_to - Duration::days(span - 1), prev_to)
}

#[derive(Debug, Serialize)]
pub struct SmallBusiness {
    pub year: i32,
    pub revenue_cents: i64,
    pub previous_year_revenue_cents: i64,
    pub limit_cents: i64,
    pub upper_limit_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub from: String,
    pub to: String,
    pub currency: String,
    pub revenue_cents: i64,
    pub expenses_cents: i64,
    pub profit_cents: i64,
    pub prev_revenue_cents: i64,
    pub prev_expenses_cents: i64,
    pub open_invoices_cents: i64,
    pub open_invoices_count: i64,
    pub overdue_cents: i64,
    pub overdue_count: i64,
    pub small_business: SmallBusiness,
}

pub fn summary(conn: &Connection, from: &str, to: &str) -> AppResult<Summary> {
    let (f, t) = (parse_date(from).ok_or_else(|| msg(tr!("Invalid start date", "Ungültiges Startdatum")))?, parse_date(to).ok_or_else(|| msg(tr!("Invalid end date", "Ungültiges Enddatum")))?);
    let (prev_from, prev_to) = previous_period(f, t);

    let rev = revenue(conn, from, to)?;
    let exp = expenses(conn, from, to)?;
    let (open_count, open_cents): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(amount_cents), 0) FROM invoices WHERE status IN ('unpaid','overdue')",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let (overdue_count, overdue_cents): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(amount_cents), 0) FROM invoices WHERE status = 'overdue'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let year = today_date().year();
    let (ys, ye) = year_bounds(year);
    let (ps, pe) = year_bounds(year - 1);

    let settings = load_settings(conn)?;
    Ok(Summary {
        from: from.into(),
        to: to.into(),
        currency: settings.base_currency,
        revenue_cents: rev,
        expenses_cents: exp,
        profit_cents: rev - exp,
        prev_revenue_cents: revenue(conn, &iso(prev_from), &iso(prev_to))?,
        prev_expenses_cents: expenses(conn, &iso(prev_from), &iso(prev_to))?,
        open_invoices_cents: open_cents,
        open_invoices_count: open_count,
        overdue_cents,
        overdue_count,
        small_business: SmallBusiness {
            year,
            revenue_cents: revenue(conn, &ys, &ye)?,
            previous_year_revenue_cents: revenue(conn, &ps, &pe)?,
            limit_cents: settings.limits.small_business_cents,
            upper_limit_cents: settings.limits.small_business_upper_cents,
        },
    })
}

pub async fn report_summary(state: &AppState, from: String, to: String) -> AppResult<Summary> {
    let conn = state.conn()?;
    summary(&conn, &from, &to)
}

#[derive(Debug, Serialize)]
pub struct MonthRow {
    pub month: String,
    pub revenue_cents: i64,
    pub expenses_cents: i64,
    pub profit_cents: i64,
}

fn sums_by_month(conn: &Connection, sql: &str, from: &str, to: &str) -> AppResult<std::collections::HashMap<String, i64>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![from, to], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn monthly(conn: &Connection, from: &str, to: &str) -> AppResult<Vec<MonthRow>> {
    let (Some(f), Some(t)) = (parse_date(from), parse_date(to)) else { return Err(msg(tr!("Invalid period", "Ungültiger Zeitraum"))) };
    let rev = sums_by_month(
        conn,
        "SELECT substr(paid_at, 1, 7), SUM(amount_cents) FROM invoices WHERE status = 'paid' AND paid_at BETWEEN ?1 AND ?2 GROUP BY 1",
        from,
        to,
    )?;
    let exp = sums_by_month(conn, "SELECT substr(date, 1, 7), SUM(amount_cents) FROM expenses WHERE date BETWEEN ?1 AND ?2 GROUP BY 1", from, to)?;
    let mut rows = Vec::new();
    let mut cursor = month_start(f);
    while cursor <= t {
        let key = cursor.format("%Y-%m").to_string();
        let (r, e) = (rev.get(&key).copied().unwrap_or(0), exp.get(&key).copied().unwrap_or(0));
        rows.push(MonthRow { month: key, revenue_cents: r, expenses_cents: e, profit_cents: r - e });
        cursor = cursor + Months::new(1);
    }
    let first = rows.iter().position(|m| m.revenue_cents != 0 || m.expenses_cents != 0).unwrap_or(rows.len());
    Ok(rows.split_off(first))
}

pub async fn report_monthly(state: &AppState, from: String, to: String) -> AppResult<Vec<MonthRow>> {
    let conn = state.conn()?;
    monthly(&conn, &from, &to)
}

#[derive(Debug, Serialize)]
pub struct ShareRow {
    pub key: String,
    pub name: String,
    pub color: String,
    pub total_cents: i64,
    pub count: i64,
}

const CUSTOMER_COLORS: &[&str] = &["#12804a", "#2457c5", "#a8640a", "#7c3aed", "#0e7490", "#be185d", "#4d7c0f", "#6b7280"];

pub fn shares(conn: &Connection, from: &str, to: &str, kind: Option<&str>) -> AppResult<Vec<ShareRow>> {
    let income = kind == Some("income");
    let sql = if income {
        "SELECT COALESCE(customer_id, customer_name, '-'), customer_name, '', SUM(amount_cents), COUNT(*)
         FROM invoices WHERE status = 'paid' AND paid_at BETWEEN ?1 AND ?2 GROUP BY 1 ORDER BY 4 DESC"
    } else {
        "SELECT COALESCE(c.slug, 'uncategorized'), c.name, COALESCE(c.color, '#9CA3AF'),
                SUM(e.amount_cents), COUNT(*)
         FROM expenses e LEFT JOIN categories c ON c.slug = e.category_slug
         WHERE e.date BETWEEN ?1 AND ?2 GROUP BY 1 ORDER BY 4 DESC"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![from, to], |r| {
        let name: Option<String> = r.get(1)?;
        let fallback = || if income { tr!("No customer", "Ohne Kunde") } else { tr!("Uncategorised", "Ohne Kategorie") };
        Ok(ShareRow { key: r.get(0)?, name: name.unwrap_or_else(fallback), color: r.get(2)?, total_cents: r.get(3)?, count: r.get(4)? })
    })?;
    let mut rows: Vec<ShareRow> = rows.collect::<Result<_, _>>()?;
    if income {
        for (i, row) in rows.iter_mut().enumerate() {
            row.color = CUSTOMER_COLORS[i % CUSTOMER_COLORS.len()].into();
        }
    }
    Ok(rows)
}

pub async fn report_categories(state: &AppState, from: String, to: String, kind: Option<String>) -> AppResult<Vec<ShareRow>> {
    let conn = state.conn()?;
    shares(&conn, &from, &to, kind.as_deref())
}

#[derive(Debug, Serialize)]
pub struct CustomerStat {
    pub key: String,
    pub customer_id: Option<String>,
    pub name: String,
    pub revenue_cents: i64,
    pub paid_count: i64,
    pub on_time_count: i64,
    pub avg_payment_days: Option<f64>,
    pub tracked_secs: i64,
    pub open_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct CategoryStat {
    pub key: String,
    pub name: String,
    pub color: String,
    pub total_cents: i64,
    pub prev_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct ProjectStat {
    pub id: String,
    pub name: String,
    pub customer_name: Option<String>,
    pub billable: bool,
    pub tracked_secs: i64,
}

#[derive(Debug, Serialize)]
pub struct Analysis {
    pub customers: Vec<CustomerStat>,
    pub categories: Vec<CategoryStat>,
    pub projects: Vec<ProjectStat>,
    pub paid_count: i64,
    pub on_time_count: i64,
    pub avg_payment_days: Option<f64>,
    pub fixed_costs_monthly_cents: i64,
    pub fixed_costs_count: i64,
    pub tax_year: i32,
    pub tax_share: Option<f64>,
}

fn monthly_factor(frequency: &str, interval_days: Option<i64>) -> f64 {
    match frequency {
        "weekly" => 52.0 / 12.0,
        "biweekly" => 26.0 / 12.0,
        "monthly_date" | "monthly_last_day" => 1.0,
        "quarterly" => 1.0 / 3.0,
        "semi_annual" => 1.0 / 6.0,
        "annual" => 1.0 / 12.0,
        _ => 365.25 / 12.0 / interval_days.unwrap_or(30).max(1) as f64,
    }
}

fn customer_entry(by_key: &mut std::collections::HashMap<String, CustomerStat>, key: String, id: Option<String>, name: Option<String>) -> &mut CustomerStat {
    let stat = by_key.entry(key.clone()).or_insert_with(|| CustomerStat {
        key,
        customer_id: None,
        name: String::new(),
        revenue_cents: 0,
        paid_count: 0,
        on_time_count: 0,
        avg_payment_days: None,
        tracked_secs: 0,
        open_cents: 0,
    });
    if stat.name.is_empty() {
        stat.name = name.unwrap_or_default();
    }
    if stat.customer_id.is_none() {
        stat.customer_id = id;
    }
    stat
}

fn customer_stats(conn: &Connection, from: &str, to: &str) -> AppResult<Vec<CustomerStat>> {
    let mut by_key: std::collections::HashMap<String, CustomerStat> = std::collections::HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT COALESCE(i.customer_id, i.customer_name, '-'), i.customer_id, COALESCE(c.name, i.customer_name),
                SUM(i.amount_cents), COUNT(*), SUM(substr(i.paid_at, 1, 10) <= i.due_date),
                AVG(MAX(0, julianday(substr(i.paid_at, 1, 10)) - julianday(i.issue_date)))
         FROM invoices i LEFT JOIN customers c ON c.id = i.customer_id
         WHERE i.status = 'paid' AND i.paid_at BETWEEN ?1 AND ?2 GROUP BY 1",
    )?;
    let rows = stmt.query_map(params![from, to], |r| {
        Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?, r.get::<_, i64>(5)?, r.get::<_, Option<f64>>(6)?))
    })?;
    for row in rows {
        let (key, id, name, revenue, count, on_time, days) = row?;
        let s = customer_entry(&mut by_key, key, id, name);
        s.revenue_cents = revenue;
        s.paid_count = count;
        s.on_time_count = on_time;
        s.avg_payment_days = days;
    }

    let mut stmt = conn.prepare(
        "SELECT p.customer_id, c.name, SUM(e.duration)
         FROM tracker_entries e JOIN tracker_projects p ON p.id = e.project_id JOIN customers c ON c.id = p.customer_id
         WHERE e.stop IS NOT NULL AND e.date BETWEEN ?1 AND ?2 GROUP BY 1",
    )?;
    let rows = stmt.query_map(params![from, to], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?;
    for row in rows {
        let (id, name, secs) = row?;
        customer_entry(&mut by_key, id.clone(), Some(id), Some(name)).tracked_secs = secs;
    }

    let mut stmt = conn.prepare(
        "SELECT COALESCE(i.customer_id, i.customer_name, '-'), i.customer_id, COALESCE(c.name, i.customer_name), SUM(i.amount_cents)
         FROM invoices i LEFT JOIN customers c ON c.id = i.customer_id
         WHERE i.status IN ('unpaid','overdue') GROUP BY 1",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)?)))?;
    for row in rows {
        let (key, id, name, open) = row?;
        customer_entry(&mut by_key, key, id, name).open_cents = open;
    }

    let mut out: Vec<CustomerStat> = by_key.into_values().collect();
    for s in &mut out {
        if s.name.is_empty() {
            s.name = tr!("No customer", "Ohne Kunde");
        }
    }
    out.sort_by(|a, b| b.revenue_cents.cmp(&a.revenue_cents).then(b.tracked_secs.cmp(&a.tracked_secs)).then(b.open_cents.cmp(&a.open_cents)).then(a.name.cmp(&b.name)));
    Ok(out)
}

fn category_stats(conn: &Connection, from: &str, to: &str, prev_from: &str, prev_to: &str) -> AppResult<Vec<CategoryStat>> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(c.slug, 'uncategorized'), c.name, COALESCE(c.color, '#9CA3AF'),
                SUM(CASE WHEN e.date BETWEEN ?1 AND ?2 THEN e.amount_cents ELSE 0 END),
                SUM(CASE WHEN e.date BETWEEN ?3 AND ?4 THEN e.amount_cents ELSE 0 END)
         FROM expenses e LEFT JOIN categories c ON c.slug = e.category_slug
         WHERE e.date BETWEEN ?1 AND ?2 OR e.date BETWEEN ?3 AND ?4
         GROUP BY 1 ORDER BY 4 DESC, 5 DESC",
    )?;
    let rows = stmt.query_map(params![from, to, prev_from, prev_to], |r| {
        let name: Option<String> = r.get(1)?;
        Ok(CategoryStat { key: r.get(0)?, name: name.unwrap_or_else(|| tr!("Uncategorised", "Ohne Kategorie")), color: r.get(2)?, total_cents: r.get(3)?, prev_cents: r.get(4)? })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn project_stats(conn: &Connection, from: &str, to: &str) -> AppResult<Vec<ProjectStat>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, c.name, p.billable, SUM(e.duration)
         FROM tracker_entries e JOIN tracker_projects p ON p.id = e.project_id LEFT JOIN customers c ON c.id = p.customer_id
         WHERE e.stop IS NOT NULL AND e.date BETWEEN ?1 AND ?2 GROUP BY p.id HAVING SUM(e.duration) > 0 ORDER BY 5 DESC",
    )?;
    let rows = stmt.query_map(params![from, to], |r| {
        Ok(ProjectStat { id: r.get(0)?, name: r.get(1)?, customer_name: r.get(2)?, billable: r.get(3)?, tracked_secs: r.get(4)? })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn fixed_costs(conn: &Connection) -> AppResult<(i64, i64)> {
    let mut stmt = conn.prepare("SELECT amount_cents, frequency, frequency_interval FROM expense_recurring WHERE status = 'active'")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?)))?;
    let (mut total, mut count) = (0.0, 0);
    for row in rows {
        let (amount, frequency, interval) = row?;
        total += amount as f64 * monthly_factor(&frequency, interval);
        count += 1;
    }
    Ok((total.round() as i64, count))
}

pub fn analysis(conn: &Connection, from: &str, to: &str) -> AppResult<Analysis> {
    let (Some(f), Some(t)) = (parse_date(from), parse_date(to)) else { return Err(msg(tr!("Invalid period", "Ungültiger Zeitraum"))) };
    let (prev_from, prev_to) = previous_period(f, t);
    let (paid_count, on_time_count, avg_payment_days): (i64, i64, Option<f64>) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(substr(paid_at, 1, 10) <= due_date), 0),
                AVG(MAX(0, julianday(substr(paid_at, 1, 10)) - julianday(issue_date)))
         FROM invoices WHERE status = 'paid' AND paid_at BETWEEN ?1 AND ?2",
        params![from, to],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let (fixed_costs_monthly_cents, fixed_costs_count) = fixed_costs(conn)?;
    Ok(Analysis {
        customers: customer_stats(conn, from, to)?,
        categories: category_stats(conn, from, to, &iso(prev_from), &iso(prev_to))?,
        projects: project_stats(conn, from, to)?,
        paid_count,
        on_time_count,
        avg_payment_days,
        fixed_costs_monthly_cents,
        fixed_costs_count,
        tax_year: t.year(),
        tax_share: crate::forecast::business_tax_share(conn, t.year(), today_date())?,
    })
}

pub async fn report_analysis(state: &AppState, from: String, to: String) -> AppResult<Analysis> {
    let conn = state.conn()?;
    analysis(&conn, &from, &to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profit_uses_payment_dates_and_expenses() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO invoices (id, invoice_number, customer_name, status, issue_date, due_date, amount_cents, paid_at) VALUES
               ('a', 'INV-1', 'Acme GmbH', 'paid', '2026-08-24', '2026-09-07', 219000, '2026-09-04'),
               ('b', 'INV-2', 'Acme GmbH', 'paid', '2026-08-03', '2026-08-17', 237000, '2026-08-14'),
               ('c', 'INV-3', 'Beispiel e.V.', 'unpaid', '2026-09-20', '2026-10-04', 120000, NULL);
             INSERT INTO expenses (id, date, name, amount_cents, category_slug) VALUES
               ('x', '2026-09-12', 'Figma', 1440, 'software'), ('y', '2026-08-02', 'Bahn', 2990, 'travel');",
        )
        .unwrap();
        let s = summary(&conn, "2026-09-01", "2026-09-30").unwrap();
        assert_eq!((s.revenue_cents, s.expenses_cents, s.profit_cents), (219000, 1440, 217560));
        assert_eq!((s.prev_revenue_cents, s.prev_expenses_cents), (237000, 2990));
        assert_eq!((s.open_invoices_count, s.open_invoices_cents), (1, 120000));

        let months = monthly(&conn, "2026-08-01", "2026-09-30").unwrap();
        assert_eq!(months.iter().map(|m| m.profit_cents).collect::<Vec<_>>(), [237000 - 2990, 219000 - 1440]);
        let all = monthly(&conn, "2000-01-01", "2026-10-31").unwrap();
        assert_eq!(all.first().map(|m| m.month.as_str()), Some("2026-08"), "leading empty months are dropped");
        assert_eq!(all.len(), 3, "empty months after the first booking stay");

        let by_customer = shares(&conn, "2026-08-01", "2026-09-30", Some("income")).unwrap();
        assert_eq!((by_customer.len(), by_customer[0].total_cents), (1, 456000));
        let by_category = shares(&conn, "2026-08-01", "2026-09-30", None).unwrap();
        assert_eq!(by_category[0].name, "Travel");
    }

    #[test]
    fn analysis_combines_customers_time_and_categories() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO customers (id, name) VALUES ('acme', 'Acme GmbH'), ('beta', 'Beta AG');
             INSERT INTO invoices (id, invoice_number, customer_id, customer_name, status, issue_date, due_date, amount_cents, paid_at) VALUES
               ('a', 'INV-1', 'acme', 'Acme GmbH', 'paid', '2026-09-01', '2026-09-15', 300000, '2026-09-11'),
               ('b', 'INV-2', 'acme', 'Acme GmbH', 'paid', '2026-09-01', '2026-09-15', 100000, '2026-09-21'),
               ('c', 'INV-3', 'beta', 'Beta AG', 'unpaid', '2026-09-20', '2026-10-04', 50000, NULL);
             INSERT INTO tracker_projects (id, name, customer_id) VALUES ('p', 'Website', 'acme'), ('q', 'Intern', NULL);
             INSERT INTO tracker_entries (id, project_id, date, start, stop, duration) VALUES
               ('1', 'p', '2026-09-03', '2026-09-03T09:00:00Z', '2026-09-03T17:00:00Z', 28800),
               ('2', 'q', '2026-09-04', '2026-09-04T09:00:00Z', '2026-09-04T10:00:00Z', 3600);
             INSERT INTO expenses (id, date, name, amount_cents, category_slug) VALUES
               ('x', '2026-09-12', 'Figma', 2000, 'software'), ('y', '2026-08-12', 'Figma', 1000, 'software'), ('z', '2026-08-02', 'Bahn', 2990, 'travel');
             INSERT INTO expense_recurring (id, name, amount_cents, frequency, next_scheduled_at, status) VALUES
               ('r1', 'Hosting', 1200, 'monthly_date', '2026-10-01', 'active'),
               ('r2', 'Domain', 2400, 'annual', '2027-01-01', 'active'),
               ('r3', 'Alt', 9900, 'monthly_date', '2026-10-01', 'paused');",
        )
        .unwrap();
        let a = analysis(&conn, "2026-09-01", "2026-09-30").unwrap();
        let acme = &a.customers[0];
        assert_eq!((acme.name.as_str(), acme.revenue_cents, acme.paid_count, acme.on_time_count, acme.tracked_secs), ("Acme GmbH", 400000, 2, 1, 28800));
        assert_eq!(acme.avg_payment_days, Some(15.0));
        let beta = a.customers.iter().find(|c| c.key == "beta").unwrap();
        assert_eq!((beta.revenue_cents, beta.open_cents), (0, 50000));
        assert_eq!((a.paid_count, a.on_time_count), (2, 1));
        assert_eq!(a.projects.iter().map(|p| p.tracked_secs).collect::<Vec<_>>(), [28800, 3600]);
        let software = a.categories.iter().find(|c| c.key == "software").unwrap();
        assert_eq!((software.total_cents, software.prev_cents), (2000, 1000));
        assert!(a.categories.iter().any(|c| c.key == "travel" && c.total_cents == 0), "categories only booked in the previous period stay visible");
        assert_eq!((a.fixed_costs_monthly_cents, a.fixed_costs_count), (1400, 2));
    }
}
