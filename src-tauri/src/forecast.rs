use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::db::{load_settings, AppState};
use crate::error::{msg, AppResult};
use crate::i18n::{fmt_date, is_de, tr};
use crate::tax::{compute, compute_with, params_for, Adjust, IncomeRow};
use crate::tracker::{fixed_price_billed, query_projects, week_bounds, Project};
use crate::util::{fmt_money, iso, last_day_of_month, parse_date, today_date};

#[derive(Debug, Clone, Serialize)]
pub struct ForecastItem {
    pub kind: &'static str,
    pub label: String,
    pub customer: String,
    pub date: String,
    pub amount_cents: i64,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectPlan {
    pub name: String,
    pub customer: String,
    pub fixed_price: bool,
    pub billable: bool,
    pub remaining_hours: f64,
    pub weekly_hours: f64,
    pub finish: Option<String>,
    pub end: Option<String>,
    pub delayed: bool,
}

#[derive(Debug, Serialize)]
pub struct Outlook {
    pub booked_income_cents: i64,
    pub expected_income_cents: i64,
    pub expected_expense_cents: i64,
    pub income_cents: i64,
    pub profit_cents: i64,
    pub taxable_income_cents: i64,
    pub tax_total_cents: i64,
    pub balance_cents: i64,
    pub marginal_rate: f64,
}

#[derive(Debug, Serialize)]
pub struct ForecastWarning {
    pub level: &'static str,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct Forecast {
    pub year: i32,
    pub as_of: String,
    pub active: bool,
    pub items: Vec<ForecastItem>,
    pub expenses: Vec<ForecastItem>,
    pub outlook: Outlook,
    pub warnings: Vec<ForecastWarning>,
    pub projects_without_end: Vec<String>,
    pub plans: Vec<ProjectPlan>,
    pub weekly_hours_limit: f64,
}

#[derive(Debug, Default)]
pub struct Collected {
    pub items: Vec<ForecastItem>,
    pub plans: Vec<ProjectPlan>,
    pub without_end: Vec<String>,
}

const EPS: f64 = 1e-6;

fn hrs(h: f64) -> String {
    let text = format!("{:.1}", (h * 10.0).round() / 10.0);
    let text = text.strip_suffix(".0").unwrap_or(&text).to_string();
    if is_de() { text.replace('.', ",") } else { text }
}

struct Delays<'a> {
    conn: &'a Connection,
    fallback: i64,
    cache: HashMap<Option<String>, Option<i64>>,
}

impl<'a> Delays<'a> {
    fn new(conn: &'a Connection, fallback: i64) -> Self {
        Delays { conn, fallback: fallback.max(0), cache: HashMap::new() }
    }

    fn observed(&mut self, customer_id: Option<&str>) -> AppResult<Option<i64>> {
        let key = customer_id.map(str::to_string);
        if let Some(v) = self.cache.get(&key) {
            return Ok(*v);
        }
        let value = match customer_id {
            Some(id) => {
                let mut stmt = self.conn.prepare(
                    "SELECT CAST(ROUND(julianday(paid_at) - julianday(issue_date)) AS INTEGER) FROM invoices
                     WHERE customer_id = ?1 AND status = 'paid' AND paid_at IS NOT NULL
                     ORDER BY paid_at DESC LIMIT 6",
                )?;
                let mut days: Vec<i64> = stmt.query_map(params![id], |r| r.get(0))?.collect::<Result<_, _>>()?;
                days.sort_unstable();
                (days.len() >= 2).then(|| days[days.len() / 2].max(0))
            }
            None => None,
        };
        self.cache.insert(key, value);
        Ok(value)
    }

    fn get(&mut self, customer_id: Option<&str>) -> AppResult<i64> {
        if let Some(days) = self.observed(customer_id)? {
            return Ok(days);
        }
        let terms: Option<i64> = match customer_id {
            Some(id) => self.conn.query_row("SELECT default_payment_terms FROM customers WHERE id = ?1", params![id], |r| r.get(0)).optional()?.flatten(),
            None => None,
        };
        Ok(terms.unwrap_or(self.fallback).max(0))
    }
}

