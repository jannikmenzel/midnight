use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::path::Path;

use rusqlite::{params, Connection};
use serde::Serialize;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::activities;
use crate::db::AppState;
use crate::error::{msg, AppResult, Context};
use crate::files::remove_file_quietly;
use crate::i18n::{is_de, tr};
use crate::ledger::csv_amount;
use crate::tax;
use crate::util::{fmt_date_de, fmt_money, safe_file_name};

#[derive(Debug, Serialize)]
pub struct TaxExport {
    pub invoices: usize,
    pub receipts: usize,
    pub missing_invoices: usize,
    pub missing_receipts: usize,
}

struct PaidInvoice {
    number: String,
    issue_date: String,
    customer: String,
    paid_at: String,
    vat_cents: i64,
    amount_cents: i64,
    file_path: Option<String>,
}

fn date(iso: &str) -> String {
    if is_de() { fmt_date_de(iso) } else { iso.to_string() }
}

fn csv_bytes(header: &[String], rows: &[Vec<String>]) -> AppResult<Vec<u8>> {
    let what = tr!("Writing CSV", "CSV schreiben");
    let mut wtr = csv::WriterBuilder::new().delimiter(if is_de() { b';' } else { b',' }).from_writer(vec![0xEF, 0xBB, 0xBF]);
    wtr.write_record(header).ctx(&what)?;
    for row in rows {
        wtr.write_record(row).ctx(&what)?;
    }
    wtr.into_inner().map_err(|e| msg(e.to_string()))
}

fn unique(used: &mut HashSet<String>, dir: &str, stem: &str, ext: &str) -> String {
    let ext = if ext.is_empty() { String::new() } else { format!(".{ext}") };
    let mut name = format!("{dir}/{stem}{ext}");
    let mut n = 2;
    while !used.insert(name.to_lowercase()) {
        name = format!("{dir}/{stem} {n}{ext}");
        n += 1;
    }
    name
}

