use std::path::PathBuf;

use chrono::Datelike;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::i18n::tr;
use crate::customers::{address_block, get_customer, Customer};
use crate::db::{load_settings, AppState, Settings};
use crate::error::{msg, AppResult};
use crate::files::remove_file_quietly;
use crate::util::{fmt_money, iso, new_id, now_ts, parse_date, safe_file_name, today};
use crate::einvoice::{self, Profile};
use crate::{activities, latex, units, vault};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct LineItem {
    pub name: String,
    pub quantity: f64,
    pub price_cents: i64,
    #[serde(default)]
    pub unit: Option<String>,
}

impl LineItem {
    pub fn total_cents(&self) -> i64 {
        (self.quantity * self.price_cents as f64).round() as i64
    }

    pub fn is_hours(&self) -> bool {
        self.unit.as_deref() == Some(units::HOURS)
    }
}

pub fn fmt_hours(quantity: f64) -> String {
    let minutes = (quantity * 60.0).round() as i64;
    format!("{}{}:{:02}", if minutes < 0 { "-" } else { "" }, minutes.abs() / 60, minutes.abs() % 60)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Totals {
    pub subtotal_cents: i64,
    pub vat_cents: i64,
    pub amount_cents: i64,
}

pub fn compute_totals(lines: &[LineItem], vat_rate: f64, discount_cents: i64) -> Totals {
    let subtotal_cents: i64 = lines.iter().map(LineItem::total_cents).sum();
    let net = subtotal_cents - discount_cents;
    let vat_cents = (net as f64 * vat_rate / 100.0).round() as i64;
    Totals { subtotal_cents, vat_cents, amount_cents: net + vat_cents }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Invoice {
    pub id: String,
    pub invoice_number: String,
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
    pub customer_details: Option<String>,
    pub status: String,
    pub issue_date: String,
    pub due_date: String,
    pub currency: String,
    pub line_items: Vec<LineItem>,
    pub vat_rate: f64,
    pub discount_cents: i64,
    pub subtotal_cents: i64,
    pub vat_cents: i64,
    pub amount_cents: i64,
    pub note: Option<String>,
    pub internal_note: Option<String>,
    pub paid_at: Option<String>,
    pub file_path: Option<String>,
    pub created_at: String,
    pub order_number: Option<String>,
    pub customer_number: Option<String>,
    pub tracked_entries: i64,
}

const SELECT: &str = "SELECT i.id, i.invoice_number, i.customer_id, i.customer_name, i.customer_details,
    i.status, i.issue_date, i.due_date, i.currency, i.line_items, i.vat_rate,
    i.discount_cents, i.subtotal_cents, i.vat_cents, i.amount_cents, i.note, i.internal_note, i.paid_at,
    i.file_path, i.created_at, i.order_number, c.customer_number,
    (SELECT COUNT(*) FROM tracker_entries e WHERE e.invoice_id = i.id)
  FROM invoices i LEFT JOIN customers c ON c.id = i.customer_id";

fn map_row(r: &rusqlite::Row) -> rusqlite::Result<Invoice> {
    let lines: String = r.get(9)?;
    Ok(Invoice {
        id: r.get(0)?,
        invoice_number: r.get(1)?,
        customer_id: r.get(2)?,
        customer_name: r.get(3)?,
        customer_details: r.get(4)?,
        status: r.get(5)?,
        issue_date: r.get(6)?,
        due_date: r.get(7)?,
        currency: r.get(8)?,
        line_items: serde_json::from_str(&lines).unwrap_or_default(),
        vat_rate: r.get(10)?,
        discount_cents: r.get(11)?,
        subtotal_cents: r.get(12)?,
        vat_cents: r.get(13)?,
        amount_cents: r.get(14)?,
        note: r.get(15)?,
        internal_note: r.get(16)?,
        paid_at: r.get(17)?,
        file_path: r.get(18)?,
        created_at: r.get(19)?,
        order_number: r.get(20)?,
        customer_number: r.get(21)?,
        tracked_entries: r.get(22)?,
    })
}

pub fn load_invoice(conn: &Connection, id: &str) -> AppResult<Invoice> {
    conn.query_row(&format!("{SELECT} WHERE i.id = ?1"), params![id], map_row)
        .optional()?
        .ok_or_else(|| msg(tr!("Invoice not found", "Rechnung nicht gefunden")))
}

pub fn next_number(conn: &Connection, settings: &Settings, year: i32) -> AppResult<String> {
    let prefix = settings.invoice_prefix.replace("{YYYY}", &year.to_string());
    let mut stmt = conn.prepare("SELECT invoice_number FROM invoices WHERE invoice_number LIKE ?1 || '%'")?;
    let max = stmt
        .query_map(params![prefix], |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .filter_map(|n| n.strip_prefix(&prefix).and_then(|rest| rest.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    let digits = settings.invoice_number_digits.clamp(1, 10) as usize;
    Ok(format!("{prefix}{:0digits$}", max + 1))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct InvoiceInput {
    pub id: Option<String>,
    pub invoice_number: Option<String>,
    pub customer_id: Option<String>,
    pub issue_date: String,
    pub due_date: String,
    pub currency: String,
    pub line_items: Vec<LineItem>,
    pub vat_rate: f64,
    pub discount_cents: i64,
    pub note: Option<String>,
    pub internal_note: Option<String>,
    pub status: Option<String>,
    pub tracker_entry_ids: Vec<String>,
    pub order_number: Option<String>,
}

pub fn upsert_invoice(conn: &Connection, settings: &Settings, input: &InvoiceInput) -> AppResult<String> {
    let issue = parse_date(&input.issue_date).ok_or_else(|| msg(tr!("Invalid invoice date", "Ungültiges Rechnungsdatum")))?;
    let due = parse_date(&input.due_date).ok_or_else(|| msg(tr!("Invalid due date", "Ungültiges Fälligkeitsdatum")))?;
    if input.line_items.is_empty() {
        return Err(msg(tr!("The invoice needs at least one line item", "Die Rechnung braucht mindestens eine Position")));
    }
    let line_items = input
        .line_items
        .iter()
        .map(|li| Ok(LineItem { unit: units::parse(li.unit.as_deref())?, ..li.clone() }))
        .collect::<AppResult<Vec<_>>>()?;
    let totals = compute_totals(&line_items, input.vat_rate, input.discount_cents);
    let currency = if input.currency.is_empty() { settings.base_currency.clone() } else { input.currency.clone() };
    let customer_id = input.customer_id.clone().filter(|s| !s.is_empty());
    let (customer_name, customer_details) = match &customer_id {
        Some(cid) => {
            let c = get_customer(conn, cid)?;
            (Some(c.name.clone()), Some(address_block(&c, settings)))
        }
        None => (None, None),
    };
    let order_number = input.order_number.as_deref().map(str::trim).filter(|o| !o.is_empty()).map(String::from);
    let existing: Option<(String, String, Option<String>)> = match &input.id {
        Some(id) => conn
            .query_row("SELECT invoice_number, status, paid_at FROM invoices WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?,
        None => None,
    };
    let number = match input.invoice_number.clone().filter(|n| !n.trim().is_empty()) {
        Some(n) => n.trim().to_string(),
        None => match &existing {
            Some((number, _, _)) => number.clone(),
            None => next_number(conn, settings, issue.year())?,
        },
    };
    let taken: Option<String> = conn
        .query_row(
            "SELECT id FROM invoices WHERE invoice_number = ?1 AND id != COALESCE(?2, '')",
            params![number, input.id],
            |r| r.get(0),
        )
        .optional()?;
    if taken.is_some() {
        return Err(msg(tr!("Invoice number {number} is already taken", "Die Rechnungsnummer {number} ist bereits vergeben")));
    }
    let lines_json = serde_json::to_string(&line_items)?;
    let id = input.id.clone().unwrap_or_else(new_id);
    let (status, paid_at) = match (input.status.clone(), existing) {
        (Some(status), existing) => {
            let paid_at = (status == "paid").then(|| existing.and_then(|e| e.2).unwrap_or_else(today));
            (status, paid_at)
        }
        (None, Some((_, status, paid_at))) => (status, paid_at),
        (None, None) => ("draft".to_string(), None),
    };
    conn.execute(
        "INSERT INTO invoices (id, invoice_number, customer_id, customer_name, customer_details, status, issue_date,
            due_date, currency, line_items, vat_rate, discount_cents, subtotal_cents, vat_cents, amount_cents, note,
            internal_note, order_number, paid_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?19, ?20)
         ON CONFLICT(id) DO UPDATE SET invoice_number = excluded.invoice_number, customer_id = excluded.customer_id,
            customer_name = excluded.customer_name, customer_details = excluded.customer_details,
            status = excluded.status, issue_date = excluded.issue_date, due_date = excluded.due_date,
            currency = excluded.currency, line_items = excluded.line_items, vat_rate = excluded.vat_rate,
            discount_cents = excluded.discount_cents, subtotal_cents = excluded.subtotal_cents,
            vat_cents = excluded.vat_cents, amount_cents = excluded.amount_cents, note = excluded.note,
            internal_note = excluded.internal_note, order_number = excluded.order_number, paid_at = excluded.paid_at,
            updated_at = ?18",
        params![
            id, number, customer_id, customer_name, customer_details, status, iso(issue), iso(due), currency,
            lines_json, input.vat_rate, input.discount_cents, totals.subtotal_cents, totals.vat_cents, totals.amount_cents,
            input.note, input.internal_note, now_ts(), order_number, paid_at
        ],
    )?;
    for entry_id in &input.tracker_entry_ids {
        conn.execute(
            "UPDATE tracker_entries SET billed = 1, invoice_id = ?2 WHERE id = ?1",
            params![entry_id, id],
        )?;
    }
    Ok(id)
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct InvoiceFilter {
    pub status: Option<String>,
    pub search: Option<String>,
    pub customer_id: Option<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct StatusTotal {
    pub count: i64,
    pub cents: i64,
}

#[derive(Debug, Serialize)]
pub struct InvoiceList {
    pub items: Vec<Invoice>,
    pub draft: StatusTotal,
    pub open: StatusTotal,
    pub overdue: StatusTotal,
    pub paid: StatusTotal,
}

fn status_total(conn: &Connection, statuses: &str) -> AppResult<StatusTotal> {
    Ok(conn.query_row(
        &format!("SELECT COUNT(*), COALESCE(SUM(amount_cents), 0) FROM invoices WHERE status IN ({statuses})"),
        [],
        |r| Ok(StatusTotal { count: r.get(0)?, cents: r.get(1)? }),
    )?)
}

pub async fn list_invoices(state: &AppState, filter: InvoiceFilter) -> AppResult<InvoiceList> {
    let conn = state.conn()?;
    let pattern = format!("%{}%", filter.search.unwrap_or_default().trim());
    let status = filter.status.filter(|s| !s.is_empty() && s != "all");
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE (?1 IS NULL OR i.status = ?1 OR (?1 = 'open' AND i.status IN ('unpaid','overdue')))
           AND (?2 IS NULL OR i.customer_id = ?2)
           AND (i.invoice_number LIKE ?3 OR COALESCE(i.customer_name, '') LIKE ?3)
         ORDER BY i.issue_date DESC, i.invoice_number DESC"
    ))?;
    let items = stmt
        .query_map(params![status, filter.customer_id, pattern], map_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(InvoiceList {
        items,
        draft: status_total(&conn, "'draft'")?,
        open: status_total(&conn, "'unpaid','overdue'")?,
        overdue: status_total(&conn, "'overdue'")?,
        paid: status_total(&conn, "'paid'")?,
    })
}

pub async fn get_invoice(state: &AppState, id: String) -> AppResult<Invoice> {
    let conn = state.conn()?;
    load_invoice(&conn, &id)
}

pub async fn next_invoice_number(state: &AppState, issue_date: Option<String>) -> AppResult<String> {
    let conn = state.conn()?;
    let settings = load_settings(&conn)?;
    let year = issue_date.as_deref().and_then(parse_date).unwrap_or_else(crate::util::today_date).year();
    next_number(&conn, &settings, year)
}

pub async fn save_invoice(state: &AppState, input: InvoiceInput) -> AppResult<Invoice> {
    let conn = state.conn()?;
    let settings = load_settings(&conn)?;
    let is_new = input.id.is_none();
    let id = upsert_invoice(&conn, &settings, &input)?;
    if is_new && !input.tracker_entry_ids.is_empty() {
        timesheet_after_create(&conn, &state.files_dir, &id);
    }
    sync_vault(&conn, &state.files_dir, &id, true);
    let invoice = load_invoice(&conn, &id)?;
    if is_new {
        activities::log(
            &conn,
            "invoice_created",
            &tr!("Invoice {} created ({})", "Rechnung {} erstellt ({})", invoice.invoice_number, fmt_money(invoice.amount_cents, &invoice.currency)),
            Some(serde_json::json!({ "invoice_id": id })),
        )?;
    }
    Ok(invoice)
}

pub fn apply_status(conn: &Connection, id: &str, status: &str, paid_at: Option<&str>) -> AppResult<()> {
    match status {
        "paid" => conn.execute(
            "UPDATE invoices SET status = 'paid', paid_at = COALESCE(?2, ?3), updated_at = ?4 WHERE id = ?1",
            params![id, paid_at, today(), now_ts()],
        )?,
        _ => conn.execute(
            "UPDATE invoices SET status = ?2, paid_at = NULL, updated_at = ?3 WHERE id = ?1",
            params![id, status, now_ts()],
        )?,
    };
    Ok(())
}

const STATUSES: &[&str] = &["draft", "unpaid", "overdue", "paid", "canceled", "scheduled", "refunded"];

pub async fn set_invoice_status(state: &AppState, id: String, status: String, paid_at: Option<String>) -> AppResult<Invoice> {
    if !STATUSES.contains(&status.as_str()) {
        return Err(msg(tr!("Invalid status", "Ungültiger Status")));
    }
    let paid_at = match paid_at.filter(|d| !d.is_empty()) {
        Some(d) => Some(iso(parse_date(&d).ok_or_else(|| msg(tr!("Invalid payment date", "Ungültiges Zahlungsdatum")))?)),
        None => None,
    };
    let conn = state.conn()?;
    apply_status(&conn, &id, &status, paid_at.as_deref())?;
    mark_overdue(&conn)?;
    sync_vault(&conn, &state.files_dir, &id, false);
    let invoice = load_invoice(&conn, &id)?;
    let number = &invoice.invoice_number;
    let message = match status.as_str() {
        "paid" => tr!("Invoice {number} marked as paid", "Rechnung {number} als bezahlt markiert"),
        "canceled" => tr!("Invoice {number} canceled", "Rechnung {number} storniert"),
        "unpaid" => tr!("Invoice {number} marked as open", "Rechnung {number} als offen markiert"),
        "draft" => tr!("Invoice {number} set back to draft", "Rechnung {number} zurück auf Entwurf gesetzt"),
        _ => tr!("Invoice {number} updated", "Rechnung {number} aktualisiert"),
    };
    activities::log(&conn, &format!("invoice_{status}"), &message, None)?;
    Ok(invoice)
}

pub async fn delete_invoice(state: &AppState, id: String) -> AppResult<()> {
    let conn = state.conn()?;
    let invoice = load_invoice(&conn, &id)?;
    conn.execute("UPDATE tracker_entries SET billed = 0, invoice_id = NULL WHERE invoice_id = ?1", params![id])?;
    conn.execute("DELETE FROM invoices WHERE id = ?1", params![id])?;
    vault::prune_tags(&conn)?;
    if let Some(path) = invoice.file_path {
        remove_file_quietly(&path);
    }
    let number = safe_file_name(&invoice.invoice_number);
    for name in [format!("{number}-Timesheet.pdf"), format!("{number}-Zeiterfassung.pdf")] {
        remove_file_quietly(&state.files_dir.join("invoices").join(name).to_string_lossy());
    }
    Ok(())
}

pub async fn duplicate_invoice(state: &AppState, id: String) -> AppResult<Invoice> {
    let conn = state.conn()?;
    let settings = load_settings(&conn)?;
    let src = load_invoice(&conn, &id)?;
    let issue = crate::util::today_date();
    let input = InvoiceInput {
        customer_id: src.customer_id,
        issue_date: crate::util::iso(issue),
        due_date: crate::util::iso(issue + chrono::Duration::days(settings.default_payment_terms)),
        currency: src.currency,
        line_items: src.line_items,
        vat_rate: src.vat_rate,
        discount_cents: src.discount_cents,
        note: src.note,
        order_number: src.order_number,
        ..Default::default()
    };
    let new_id = upsert_invoice(&conn, &settings, &input)?;
    load_invoice(&conn, &new_id)
}

#[derive(Debug, Serialize)]
pub struct RenderedPdf {
    pub path: String,
    pub e_invoice_warning: Option<String>,
}

fn invoice_context(conn: &Connection, id: &str) -> AppResult<(Settings, Invoice, Option<Customer>)> {
    let settings = load_settings(conn)?;
    let invoice = load_invoice(conn, id)?;
    let customer = invoice.customer_id.as_deref().map(|customer_id| get_customer(conn, customer_id)).transpose()?;
    Ok((settings, invoice, customer))
}

pub fn render_pdf_for(conn: &Connection, files_dir: &std::path::Path, id: &str) -> AppResult<RenderedPdf> {
    let (settings, invoice, customer) = invoice_context(conn, id)?;
    let path = files_dir.join("invoices").join(format!("{}.pdf", safe_file_name(&invoice.invoice_number)));
    let tc = latex::toolchain(files_dir).map_err(msg)?;
    let xml = settings.e_invoice.then(|| einvoice::cii(&invoice, customer.as_ref(), &settings, Profile::Zugferd));
    latex::render_invoice(&tc, &invoice, customer.as_ref(), &settings, &path)?;
    let title = tr!("Invoice {}", "Rechnung {}", invoice.invoice_number);
    let e_invoice_warning = xml
        .and_then(|xml| xml.and_then(|xml| einvoice::embed(&path, &xml, &title, &einvoice::seller_name(&settings))).err())
        .map(|e| tr!("The PDF was created without an e-invoice. {e}", "Die PDF wurde ohne E-Rechnung erstellt. {e}"));
    let new_path = path.to_string_lossy().to_string();
    conn.execute("UPDATE invoices SET file_path = ?2 WHERE id = ?1", params![id, new_path])?;
    if let Some(old) = invoice.file_path.filter(|old| *old != new_path) {
        remove_file_quietly(&old);
    }
    vault::sync_invoice(conn, id)?;
    Ok(RenderedPdf { path: new_path, e_invoice_warning })
}

pub fn sync_vault(conn: &Connection, files_dir: &std::path::Path, id: &str, rerender: bool) {
    let result = (|| -> AppResult<()> {
        let row: Option<(String, Option<String>)> = conn
            .query_row("SELECT status, file_path FROM invoices WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let Some((status, file_path)) = row else { return Ok(()) };
        let has_pdf = file_path.is_some_and(|p| std::path::Path::new(&p).is_file());
        if vault::is_issued(&status) && (rerender || (!has_pdf && !vault::has_invoice(conn, id)?)) {
            render_pdf_for(conn, files_dir, id).map(|_| ())
        } else {
            vault::sync_invoice(conn, id)
        }
    })();
    if let Err(e) = result {
        eprintln!("[vault] Invoice {id} was not synced to the vault: {e}");
    }
}

pub fn render_timesheet_for(conn: &Connection, files_dir: &std::path::Path, id: &str) -> AppResult<Option<PathBuf>> {
    let entries = crate::tracker::timesheet_of_invoice(conn, id)?;
    let Some(first) = entries.iter().filter_map(|e| parse_date(&e.date)).min() else { return Ok(None) };
    let settings = load_settings(conn)?;
    let invoice = load_invoice(conn, id)?;
    let tc = latex::toolchain(files_dir).map_err(msg)?;
    let path = files_dir.join("invoices").join(tr!("{}-Timesheet.pdf", "{}-Zeiterfassung.pdf", safe_file_name(&invoice.invoice_number)));
    latex::render_timesheet(&tc, &entries, &tr!("Attachment to invoice {}", "Anlage zu Rechnung {}", invoice.invoice_number), first, &settings, &path)?;
    Ok(Some(path))
}

pub fn timesheet_after_create(conn: &Connection, files_dir: &std::path::Path, id: &str) {
    if let Err(e) = render_timesheet_for(conn, files_dir, id) {
        eprintln!("[timesheet] Timesheet for the invoice was not created: {e}");
    }
}

pub async fn render_invoice_timesheet(state: &AppState, id: String) -> AppResult<Option<String>> {
    let conn = state.conn()?;
    Ok(render_timesheet_for(&conn, &state.files_dir, &id)?.map(|p| p.to_string_lossy().to_string()))
}

pub async fn render_invoice_pdf(state: &AppState, id: String) -> AppResult<RenderedPdf> {
    let conn = state.conn()?;
    render_pdf_for(&conn, &state.files_dir, &id)
}

pub async fn export_invoice_pdf(state: &AppState, id: String, target: String) -> AppResult<Option<String>> {
    let conn = state.conn()?;
    let rendered = render_pdf_for(&conn, &state.files_dir, &id)?;
    std::fs::copy(&rendered.path, target)?;
    Ok(rendered.e_invoice_warning)
}

pub async fn export_invoice_xrechnung(state: &AppState, id: String, target: String) -> AppResult<()> {
    let conn = state.conn()?;
    let (settings, invoice, customer) = invoice_context(&conn, &id)?;
    let xml = einvoice::cii(&invoice, customer.as_ref(), &settings, Profile::XRechnung)?;
    std::fs::write(target, xml)?;
    Ok(())
}

pub fn mark_overdue(conn: &Connection) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT invoice_number FROM invoices WHERE status = 'unpaid' AND due_date < ?1")?;
    let numbers: Vec<String> = stmt.query_map(params![today()], |r| r.get(0))?.collect::<Result<_, _>>()?;
    conn.execute("UPDATE invoices SET status = 'overdue' WHERE status = 'unpaid' AND due_date < ?1", params![today()])?;
    Ok(numbers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(q: f64, p: i64) -> LineItem {
        LineItem { name: "x".into(), quantity: q, price_cents: p, unit: None }
    }

    #[test]
    fn totals_with_vat_and_discount() {
        let t = compute_totals(&[line(2.0, 10000), line(1.5, 8000)], 19.0, 2000);
        assert_eq!(t.subtotal_cents, 32000);
        assert_eq!(t.vat_cents, 5700);
        assert_eq!(t.amount_cents, 35700);
    }

    #[test]
    fn tracked_hours_stay_exact_and_print_as_clock_time() {
        let tracked = LineItem { name: "x".into(), quantity: 38100.0 / 3600.0, price_cents: 7000, unit: Some(units::HOURS.into()) };
        assert!(tracked.is_hours());
        assert_eq!(fmt_hours(tracked.quantity), "10:35");
        assert_eq!(tracked.total_cents(), 74083);
        assert_eq!(fmt_hours(16.5), "16:30");
        assert!(!LineItem { unit: Some(units::PIECES.into()), ..tracked }.is_hours());
    }

    #[test]
    fn numbering_and_payment_date() {
        let conn = crate::db::open_in_memory().unwrap();
        let settings = Settings::default();
        conn.execute("INSERT INTO customers (id, name) VALUES ('c1', 'Acme Software GmbH')", []).unwrap();
        let input = InvoiceInput {
            customer_id: Some("c1".into()),
            issue_date: "2026-09-01".into(),
            due_date: "2026-09-15".into(),
            currency: "EUR".into(),
            line_items: vec![line(10.0, 10000)],
            vat_rate: 19.0,
            status: Some("unpaid".into()),
            ..Default::default()
        };
        let id = upsert_invoice(&conn, &settings, &input).unwrap();
        let inv = load_invoice(&conn, &id).unwrap();
        assert_eq!(inv.invoice_number, "INV-2026-0001");
        assert_eq!(inv.amount_cents, 119000);
        assert_eq!(next_number(&conn, &settings, 2026).unwrap(), "INV-2026-0002");

        apply_status(&conn, &id, "paid", Some("2026-09-10")).unwrap();
        let inv = load_invoice(&conn, &id).unwrap();
        assert_eq!((inv.status.as_str(), inv.paid_at.as_deref()), ("paid", Some("2026-09-10")));
        apply_status(&conn, &id, "paid", Some("2026-09-12")).unwrap();
        assert_eq!(load_invoice(&conn, &id).unwrap().paid_at.as_deref(), Some("2026-09-12"), "a corrected date replaces the old one");
        apply_status(&conn, &id, "unpaid", None).unwrap();
        assert_eq!(load_invoice(&conn, &id).unwrap().paid_at, None);
    }

    #[test]
    fn saving_rejects_unknown_units() {
        let conn = crate::db::open_in_memory().unwrap();
        let input = InvoiceInput { issue_date: "2026-09-01".into(), due_date: "2026-09-15".into(), line_items: vec![LineItem { unit: Some("Seiten".into()), ..line(1.0, 100) }], ..Default::default() };
        assert!(upsert_invoice(&conn, &Settings::default(), &input).is_err());
    }

    #[test]
    fn saving_without_status_keeps_status_and_payment_date() {
        let conn = crate::db::open_in_memory().unwrap();
        let settings = Settings::default();
        let input = InvoiceInput {
            issue_date: "2026-09-01".into(),
            due_date: "2026-09-15".into(),
            currency: "EUR".into(),
            line_items: vec![line(1.0, 10000)],
            ..Default::default()
        };
        let id = upsert_invoice(&conn, &settings, &input).unwrap();
        apply_status(&conn, &id, "paid", Some("2026-09-10")).unwrap();

        upsert_invoice(&conn, &settings, &InvoiceInput { id: Some(id.clone()), note: Some("edited".into()), ..input.clone() }).unwrap();
        let inv = load_invoice(&conn, &id).unwrap();
        assert_eq!((inv.status.as_str(), inv.paid_at.as_deref(), inv.note.as_deref()), ("paid", Some("2026-09-10"), Some("edited")));

        upsert_invoice(&conn, &settings, &InvoiceInput { id: Some(id.clone()), status: Some("unpaid".into()), ..input }).unwrap();
        let inv = load_invoice(&conn, &id).unwrap();
        assert_eq!((inv.status.as_str(), inv.paid_at), ("unpaid", None), "leaving paid clears the payment date");
    }
}