struct Demand {
    remaining: f64,
    tracked_week: f64,
    start: NaiveDate,
    end: NaiveDate,
}

fn workdays(week: NaiveDate, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    (0..7)
        .map(|i| week + Duration::days(i))
        .filter(|d| d.weekday().number_from_monday() <= 5 && *d >= from && *d <= to)
        .collect()
}

fn weeks_left(week: NaiveDate, from: NaiveDate, to: NaiveDate) -> usize {
    let mut count = 0;
    let mut w = week;
    while w <= to {
        if !workdays(w, from, to).is_empty() {
            count += 1;
        }
        w += Duration::days(7);
    }
    count
}

fn schedule(demands: &[Demand], limit: f64, tracked_all_week: f64, today: NaiveDate, week0: NaiveDate) -> Vec<BTreeMap<NaiveDate, f64>> {
    let mut remaining: Vec<f64> = demands.iter().map(|d| d.remaining).collect();
    let mut out = vec![BTreeMap::new(); demands.len()];
    let horizon = week0 + Duration::weeks(520);
    let mut week = week0;
    while remaining.iter().any(|r| *r > EPS) && week < horizon {
        let first = week == week0;
        let week_end = week + Duration::days(6);
        let open = workdays(week, today, week_end);
        let capacity = if limit <= 0.0 {
            f64::INFINITY
        } else if first {
            (limit - tracked_all_week).min(limit * open.len() as f64 / 5.0).max(0.0)
        } else {
            limit
        };
        let mut wants: Vec<(usize, f64, Vec<NaiveDate>)> = Vec::new();
        for (i, d) in demands.iter().enumerate() {
            if remaining[i] <= EPS || week_end < d.start {
                continue;
            }
            let from = today.max(d.start);
            let left = weeks_left(week, from, d.end);
            if left == 0 {
                if !open.is_empty() {
                    wants.push((i, remaining[i], open.clone()));
                }
                continue;
            }
            let days = workdays(week, from, d.end);
            if days.is_empty() {
                continue;
            }
            let tracked = if first { d.tracked_week } else { 0.0 };
            let want = ((remaining[i] + tracked) / left as f64 - tracked).clamp(0.0, remaining[i]);
            wants.push((i, want, days));
        }
        let total: f64 = wants.iter().map(|w| w.1).sum();
        let scale = if total > capacity { capacity / total } else { 1.0 };
        for (i, want, days) in wants {
            let hours = want * scale;
            if hours <= EPS {
                continue;
            }
            remaining[i] -= hours;
            for day in &days {
                *out[i].entry(*day).or_insert(0.0) += hours / days.len() as f64;
            }
        }
        week += Duration::days(7);
    }
    out
}