fn extension(path: &str) -> String {
    Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn paid_invoices(conn: &Connection, from: &str, to: &str) -> AppResult<Vec<PaidInvoice>> {
    let mut stmt = conn.prepare(
        "SELECT invoice_number, issue_date, customer_name, substr(paid_at, 1, 10), vat_cents, amount_cents, file_path
         FROM invoices WHERE status = 'paid' AND substr(paid_at, 1, 10) BETWEEN ?1 AND ?2 ORDER BY paid_at, invoice_number",
    )?;
    let rows = stmt
        .query_map(params![from, to], |r| {
            Ok(PaidInvoice {
                number: r.get(0)?,
                issue_date: r.get(1)?,
                customer: r.get::<_, Option<String>>(2)?.unwrap_or_else(|| tr!("No customer", "Ohne Kunde")),
                paid_at: r.get(3)?,
                vat_cents: r.get(4)?,
                amount_cents: r.get(5)?,
                file_path: r.get(6)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

fn receipts_by_expense(conn: &Connection, from: &str, to: &str) -> AppResult<BTreeMap<String, Vec<String>>> {
    let mut stmt = conn.prepare(
        "SELECT r.expense_id, r.path FROM receipts r JOIN expenses e ON e.id = r.expense_id
         WHERE e.date BETWEEN ?1 AND ?2 ORDER BY r.created_at, r.id",
    )?;
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in stmt.query_map(params![from, to], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (expense_id, path) = row?;
        map.entry(expense_id).or_default().push(path);
    }
    Ok(map)
}

pub struct Bundle {
    pub counts: TaxExport,
    documents: Vec<(String, Vec<u8>)>,
    files: Vec<(String, String)>,
}

fn file_stem(s: &str) -> String {
    safe_file_name(s).chars().take(80).collect::<String>().trim_end().to_string()
}

fn text(s: &str) -> String {
    if s.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{s}") } else { s.to_string() }
}

pub fn collect(conn: &Connection, year: i32) -> AppResult<Bundle> {
    let report = tax::compute(conn, year)?;
    let (from, to) = (format!("{year}-01-01"), format!("{year}-12-31"));
    let invoices = paid_invoices(conn, &from, &to)?;
    let receipts = receipts_by_expense(conn, &from, &to)?;

    let invoice_dir = tr!("Invoices", "Rechnungen");
    let receipt_dir = tr!("Receipts", "Belege");
    let mut used = HashSet::new();
    let mut files: Vec<(String, String)> = Vec::new();

    let mut income_rows = Vec::new();
    let mut missing_invoices = 0;
    for inv in &invoices {
        let pdf = inv.file_path.as_deref().filter(|p| Path::new(p).is_file()).map(|p| {
            let name = unique(&mut used, &invoice_dir, &file_stem(&inv.number), "pdf");
            files.push((name.clone(), p.to_string()));
            name
        });
        if pdf.is_none() {
            missing_invoices += 1;
        }
        income_rows.push(vec![
            date(&inv.paid_at),
            text(&inv.number),
            date(&inv.issue_date),
            text(&inv.customer),
            csv_amount(inv.amount_cents - inv.vat_cents),
            csv_amount(inv.vat_cents),
            csv_amount(inv.amount_cents),
            pdf.unwrap_or_default(),
        ]);
    }

    let mut expense_rows = Vec::new();
    let (mut receipt_count, mut missing_receipts) = (0, 0);
    for e in &report.expenses {
        let names: Vec<String> = receipts
            .get(&e.id)
            .into_iter()
            .flatten()
            .filter(|p| Path::new(p).is_file())
            .map(|p| {
                let name = unique(&mut used, &receipt_dir, &format!("{} {}", e.date, file_stem(&e.name)), &extension(p));
                files.push((name.clone(), p.clone()));
                name
            })
            .collect();
        if names.is_empty() {
            missing_receipts += 1;
        }
        receipt_count += names.len();
        expense_rows.push(vec![
            date(&e.date),
            text(&e.name),
            text(&e.category),
            e.euer_line.clone(),
            csv_amount(e.amount_cents),
            csv_amount(e.deductible_cents),
            names.join(", "),
        ]);
    }

    let vat: i64 = invoices.iter().map(|i| i.vat_cents).sum();
    let blank = || vec![String::new(), String::new()];
    let mut summary = vec![vec![tr!("Business income (net)", "Betriebseinnahmen (netto)"), csv_amount(report.income_cents - vat)]];
    if vat != 0 {
        summary.push(vec![tr!("VAT received", "Vereinnahmte Umsatzsteuer"), csv_amount(vat)]);
    }
    summary.push(vec![tr!("Total business income", "Summe Betriebseinnahmen"), csv_amount(report.income_cents)]);
    summary.push(blank());
    for (line, cents) in &report.expenses_by_line {
        summary.push(vec![line.clone(), csv_amount(*cents)]);
    }
    if report.home_office_business_cents > 0 {
        let rate = fmt_money(report.params.home_office_per_day, "EUR");
        let days = report.home_office_business_days;
        summary.push(vec![tr!("Home office allowance ({days} days × {rate})", "Homeoffice-Tagespauschale ({days} Tage × {rate})"), csv_amount(report.home_office_business_cents)]);
    }
    summary.push(vec![tr!("Total business expenses", "Summe Betriebsausgaben"), csv_amount(report.expenses_cents + report.home_office_business_cents)]);
    summary.push(blank());
    summary.push(vec![tr!("Profit", "Gewinn"), csv_amount(report.profit_cents)]);

    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let documents = vec![
        (
            tr!("Profit statement {year}.csv", "EÜR {year}.csv"),
            csv_bytes(&if is_de() { s(&["Position", "Betrag"]) } else { s(&["Item", "Amount"]) }, &summary)?,
        ),
        (
            tr!("Income {year}.csv", "Einnahmen {year}.csv"),
            csv_bytes(
                &if is_de() {
                    s(&["Bezahlt am", "Rechnung", "Rechnungsdatum", "Kunde", "Netto", "Umsatzsteuer", "Brutto", "PDF"])
                } else {
                    s(&["Paid on", "Invoice", "Invoice date", "Customer", "Net", "VAT", "Gross", "PDF"])
                },
                &income_rows,
            )?,
        ),
        (
            tr!("Expenses {year}.csv", "Ausgaben {year}.csv"),
            csv_bytes(
                &if is_de() {
                    s(&["Datum", "Name", "Kategorie", "EÜR-Position", "Betrag", "Abziehbar", "Belege"])
                } else {
                    s(&["Date", "Name", "Category", "EÜR item", "Amount", "Deductible", "Receipts"])
                },
                &expense_rows,
            )?,
        ),
    ];

    let counts = TaxExport { invoices: invoices.len() - missing_invoices, receipts: receipt_count, missing_invoices, missing_receipts };
    Ok(Bundle { counts, documents, files })
}

pub fn write_zip(bundle: &Bundle, path: &Path) -> AppResult<()> {
    let file = File::create(path)?;
    let result = (|| -> AppResult<()> {
        let what = tr!("Writing ZIP", "ZIP schreiben");
        let now = zip::DateTime::try_from(chrono::Local::now().naive_local()).unwrap_or_default();
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated).last_modified_time(now);
        let mut zip = ZipWriter::new(file);
        for (name, bytes) in &bundle.documents {
            zip.start_file(name.as_str(), opts).ctx(&what)?;
            zip.write_all(bytes)?;
        }
        for (name, src) in &bundle.files {
            zip.start_file(name.as_str(), opts).ctx(&what)?;
            std::io::copy(&mut File::open(src)?, &mut zip)?;
        }
        zip.finish().ctx(&what)?;
        Ok(())
    })();
    if result.is_err() {
        remove_file_quietly(&path.to_string_lossy());
    }
    result
}

pub async fn export_tax_year(state: &AppState, year: i32, path: String) -> AppResult<TaxExport> {
    let bundle = collect(&*state.conn()?, year)?;
    write_zip(&bundle, Path::new(&path))?;
    activities::log(&*state.conn()?, "tax_exported", &tr!("Tax documents for {year} exported", "Steuerunterlagen {year} exportiert"), None)?;
    Ok(bundle.counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn bundles_statement_lists_and_files() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("RE-1.pdf");
        let receipt = dir.path().join("upload.PDF");
        std::fs::write(&pdf, b"invoice").unwrap();
        std::fs::write(&receipt, b"receipt").unwrap();

        let conn = crate::db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO invoices (id, invoice_number, customer_name, status, issue_date, due_date, vat_cents, amount_cents, paid_at, file_path) VALUES
               ('a', 'RE-1', 'Acme GmbH', 'paid', '2026-03-01', '2026-03-15', 19000, 119000, '2026-03-10', ?1),
               ('b', 'RE-2', 'Acme GmbH', 'paid', '2026-04-01', '2026-04-15', 0, 50000, '2026-04-10', NULL),
               ('c', 'RE-3', 'Acme GmbH', 'paid', '2025-12-01', '2025-12-15', 0, 99900, '2025-12-20', NULL)",
            [pdf.to_string_lossy()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO expenses (id, date, name, amount_cents, category_slug) VALUES
               ('e1', '2026-04-01', 'JetBrains', 10000, 'software'),
               ('e2', '2026-04-01', 'JetBrains', 5000, 'software'),
               ('e3', '2026-05-02', 'Kundenessen', 10000, 'meals'),
               ('e4', '2026-06-01', ?1, 100, 'other')",
            [format!("=HYPERLINK(\"x\") {}", "a".repeat(300))],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO receipts (id, expense_id, name, path) VALUES ('r1', 'e1', 'a.pdf', ?1), ('r2', 'e2', 'b.pdf', ?1), ('r3', 'e3', 'c.pdf', '/missing.pdf'), ('r4', 'e4', 'd.pdf', ?1)",
            [receipt.to_string_lossy()],
        )
        .unwrap();

        let out = dir.path().join("export.zip");
        let bundle = collect(&conn, 2026).unwrap();
        write_zip(&bundle, &out).unwrap();
        let r = &bundle.counts;
        assert_eq!((r.invoices, r.missing_invoices), (1, 1), "only invoices paid in 2026, RE-2 has no PDF");
        assert_eq!((r.receipts, r.missing_receipts), (3, 1), "the receipt of e3 is gone");

        let mut zip = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
        let names: Vec<String> = zip.file_names().map(String::from).collect();
        for name in ["Profit statement 2026.csv", "Income 2026.csv", "Expenses 2026.csv", "Invoices/RE-1.pdf", "Receipts/2026-04-01 JetBrains.pdf", "Receipts/2026-04-01 JetBrains 2.pdf"] {
            assert!(names.contains(&name.to_string()), "{name} missing in {names:?}");
        }
        let mut summary = String::new();
        zip.by_name("Profit statement 2026.csv").unwrap().read_to_string(&mut summary).unwrap();
        assert!(summary.contains("Business income (net),1500.00"));
        assert!(summary.contains("VAT received,190.00"));
        assert!(summary.contains("Total business expenses,221.00"), "meals at 70 %: {summary}");
        assert!(summary.contains("Profit,1469.00"));

        let mut expenses = String::new();
        zip.by_name("Expenses 2026.csv").unwrap().read_to_string(&mut expenses).unwrap();
        assert!(expenses.contains("'=HYPERLINK"), "formulas are neutralised: {expenses}");
        assert!(names.iter().all(|n| n.len() < 120), "long names are shortened: {names:?}");
    }

    #[test]
    fn failed_write_keeps_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("locked.zip");
        std::fs::write(&existing, b"keep").unwrap();
        let mut perms = std::fs::metadata(&existing).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&existing, perms).unwrap();

        let conn = crate::db::open_in_memory().unwrap();
        assert!(write_zip(&collect(&conn, 2026).unwrap(), &existing).is_err());
        assert_eq!(std::fs::read(&existing).unwrap(), b"keep");
    }
}
