use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::i18n::tr;
use crate::activities;
use crate::db::AppState;
use crate::error::{msg, AppResult, Context};
use crate::util::fmt_date_de;

#[derive(Debug, Serialize, Clone)]
pub struct LedgerEntry {
    pub kind: String,
    pub id: String,
    pub date: String,
    pub name: String,
    pub description: Option<String>,
    pub amount_cents: i64,
    pub currency: String,
    pub category_slug: Option<String>,
    pub category_name: Option<String>,
    pub category_color: Option<String>,
    pub invoice_number: Option<String>,
    pub receipt_count: i64,
    pub recurring: bool,
}

#[derive(Debug, Deserialize, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct LedgerFilter {
    pub kind: Option<String>,
    pub category: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub search: Option<String>,
    pub missing_receipt: bool,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct LedgerPage {
    pub items: Vec<LedgerEntry>,
    pub total: usize,
    pub income_cents: i64,
    pub expense_cents: i64,
}

fn non_empty(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

fn income(conn: &Connection, f: &LedgerFilter) -> AppResult<Vec<LedgerEntry>> {
    if f.kind.as_deref() == Some("expense") || non_empty(&f.category).is_some() || f.missing_receipt {
        return Ok(Vec::new());
    }
    let like = non_empty(&f.search).map(|s| format!("%{s}%"));
    let mut stmt = conn.prepare(
        "SELECT id, paid_at, COALESCE(customer_name, ?4), invoice_number, amount_cents, currency FROM invoices
         WHERE status = 'paid' AND paid_at IS NOT NULL
           AND (?1 IS NULL OR paid_at >= ?1) AND (?2 IS NULL OR paid_at <= ?2)
           AND (?3 IS NULL OR invoice_number LIKE ?3 OR COALESCE(customer_name, '') LIKE ?3 OR line_items LIKE ?3)",
    )?;
    let rows = stmt.query_map(params![non_empty(&f.from), non_empty(&f.to), like, tr!("No customer", "Ohne Kunde")], |r| {
        let number: String = r.get(3)?;
        Ok(LedgerEntry {
            kind: "income".into(),
            id: r.get(0)?,
            date: r.get(1)?,
            name: r.get(2)?,
            description: Some(tr!("Invoice {number}", "Rechnung {number}")),
            amount_cents: r.get(4)?,
            currency: r.get(5)?,
            category_slug: None,
            category_name: None,
            category_color: None,
            invoice_number: Some(number),
            receipt_count: 0,
            recurring: false,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn expenses(conn: &Connection, f: &LedgerFilter) -> AppResult<Vec<LedgerEntry>> {
    if f.kind.as_deref() == Some("income") {
        return Ok(Vec::new());
    }
    let like = non_empty(&f.search).map(|s| format!("%{s}%"));
    let mut stmt = conn.prepare(
        "SELECT e.id, e.date, e.name, e.description, e.amount_cents, e.currency, e.category_slug, c.name, c.color,
                (SELECT COUNT(*) FROM receipts r WHERE r.expense_id = e.id) AS receipts, e.expense_recurring_id IS NOT NULL
         FROM expenses e LEFT JOIN categories c ON c.slug = e.category_slug
         WHERE (?1 IS NULL OR e.date >= ?1) AND (?2 IS NULL OR e.date <= ?2)
           AND (?3 IS NULL OR e.name LIKE ?3 OR COALESCE(e.description, '') LIKE ?3 OR COALESCE(e.note, '') LIKE ?3)
           AND (?4 IS NULL OR (?4 = 'uncategorized' AND e.category_slug IS NULL) OR e.category_slug = ?4)
           AND (?5 = 0 OR receipts = 0)",
    )?;
    let rows = stmt.query_map(
        params![non_empty(&f.from), non_empty(&f.to), like, non_empty(&f.category), f.missing_receipt],
        |r| {
            Ok(LedgerEntry {
                kind: "expense".into(),
                id: r.get(0)?,
                date: r.get(1)?,
                name: r.get(2)?,
                description: r.get(3)?,
                amount_cents: -r.get::<_, i64>(4)?,
                currency: r.get(5)?,
                category_slug: r.get(6)?,
                category_name: r.get(7)?,
                category_color: r.get(8)?,
                invoice_number: None,
                receipt_count: r.get(9)?,
                recurring: r.get(10)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn query_ledger(conn: &Connection, f: &LedgerFilter) -> AppResult<LedgerPage> {
    let mut items = income(conn, f)?;
    items.extend(expenses(conn, f)?);
    items.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.kind.cmp(&b.kind)));
    let income_cents = items.iter().filter(|e| e.amount_cents > 0).map(|e| e.amount_cents).sum();
    let expense_cents = -items.iter().filter(|e| e.amount_cents < 0).map(|e| e.amount_cents).sum::<i64>();
    let total = items.len();
    items.truncate(f.limit.unwrap_or(500));
    Ok(LedgerPage { items, total, income_cents, expense_cents })
}

pub async fn list_ledger(state: &AppState, filter: LedgerFilter) -> AppResult<LedgerPage> {
    let conn = state.conn()?;
    query_ledger(&conn, &filter)
}

fn csv_amount(cents: i64) -> String {
    let sep = if crate::i18n::is_de() { ',' } else { '.' };
    format!("{}{}{sep}{:02}", if cents < 0 { "-" } else { "" }, cents.unsigned_abs() / 100, cents.unsigned_abs() % 100)
}

pub async fn export_ledger_csv(state: &AppState, filter: LedgerFilter, path: String) -> AppResult<usize> {
    let conn = state.conn()?;
    let mut f = filter;
    f.limit = Some(usize::MAX);
    let mut page = query_ledger(&conn, &f)?;
    page.items.reverse();

    let de = crate::i18n::is_de();
    let mut wtr = csv::WriterBuilder::new().delimiter(if de { b';' } else { b',' }).from_writer(Vec::new());
    let header = if de {
        ["Datum", "Art", "Name", "Beschreibung", "Kategorie", "Betrag", "Währung", "Rechnung", "Belege"]
    } else {
        ["Date", "Type", "Name", "Description", "Category", "Amount", "Currency", "Invoice", "Receipts"]
    };
    wtr.write_record(header).ctx(&tr!("Writing CSV", "CSV schreiben"))?;
    for e in &page.items {
        let receipts = if e.kind == "expense" { e.receipt_count.to_string() } else { String::new() };
        wtr.write_record([
            if de { fmt_date_de(&e.date) } else { e.date.clone() },
            if e.kind == "income" { tr!("Income", "Einnahme") } else { tr!("Expense", "Ausgabe") },
            e.name.clone(),
            e.description.clone().unwrap_or_default(),
            e.category_name.clone().unwrap_or_default(),
            csv_amount(e.amount_cents),
            e.currency.clone(),
            e.invoice_number.clone().unwrap_or_default(),
            receipts,
        ])
        .ctx(&tr!("Writing CSV", "CSV schreiben"))?;
    }
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend(wtr.into_inner().map_err(|e| msg(e.to_string()))?);
    std::fs::write(&path, bytes)?;
    activities::log(&conn, "ledger_exported", &tr!("{} transactions exported", "{} Buchungen exportiert", page.items.len()), None)?;
    Ok(page.items.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO invoices (id, invoice_number, customer_name, status, issue_date, due_date, amount_cents, paid_at)
               VALUES ('i1', 'INV-001', 'Acme GmbH', 'paid', '2026-09-01', '2026-09-15', 100000, '2026-09-10'),
                      ('i2', 'INV-002', 'Acme GmbH', 'unpaid', '2026-09-20', '2026-10-04', 50000, NULL);
             INSERT INTO expenses (id, date, name, amount_cents, category_slug) VALUES
               ('e1', '2026-09-12', 'Figma', 1440, 'software'),
               ('e2', '2026-08-30', 'Bahn', 2990, NULL);
             INSERT INTO receipts (id, expense_id, name, path) VALUES ('r1', 'e1', 'figma.pdf', '/x');",
        )
        .unwrap();
        conn
    }

    #[test]
    fn combines_paid_invoices_and_expenses() {
        let conn = setup();
        let page = query_ledger(&conn, &LedgerFilter::default()).unwrap();
        let ids: Vec<&str> = page.items.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["e1", "i1", "e2"], "newest first, unpaid invoices are not income");
        assert_eq!(page.income_cents, 100000);
        assert_eq!(page.expense_cents, 4430);
        assert_eq!(page.items[1].date, "2026-09-10", "income counts on the payment date");
    }

    #[test]
    fn filters_by_kind_period_category_and_receipt() {
        let conn = setup();
        let f = |f: LedgerFilter| query_ledger(&conn, &f).unwrap().items.into_iter().map(|e| e.id).collect::<Vec<_>>();
        assert_eq!(f(LedgerFilter { kind: Some("income".into()), ..Default::default() }), ["i1"]);
        assert_eq!(f(LedgerFilter { from: Some("2026-09-01".into()), kind: Some("expense".into()), ..Default::default() }), ["e1"]);
        assert_eq!(f(LedgerFilter { category: Some("uncategorized".into()), ..Default::default() }), ["e2"]);
        assert_eq!(f(LedgerFilter { missing_receipt: true, ..Default::default() }), ["e2"]);
        assert_eq!(f(LedgerFilter { search: Some("inv-001".into()), ..Default::default() }), ["i1"]);
    }
}