fn tracked_this_week(conn: &Connection, week0: NaiveDate) -> AppResult<HashMap<String, f64>> {
    let mut stmt = conn.prepare(
        "SELECT project_id, SUM(duration) FROM tracker_entries
         WHERE stop IS NOT NULL AND date BETWEEN ?1 AND ?2 GROUP BY project_id",
    )?;
    let rows = stmt.query_map(params![iso(week0), iso(week0 + Duration::days(6))], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    let mut map = HashMap::new();
    for row in rows {
        let (id, seconds) = row?;
        map.insert(id, seconds as f64 / 3600.0);
    }
    Ok(map)
}

fn window(p: &Project, today: NaiveDate) -> (NaiveDate, NaiveDate, bool) {
    let start = p.start_date.as_deref().and_then(parse_date).unwrap_or(today).max(today);
    match p.end_date.as_deref().and_then(parse_date) {
        Some(end) => (start, end, true),
        None => (start, NaiveDate::from_ymd_opt(today.year(), 12, 31).expect("valid date").max(start), false),
    }
}

pub fn collect_items(conn: &Connection, today: NaiveDate) -> AppResult<Collected> {
    let settings = load_settings(conn)?;
    let vat_factor = if settings.small_business { 1.0 } else { 1.0 + settings.default_vat_rate / 100.0 };
    let gross = |net: f64| (net * vat_factor).round() as i64;
    let no_customer = || tr!("No customer", "Ohne Kunde");
    let mut delays = Delays::new(conn, settings.default_payment_terms);
    let mut out = Collected::default();

    let mut stmt = conn.prepare(
        "SELECT invoice_number, customer_id, customer_name, status, issue_date, due_date, amount_cents FROM invoices
         WHERE status IN ('draft', 'unpaid', 'overdue', 'scheduled')",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, i64>(6)?,
        ))
    })?;
    for row in rows {
        let (number, customer_id, customer, status, issue, due, amount) = row?;
        let draft = status == "draft";
        let issue = parse_date(&issue).unwrap_or(today);
        let due = parse_date(&due).unwrap_or(issue);
        let observed = delays.observed(customer_id.as_deref())?;
        let date = match (draft, observed) {
            (true, Some(days)) => issue.max(today) + Duration::days(days),
            (true, None) => issue.max(today) + Duration::days((due - issue).num_days().max(0)),
            (false, Some(days)) => (issue + Duration::days(days)).max(today),
            (false, None) => due.max(today),
        };
        out.items.push(ForecastItem {
            kind: if draft { "draft" } else { "open_invoice" },
            label: if draft { tr!("Draft {number}", "Entwurf {number}") } else { tr!("Invoice {number}", "Rechnung {number}") },
            customer: customer.unwrap_or_else(no_customer),
            date: iso(date),
            amount_cents: amount,
            detail: String::new(),
        });
    }

    let (week0, _) = week_bounds(today, settings.week_starts_on_monday);
    let tracked = tracked_this_week(conn, week0)?;
    let tracked_all_week: f64 = tracked.values().sum();
    let projects = query_projects(conn, false)?;
    let mut planned: Vec<usize> = Vec::new();
    let mut demands: Vec<Demand> = Vec::new();
    for (i, p) in projects.iter().enumerate() {
        let (start, end, has_end) = window(p, today);
        if !has_end && (p.estimate_hours.is_some() || p.fixed_price_cents.is_some()) {
            out.without_end.push(p.name.clone());
        }
        let Some(estimate) = p.estimate_hours else { continue };
        planned.push(i);
        demands.push(Demand {
            remaining: (estimate - p.total_seconds as f64 / 3600.0).max(0.0),
            tracked_week: tracked.get(&p.id).copied().unwrap_or(0.0),
            start,
            end,
        });
    }
    let limit = settings.weekly_hours_limit.max(0.0);
    let days = schedule(&demands, limit, tracked_all_week, today, week0);
    let schedule_of: HashMap<usize, &BTreeMap<NaiveDate, f64>> = planned.iter().copied().zip(days.iter()).collect();

    let current_month = last_day_of_month(today);
    let empty = BTreeMap::new();
    for (i, p) in projects.iter().enumerate() {
        let customer = p.customer_name.clone().unwrap_or_else(no_customer);
        let plan = schedule_of.get(&i).copied().unwrap_or(&empty);
        let scheduled: f64 = plan.values().sum();
        let finish = plan.keys().next_back().copied();
        let (_, end, has_end) = window(p, today);

        if p.estimate_hours.is_some() {
            let weeks = plan.keys().map(|d| week_bounds(*d, settings.week_starts_on_monday).0).collect::<BTreeSet<_>>().len();
            out.plans.push(ProjectPlan {
                name: p.name.clone(),
                customer: customer.clone(),
                fixed_price: p.fixed_price_cents.is_some(),
                billable: p.billable,
                remaining_hours: scheduled,
                weekly_hours: if weeks > 0 { scheduled / weeks as f64 } else { 0.0 },
                finish: finish.map(iso),
                end: if has_end { Some(iso(end)) } else { None },
                delayed: has_end && finish.is_some_and(|f| f > end),
            });
        }
        if !p.billable {
            continue;
        }
        let delay = delays.get(p.customer_id.as_deref())?;

        if let Some(price) = p.fixed_price_cents {
            let remaining = price - fixed_price_billed(conn, &p.id, price)?;
            if remaining <= 0 {
                continue;
            }
            let invoice = finish.unwrap_or_else(|| end.max(today));
            out.items.push(ForecastItem {
                kind: "project",
                label: tr!("Project: {}", "Projekt: {}", p.name),
                customer,
                date: iso(invoice + Duration::days(delay)),
                amount_cents: gross(remaining as f64),
                detail: if remaining < price { tr!("Remaining amount", "Restbetrag") } else { String::new() },
            });
            continue;
        }

        let rate = p.rate_cents.unwrap_or(0) as f64;
        let mut months: BTreeMap<NaiveDate, f64> = BTreeMap::new();
        for (day, hours) in plan {
            *months.entry(last_day_of_month(*day)).or_insert(0.0) += hours;
        }
        if p.unbilled_cents > 0 {
            months.entry(current_month).or_insert(0.0);
        }
        for (month_end, hours) in months {
            let (tracked_cents, tracked_hours) = if month_end == current_month { (p.unbilled_cents, p.unbilled_seconds as f64 / 3600.0) } else { (0, 0.0) };
            let net = tracked_cents as f64 + hours * rate;
            if net < 1.0 {
                continue;
            }
            let invoice = match finish {
                Some(f) if last_day_of_month(f) == month_end => f.max(today),
                _ => month_end,
            };
            out.items.push(ForecastItem {
                kind: if hours > EPS { "project" } else { "unbilled" },
                label: if hours > EPS { tr!("Project: {}", "Projekt: {}", p.name) } else { tr!("Unbilled time: {}", "Nicht abgerechnete Zeit: {}", p.name) },
                customer: customer.clone(),
                date: iso(invoice + Duration::days(delay)),
                amount_cents: gross(net),
                detail: tr!("{} hrs", "{} Std.", hrs(tracked_hours + hours)),
            });
        }
    }
    out.items.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(out)
}

fn rows_for(items: &[ForecastItem]) -> Vec<IncomeRow> {
    items
        .iter()
        .map(|i| IncomeRow {
            number: i.label.clone(),
            issue_date: i.date.clone(),
            customer: i.customer.clone(),
            paid_at: i.date.clone(),
            amount_cents: i.amount_cents,
        })
        .collect()
}

fn expected_expenses(conn: &Connection, year: i32, today: NaiveDate, horizon: NaiveDate) -> AppResult<(Vec<ForecastItem>, i64)> {
    let meals_percent = load_settings(conn)?.limits.meals_deductible_percent;
    let mut deductible = 0;
    let mut items = Vec::new();
    for e in crate::expense_recurring::expected(conn, today, horizon)?.into_iter().filter(|e| e.date.year() == year) {
        deductible += if e.category_slug.as_deref() == Some("meals") { (e.amount_cents as f64 * meals_percent as f64 / 100.0).round() as i64 } else { e.amount_cents };
        let category: Option<String> = match &e.category_slug {
            Some(slug) => conn.query_row("SELECT name FROM categories WHERE slug = ?1", params![slug], |r| r.get(0)).ok(),
            None => None,
        };
        items.push(ForecastItem {
            kind: "recurring_expense",
            label: e.name,
            customer: category.unwrap_or_else(|| tr!("Uncategorised", "Ohne Kategorie")),
            date: iso(e.date),
            amount_cents: e.amount_cents,
            detail: String::new(),
        });
    }
    Ok((items, deductible))
}

fn year_end(year: i32) -> AppResult<NaiveDate> {
    NaiveDate::from_ymd_opt(year, 12, 31).ok_or_else(|| msg(tr!("Invalid year", "Ungültiges Jahr")))
}

pub fn business_tax_share(conn: &Connection, year: i32, today: NaiveDate) -> AppResult<Option<f64>> {
    let horizon = year_end(year)?;
    let mut adj = Adjust { extra_income: Vec::new(), extra_expense_cents: 0 };
    if horizon >= today {
        let prefix = format!("{year}-");
        let items: Vec<ForecastItem> = collect_items(conn, today)?.items.into_iter().filter(|i| i.date.starts_with(&prefix)).collect();
        adj.extra_income = rows_for(&items);
        adj.extra_expense_cents = expected_expenses(conn, year, today, horizon)?.1;
    }
    let full = compute_with(conn, year, &adj)?;
    if full.income_cents <= 0 {
        return Ok(None);
    }
    let without = Adjust { extra_income: adj.extra_income.clone(), extra_expense_cents: adj.extra_expense_cents + full.profit_cents.max(0) };
    let business_tax = (full.tax_total_cents - compute_with(conn, year, &without)?.tax_total_cents).max(0);
    Ok(Some(business_tax as f64 / full.income_cents as f64))
}

fn tax_delta(conn: &Connection, year: i32, base: &Adjust, base_tax: i64, delta: i64) -> AppResult<i64> {
    let adj = Adjust { extra_income: base.extra_income.clone(), extra_expense_cents: base.extra_expense_cents - delta };
    Ok(compute_with(conn, year, &adj)?.tax_total_cents - base_tax)
}

pub fn build(conn: &Connection, year: i32, today: NaiveDate) -> AppResult<Forecast> {
    let settings = load_settings(conn)?;
    let horizon = year_end(year)?;
    let active = horizon >= today;
    let collected = if active { collect_items(conn, today)? } else { Collected::default() };
    let prefix = format!("{year}-");
    let items: Vec<ForecastItem> = collected.items.into_iter().filter(|i| i.date.starts_with(&prefix)).collect();

    let (expenses, expense_deductible) = if active { expected_expenses(conn, year, today, horizon)? } else { (Vec::new(), 0) };
    let adj = Adjust { extra_income: rows_for(&items), extra_expense_cents: expense_deductible };
    let real = compute_with(conn, year, &adj)?;
    let marginal = tax_delta(conn, year, &adj, real.tax_total_cents, 100_000)? as f64 / 100_000.0;
    let expected: i64 = items.iter().map(|i| i.amount_cents).sum();
    let outlook = Outlook {
        booked_income_cents: real.income_cents - expected,
        expected_income_cents: expected,
        expected_expense_cents: expenses.iter().map(|e| e.amount_cents).sum(),
        income_cents: real.income_cents,
        profit_cents: real.profit_cents,
        taxable_income_cents: real.taxable_income_cents,
        tax_total_cents: real.tax_total_cents,
        balance_cents: real.balance_cents,
        marginal_rate: marginal,
    };
    let eur = |c: i64| fmt_money(c, "EUR");

    let mut warnings = Vec::new();
    if active {
        let actual = compute(conn, year)?;
        let mut flow: Vec<(String, i64)> = actual.income.iter().map(|i| (i.paid_at.clone(), i.amount_cents)).collect();
        flow.extend(items.iter().map(|i| (i.date.clone(), i.amount_cents)));
        flow.sort();
        let crossing = |limit: i64| {
            let mut sum = 0;
            flow.iter()
                .find(|(_, c)| {
                    sum += c;
                    sum > limit
                })
                .map(|(d, _)| crate::i18n::fmt_date(d))
        };
        let limit = settings.weekly_hours_limit;
        for plan in collected.plans.iter().filter(|p| p.delayed) {
            let (finish, end) = (fmt_date(plan.finish.as_deref().unwrap_or_default()), fmt_date(plan.end.as_deref().unwrap_or_default()));
            let text = if limit > 0.0 {
                tr!("{0} will probably only be finished on {1} instead of {2} at a maximum of {3} hrs/week.", "{0} wird bei maximal {3} Std./Woche voraussichtlich erst am {1} fertig statt am {2}.", plan.name, finish, end, hrs(limit))
            } else {
                tr!("{0} will probably only be finished on {1} instead of {2}.", "{0} wird voraussichtlich erst am {1} fertig statt am {2}.", plan.name, finish, end)
            };
            warnings.push(ForecastWarning { level: "warn", text });
        }
        if limit <= 0.0 && !collected.plans.is_empty() {
            warnings.push(ForecastWarning { level: "info", text: tr!("No weekly working time set. The forecast assumes unlimited capacity.", "Keine Wochenarbeitszeit hinterlegt. Die Prognose geht von unbegrenzter Kapazität aus.") });
        }
        let limits = &settings.limits;
        if settings.small_business {
            if let Some(d) = crossing(limits.small_business_upper_cents) {
                warnings.push(ForecastWarning { level: "warn", text: tr!("Revenue will probably pass {1} on {0}. From that invoice on, you must charge VAT.", "Der Umsatz überschreitet voraussichtlich am {0} die Grenze von {1}. Ab dieser Rechnung fällt Umsatzsteuer an.", d, eur(limits.small_business_upper_cents)) });
            } else if let Some(d) = crossing(limits.small_business_cents) {
                warnings.push(ForecastWarning { level: "warn", text: tr!("Revenue will probably pass {1} on {0}. The small business scheme ends in {2}.", "Der Umsatz überschreitet voraussichtlich am {0} die Grenze von {1}. Ab {2} gilt die Kleinunternehmerregelung nicht mehr.", d, eur(limits.small_business_cents), year + 1) });
            } else {
                warnings.push(ForecastWarning { level: "info", text: tr!("Expected revenue of {} is below the small business limit of {}.", "Erwarteter Umsatz von {} liegt unter der Kleinunternehmergrenze von {}.", eur(real.income_cents), eur(limits.small_business_cents)) });
            }
        }
        if real.taxable_income_cents > params_for(year).basic_allowance && real.tax_total_cents >= 40_000 {
            warnings.push(ForecastWarning { level: "info", text: tr!("Expect prepayments of about {0} per quarter for {1}.", "Für {1} sind Vorauszahlungen von etwa {0} pro Quartal zu erwarten.", eur(real.tax_total_cents / 4), year + 1) });
        }
        if real.balance_cents > 0 && year == today.year() {
            let months_left = 13 - today.month() as i64;
            warnings.push(ForecastWarning { level: "info", text: tr!("Set aside {} for the tax, or {} per month until December.", "Lege {} für die Steuer zurück, also {} pro Monat bis Dezember.", eur(real.balance_cents), eur(real.balance_cents / months_left)) });
        }
    }

    Ok(Forecast {
        year,
        as_of: iso(today),
        active,
        items,
        expenses,
        outlook,
        warnings,
        projects_without_end: collected.without_end,
        plans: collected.plans,
        weekly_hours_limit: settings.weekly_hours_limit.max(0.0),
    })
}

pub async fn tax_forecast(state: &AppState, year: i32) -> AppResult<Forecast> {
    let conn = state.conn()?;
    build(&conn, year, today_date())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_expected_payments_of_the_year() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO invoices (id, invoice_number, customer_name, status, issue_date, due_date, amount_cents, paid_at) VALUES
               ('a', 'INV-1', 'Acme', 'paid', '2026-03-01', '2026-03-15', 2000000, '2026-03-10'),
               ('b', 'INV-2', 'Acme', 'unpaid', '2026-10-01', '2026-10-15', 100000, NULL),
               ('c', 'INV-3', 'Acme', 'draft', '2026-10-01', '2026-10-20', 50000, NULL),
               ('d', 'INV-4', 'Acme', 'unpaid', '2026-12-20', '2027-01-05', 70000, NULL);
             INSERT INTO tracker_projects (id, name, rate_cents, estimate_hours, start_date, end_date) VALUES
               ('p', 'Website', 10000, 20, '2026-10-01', '2026-11-30');",
        )
        .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        let f = build(&conn, 2026, today).unwrap();
        assert!(f.active);
        assert!(f.items.iter().all(|i| i.date.starts_with("2026-")), "INV-4 is only paid in 2027");
        let project: i64 = f.items.iter().filter(|i| i.kind == "project").map(|i| i.amount_cents).sum();
        assert_eq!(project, 238_000, "20 h × 100 € plus 19 % VAT, billed end of October and November");
        assert_eq!(f.outlook.booked_income_cents, 2_000_000);
        assert_eq!(f.outlook.expected_income_cents, 150_000 + 238_000);
        assert_eq!(f.outlook.income_cents, 2_388_000);
        assert!(f.outlook.tax_total_cents > compute(&conn, 2026).unwrap().tax_total_cents);
        assert!(f.projects_without_end.is_empty());

        let past = build(&conn, 2025, today).unwrap();
        assert!(!past.active && past.items.is_empty());
    }

    #[test]
    fn upcoming_recurring_expenses_lower_profit_and_tax() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO invoices (id, invoice_number, customer_name, status, issue_date, due_date, amount_cents, paid_at) VALUES
               ('a', 'INV-1', 'Acme', 'paid', '2026-03-01', '2026-03-15', 6000000, '2026-03-10');
             INSERT INTO expenses (id, date, name, amount_cents, category_slug) VALUES
               ('rent', '2026-09-01', 'Büro', 50000, NULL),
               ('lunch', '2026-09-20', 'Kundenessen', 10000, 'meals');",
        )
        .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        let before = build(&conn, 2026, today).unwrap();
        for id in ["rent", "lunch"] {
            let input = crate::expense_recurring::ExpenseRecurringInput {
                expense_id: id.into(),
                frequency: "monthly_date".into(),
                frequency_interval: None,
                end_type: "never".into(),
                end_date: None,
                end_count: None,
            };
            crate::expense_recurring::create_series(&conn, &input).unwrap();
        }
        let f = build(&conn, 2026, today).unwrap();
        let dates: Vec<_> = f.expenses.iter().map(|e| (e.date.as_str(), e.amount_cents)).collect();
        assert_eq!(dates, [("2026-10-20", 10000), ("2026-11-01", 50000), ("2026-11-20", 10000), ("2026-12-01", 50000), ("2026-12-20", 10000)]);
        assert_eq!(f.outlook.expected_expense_cents, 130_000);
        assert_eq!(before.outlook.profit_cents - f.outlook.profit_cents, 100_000 + 3 * 7_000, "meals count at 70 %");
        assert!(f.outlook.tax_total_cents < before.outlook.tax_total_cents);
        assert_eq!(f.outlook.expected_income_cents, before.outlook.expected_income_cents, "income side is unchanged");
    }

    fn by_date(items: &[ForecastItem]) -> BTreeMap<String, i64> {
        let mut map = BTreeMap::new();
        for i in items {
            *map.entry(i.date.clone()).or_insert(0) += i.amount_cents;
        }
        map
    }

    fn track(conn: &Connection, id: &str, project: &str, date: &str, hours: i64) {
        conn.execute(
            "INSERT INTO tracker_entries (id, project_id, date, start, stop, duration) VALUES (?1, ?2, ?3, ?3 || 'T08:00:00Z', ?3 || 'T09:00:00Z', ?4)",
            params![id, project, date, hours * 3600],
        )
        .unwrap();
    }

    #[test]
    fn tracking_on_pace_moves_hours_from_plan_to_tracked_without_shifting_months() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO tracker_projects (id, name, rate_cents, estimate_hours, start_date, end_date) VALUES
               ('p', 'Website', 10000, 100, '2026-10-05', '2026-12-11');",
        )
        .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        let before = by_date(&collect_items(&conn, today).unwrap().items);
        assert_eq!(before.len(), 3, "one invoice per month: {before:?}");
        assert_eq!(before["2026-11-14"], 476_000, "40 h in October");

        track(&conn, "1", "p", "2026-10-05", 6);
        track(&conn, "2", "p", "2026-10-06", 4);
        let after = collect_items(&conn, today).unwrap();
        assert_eq!(by_date(&after.items), before, "10 h tracked this week were planned for this week anyway");
        assert_eq!((after.items[0].detail.as_str(), after.items[0].kind), ("40 hrs", "project"));

        track(&conn, "3", "p", "2026-10-07", 8);
        let ahead = by_date(&collect_items(&conn, today).unwrap().items);
        assert_eq!(ahead.values().sum::<i64>(), before.values().sum::<i64>(), "working ahead never changes the total");
        assert!(ahead["2026-11-14"] > before["2026-11-14"] && ahead["2026-12-14"] < before["2026-12-14"]);
    }

    #[test]
    fn weekly_limit_caps_all_projects_and_flags_delays() {
        let conn = crate::db::open_in_memory().unwrap();
        let mut settings = load_settings(&conn).unwrap();
        settings.weekly_hours_limit = 12.5;
        crate::db::store_settings(&conn, &settings).unwrap();
        conn.execute_batch(
            "INSERT INTO tracker_projects (id, name, rate_cents, estimate_hours, start_date, end_date) VALUES
               ('a', 'Shop', 8000, 50, '2026-10-12', '2026-11-20'),
               ('b', 'App', 8000, 50, '2026-10-12', '2026-11-20');",
        )
        .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 9).unwrap();
        let demands: Vec<Demand> = ["a", "b"]
            .iter()
            .map(|_| Demand { remaining: 50.0, tracked_week: 0.0, start: NaiveDate::from_ymd_opt(2026, 10, 12).unwrap(), end: NaiveDate::from_ymd_opt(2026, 11, 20).unwrap() })
            .collect();
        let week0 = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        let plan = schedule(&demands, 12.5, 0.0, today, week0);
        let mut weeks: BTreeMap<NaiveDate, f64> = BTreeMap::new();
        for day in plan.iter().flat_map(|p| p.iter()) {
            *weeks.entry(week_bounds(*day.0, true).0).or_insert(0.0) += day.1;
        }
        assert!(weeks.values().all(|h| *h <= 12.5 + 1e-6), "{weeks:?}");
        assert!((plan.iter().flat_map(|p| p.values()).sum::<f64>() - 100.0).abs() < 1e-6, "nothing gets lost");

        let f = build(&conn, 2026, today).unwrap();
        assert!(f.plans.iter().all(|p| p.delayed && p.finish.as_deref() > Some("2026-11-20")));
        assert!(f.warnings.iter().any(|w| w.level == "warn" && w.text.contains("Shop")));
    }

    #[test]
    fn fixed_price_is_paid_after_completion_and_delay_is_learned() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO customers (id, name) VALUES ('c', 'Acme');
             INSERT INTO invoices (id, invoice_number, customer_id, status, issue_date, due_date, amount_cents, paid_at) VALUES
               ('i1', 'INV-1', 'c', 'paid', '2026-03-01', '2026-03-15', 1000, '2026-03-31'),
               ('i2', 'INV-2', 'c', 'paid', '2026-05-01', '2026-05-15', 1000, '2026-06-10'),
               ('i3', 'INV-3', 'c', 'paid', '2026-07-01', '2026-07-15', 1000, '2026-07-21');
             INSERT INTO tracker_projects (id, name, customer_id, fixed_price_cents, estimate_hours, start_date, end_date) VALUES
               ('f', 'Relaunch', 'c', 300000, 30, '2026-10-12', '2026-11-27');",
        )
        .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 9).unwrap();
        let items = collect_items(&conn, today).unwrap().items;
        assert_eq!(items.len(), 1);
        assert_eq!((items[0].date.as_str(), items[0].amount_cents), ("2026-12-27", 357_000), "finished 27 Nov, paid after the median of 20/30/40 days");
    }

    #[test]
    fn business_tax_share_and_prepayments() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO invoices (id, invoice_number, customer_name, status, issue_date, due_date, amount_cents, paid_at) VALUES
               ('a', 'INV-1', 'Acme', 'paid', '2025-03-01', '2025-03-15', 4000000, '2025-03-10');",
        )
        .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 9).unwrap();
        let report = compute(&conn, 2025).unwrap();
        let share = business_tax_share(&conn, 2025, today).unwrap().unwrap();
        assert!((share - report.tax_total_cents as f64 / 4_000_000.0).abs() < 1e-9, "no other income: all tax comes from the business");
        assert_eq!(business_tax_share(&conn, 2024, today).unwrap(), None);

        let input = crate::tax::TaxInput { prepayments_cents: 100_000, ..Default::default() };
        conn.execute("INSERT INTO tax_years (year, data) VALUES (2025, ?1)", params![serde_json::to_string(&input).unwrap()]).unwrap();
        assert_eq!(compute(&conn, 2025).unwrap().balance_cents, report.balance_cents - 100_000);
    }

    #[test]
    fn out_of_range_year_is_an_error_not_a_panic() {
        let conn = crate::db::open_in_memory().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        assert!(build(&conn, i32::MAX, today).is_err());
        assert!(business_tax_share(&conn, i32::MIN, today).is_err());
    }
}
