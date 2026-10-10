use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{Datelike, NaiveDate};

use crate::customers::Customer;
use crate::db::Settings;
use crate::error::{msg, AppResult};
use crate::i18n::{fmt_date, is_de, pick, tr};
use crate::invoices::{fmt_hours, Invoice};
use crate::templates::{self, INVOICE, TIMESHEET};
use crate::util::{currency_symbol, new_id, parse_date};

pub fn find_xelatex() -> Option<PathBuf> {
    let from_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).map(|d| d.join("xelatex")).collect::<Vec<_>>())
        .unwrap_or_default();
    from_path
        .into_iter()
        .chain(["/Library/TeX/texbin/xelatex", "/opt/homebrew/bin/xelatex", "/usr/local/bin/xelatex", "/usr/bin/xelatex"].map(PathBuf::from))
        .find(|p| p.is_file())
}

pub struct Toolchain {
    xelatex: PathBuf,
    dir: PathBuf,
}

pub fn toolchain(files_dir: &Path) -> Result<Toolchain, String> {
    let xelatex = find_xelatex().ok_or_else(|| tr!("No LaTeX installation found (xelatex). Please install MacTeX or TeX Live.", "Keine LaTeX-Installation gefunden (xelatex). Bitte MacTeX oder TeX Live installieren."))?;
    let dir = templates::ensure(files_dir).map_err(|e| e.to_string())?;
    Ok(Toolchain { xelatex, dir })
}

impl Toolchain {
    fn template(&self, name: &str) -> AppResult<String> {
        std::fs::read_to_string(self.dir.join(name)).map_err(|e| msg(tr!("Reading {name}: {e}", "{name} lesen: {e}")))
    }
}

pub fn render_invoice(tc: &Toolchain, inv: &Invoice, customer: Option<&Customer>, s: &Settings, out: &Path) -> AppResult<()> {
    let tex = invoice_tex(&tc.template(INVOICE)?, inv, customer, s)?;
    compile(tc, &tex, 1, out)
}

pub fn render_timesheet(tc: &Toolchain, entries: &[TimesheetEntry], label: &str, from: NaiveDate, s: &Settings, out: &Path) -> AppResult<()> {
    let tex = timesheet_tex(&tc.template(TIMESHEET)?, entries, label, from, s)?;
    compile(tc, &tex, 2, out)
}

fn compile(tc: &Toolchain, tex: &str, runs: u8, out: &Path) -> AppResult<()> {
    let job = std::env::temp_dir().join(format!("midnight-tex-{}", new_id()));
    std::fs::create_dir_all(&job)?;
    let result = (|| -> AppResult<()> {
        for entry in std::fs::read_dir(&tc.dir)? {
            let entry = entry?;
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with("doc.") {
                symlink(&entry.path(), &job.join(&name))?;
            }
        }
        std::fs::write(job.join("doc.tex"), tex)?;

        let mut dirs: Vec<PathBuf> = tc.xelatex.parent().map(Path::to_path_buf).into_iter().collect();
        dirs.extend(std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default());
        let path = std::env::join_paths(dirs).map_err(|_| msg(tr!("Could not set PATH", "PATH konnte nicht gesetzt werden")))?;
        for _ in 0..runs.max(1) {
            let output = Command::new(&tc.xelatex)
                .args(["-no-shell-escape", "-interaction=nonstopmode", "-halt-on-error", "-file-line-error", "doc.tex"])
                .current_dir(&job)
                .env("PATH", &path)
                .output()?;
            if !output.status.success() {
                return Err(msg(tr!("LaTeX error: {}", "LaTeX-Fehler: {}", first_error(&job.join("doc.log")))));
            }
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(job.join("doc.pdf"), out)?;
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&job);
    result
}

#[cfg(unix)]
fn symlink(src: &Path, dst: &Path) -> AppResult<()> {
    Ok(std::os::unix::fs::symlink(src, dst)?)
}

#[cfg(windows)]
fn symlink(src: &Path, dst: &Path) -> AppResult<()> {
    if src.is_dir() {
        Ok(std::os::windows::fs::symlink_dir(src, dst)?)
    } else {
        Ok(std::os::windows::fs::symlink_file(src, dst)?)
    }
}

fn is_file_line_error(line: &str) -> bool {
    let mut parts = line.splitn(3, ':');
    matches!((parts.next(), parts.next(), parts.next()), (Some(file), Some(n), Some(_)) if file.contains('.') && n.parse::<u32>().is_ok())
}

fn first_error(log: &Path) -> String {
    let text = std::fs::read(log).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let Some(i) = lines.iter().position(|l| l.starts_with('!') || is_file_line_error(l)) else {
        return tr!("unknown (see doc.log)", "unbekannt (siehe doc.log)");
    };
    let mut out = lines[i].trim_start_matches('!').trim().to_string();
    if let Some(context) = lines[i + 1..].iter().take(8).find(|l| l.starts_with("l.")) {
        out.push_str(" — ");
        out.push_str(context.trim());
    }
    out
}

pub const INVOICE_PLACEHOLDERS: &[&str] = &[
    "language", "title", "label_from", "label_to", "label_details", "from", "to", "details", "columns", "rows", "sums", "tax_note",
    "label_total", "total", "currency", "note", "regards", "signer", "payment", "invoice_number", "issue_date", "due_date",
    "company_name", "company_email", "company_phone", "company_website", "iban", "bic", "bank_name",
];

pub const TIMESHEET_PLACEHOLDERS: &[&str] = &[
    "language", "title", "subtitle", "owner", "email", "label_by", "label_email", "period", "body", "label_summary", "label_week",
    "label_hours", "label_hours_total", "overview", "grand_total",
];

fn fill(template: &str, file: &str, vars: &[(&str, String)]) -> AppResult<String> {
    let mut out = String::with_capacity(template.len() * 2);
    for line in template.split_inclusive('\n') {
        let (code, comment) = line.split_at(comment_start(line));
        fill_code(code, file, vars, &mut out)?;
        out.push_str(comment);
    }
    Ok(out)
}

fn comment_start(line: &str) -> usize {
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match c {
            '%' if !escaped => return i,
            '\\' => escaped = !escaped,
            _ => escaped = false,
        }
    }
    line.len()
}

fn fill_code(mut rest: &str, file: &str, vars: &[(&str, String)], out: &mut String) -> AppResult<()> {
    while let Some(start) = rest.find("<<") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let len = after.find(|c: char| !(c.is_ascii_lowercase() || c == '_')).unwrap_or(after.len());
        if len > 0 && after[len..].starts_with(">>") {
            let name = &after[..len];
            let value = vars
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v)
                .ok_or_else(|| msg(tr!("Unknown placeholder <<{name}>> in {file}", "Unbekannter Platzhalter <<{name}>> in {file}")))?;
            out.push_str(value);
            rest = &after[len + 2..];
        } else {
            out.push_str("<<");
            rest = after;
        }
    }
    out.push_str(rest);
    Ok(())
}

pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '&' | '%' | '$' | '#' | '_' | '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            _ => out.push(c),
        }
    }
    out
}

fn amount(cents: i64) -> String {
    format!("{}{}.{:02}", if cents < 0 { "-" } else { "" }, cents.abs() / 100, cents.abs() % 100)
}

fn short_amount(cents: i64) -> String {
    if cents % 100 == 0 {
        (cents / 100).to_string()
    } else {
        amount(cents)
    }
}

fn qty(q: f64) -> String {
    let s = format!("{q:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn hm(secs: i64) -> String {
    format!("{}:{:02} h", secs / 3600, (secs % 3600) / 60)
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const WEEKDAYS_DE: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const MONTHS_DE: [&str; 12] = ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];

fn split_period(name: &str) -> (String, Option<String>) {
    let t = name.trim();
    if let (true, Some(open)) = (t.ends_with(')'), t.rfind('(')) {
        let inner = &t[open + 1..t.len() - 1];
        if inner.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return (t[..open].trim().to_string(), Some(inner.trim().to_string()));
        }
    }
    (t.to_string(), None)
}

fn break_after_last_comma(line: &str) -> Vec<String> {
    match line.rfind(',') {
        Some(i) if line.matches(',').count() >= 2 => vec![line[..=i].trim().to_string(), line[i + 1..].trim().to_string()],
        _ => vec![line.trim().to_string()],
    }
}

fn country_name(code: &str) -> String {
    match code.trim() {
        "DE" | "" => pick("Germany", "Deutschland").into(),
        "AT" => pick("Austria", "Österreich").into(),
        "CH" => pick("Switzerland", "Schweiz").into(),
        other => other.into(),
    }
}

fn minipage_lines(lines: &[String], gap_after: Option<usize>) -> String {
    let n = lines.len();
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let sep = if i + 1 == n {
                ""
            } else if Some(i) == gap_after {
                " \\\\[4mm]"
            } else {
                " \\\\"
            };
            format!("{l}{sep}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn invoice_tex(template: &str, inv: &Invoice, customer: Option<&Customer>, s: &Settings) -> AppResult<String> {
    let cur = esc(currency_symbol(&inv.currency));
    let signer = esc(if s.company_owner.trim().is_empty() { &s.company_name } else { &s.company_owner });

    let mut from = vec![signer.clone()];
    if !s.company_email.trim().is_empty() {
        from.push(esc(s.company_email.trim()));
    }
    let from_gap = from.len() - 1;
    from.extend(s.company_address.lines().filter(|l| !l.trim().is_empty()).flat_map(break_after_last_comma).map(|l| esc(&l)));

    let mut to = Vec::new();
    let to_gap;
    match customer {
        Some(c) => {
            to.push(esc(&c.name));
            if let Some(mail) = c.billing_email.as_deref().filter(|v| !v.trim().is_empty()).or(c.email.as_deref().filter(|v| !v.trim().is_empty())) {
                to.push(esc(mail));
            }
            to_gap = to.len() - 1;
            for l in [&c.address_line1, &c.address_line2].into_iter().flatten().filter(|v| !v.trim().is_empty()) {
                to.push(esc(l));
            }
            let zip = c.zip.as_deref().unwrap_or("").trim();
            let city = c.city.as_deref().unwrap_or("").trim();
            if !zip.is_empty() || !city.is_empty() {
                to.push(esc(&format!("{zip}, {city},")));
            }
            to.push(esc(&country_name(c.country.as_deref().unwrap_or(""))));
        }
        None => {
            to.extend(inv.customer_details.as_deref().unwrap_or("").lines().map(esc));
            to_gap = 0;
        }
    }

    let mut details = vec![tr!("Invoice no.: \\hfill {}", "Rechnungs-Nr.: \\hfill {}", esc(&inv.invoice_number))];
    if let Some(n) = inv.customer_number.as_deref() {
        details.push(tr!("Customer no.: \\hfill {}", "Kunden-Nr.: \\hfill {}", esc(n)));
    }
    let details_gap = details.len() - 1;
    if let Some(o) = inv.order_number.as_deref().filter(|o| !o.trim().is_empty()) {
        details.push(tr!("Order no.: \\hfill {}", "Auftrags-Nr.: \\hfill {}", esc(o.trim()).replace('-', "\\mbox{-}")));
    }
    details.push(tr!("Invoice date: \\hfill {}", "Rechnungsdatum: \\hfill {}", fmt_date(&inv.issue_date)));
    details.push(tr!("Due date: \\hfill {}", "Fälligkeitsdatum: \\hfill {}", fmt_date(&inv.due_date)));
    if !s.company_tax_number.trim().is_empty() {
        details.push(tr!("Tax number: \\hfill {}", "Steuernummer: \\hfill {}", esc(s.company_tax_number.trim())));
    }
    if !s.company_vat_id.trim().is_empty() {
        details.push(tr!("VAT ID: \\hfill {}", "USt-IdNr.: \\hfill {}", esc(s.company_vat_id.trim())));
    }

    let rate = inv.vat_rate;
    let rate_txt = qty(rate);
    let rows: String = inv
        .line_items
        .iter()
        .map(|li| {
            let (name, period) = split_period(&li.name);
            let total = li.total_cents();
            let vat = (total as f64 * rate / 100.0).round() as i64;
            let unit = li.unit.as_deref().filter(|u| !u.trim().is_empty()).map(|u| format!("~{}", esc(u))).unwrap_or_default();
            let period = period.map(|p| esc(&p).split('–').map(str::trim).collect::<Vec<_>>().join("~–~")).unwrap_or_default();
            format!(
                "{} &\n{}~{cur} &\n{}{unit} &\n{period} &\n{}~{cur}~({rate_txt}\\%) &\n{}~{cur} \\\\\n",
                esc(&name),
                short_amount(li.price_cents),
                if li.is_hours() { fmt_hours(li.quantity) } else { qty(li.quantity) },
                short_amount(vat),
                amount(total)
            )
        })
        .collect();
    let mut sums = String::new();
    if inv.discount_cents > 0 || inv.vat_cents > 0 {
        sums.push_str(&tr!("\\multicolumn{{5}}{{r}}{{Subtotal:}} & {}~{cur} \\\\\n", "\\multicolumn{{5}}{{r}}{{Zwischensumme:}} & {}~{cur} \\\\\n", amount(inv.subtotal_cents)));
    }
    if inv.discount_cents > 0 {
        sums.push_str(&tr!("\\multicolumn{{5}}{{r}}{{Discount:}} & -{}~{cur} \\\\\n", "\\multicolumn{{5}}{{r}}{{Rabatt:}} & -{}~{cur} \\\\\n", amount(inv.discount_cents)));
    }
    if inv.vat_cents > 0 {
        sums.push_str(&tr!("\\multicolumn{{5}}{{r}}{{VAT ({rate_txt}\\%):}} & {}~{cur} \\\\\n", "\\multicolumn{{5}}{{r}}{{MwSt. ({rate_txt}\\%):}} & {}~{cur} \\\\\n", amount(inv.vat_cents)));
    }
    let tax_note = if s.small_business && rate == 0.0 { pick("No VAT is shown in accordance with § 19 UStG.", "Gemäß § 19 UStG wird keine Umsatzsteuer ausgewiesen.") } else { "" };

    let note = inv
        .note
        .as_deref()
        .filter(|n| !n.trim().is_empty())
        .map(|n| format!("{}\n\n\\vspace{{4.5mm}}\n\n", n.lines().map(esc).collect::<Vec<_>>().join("\\\\\n")))
        .unwrap_or_default();

    let mut payment = Vec::new();
    for (label, value) in [
        (pick("Recipient", "Empfänger"), signer.clone()),
        ("IBAN", esc(s.company_iban.trim())),
        ("BIC/SWIFT", esc(s.company_bic.trim())),
        (pick("Bank name", "Name der Bank"), esc(s.company_bank_name.trim())),
    ] {
        if !value.is_empty() {
            payment.push(format!("{label} & {value} \\\\"));
        }
    }
    let payment_block = if s.company_iban.trim().is_empty() {
        String::new()
    } else {
        format!(
            r"{{
\begin{{tabularx}}{{\linewidth}}{{@{{}}LR@{{}}}}
\textbf{{{l_payment}}} &
{{
  \renewcommand{{\arraystretch}}{{1}}
  \begin{{tabular}}[t]{{@{{}}r@{{}}}}
    \textbf{{{l_local}}} \\[-1pt]
    {{\small\color{{textgray2}}{l_sepa}}}
  \end{{tabular}}
}} \\
\midrule
{}
\end{{tabularx}}%
}}%",
            payment.join("\n"),
            l_payment = pick("Payment details", "Zahlungsdetails"),
            l_local = pick("Local \\& international", "Lokal \\& International"),
            l_sepa = pick("For SEPA transfers", "Für SEPA-Transaktionen"),
        )
    };

    let vars = [
        ("language", pick("british", "ngerman").to_string()),
        ("title", pick("Invoice", "Rechnung").to_string()),
        ("label_from", pick("From", "Von").to_string()),
        ("label_to", pick("To", "An").to_string()),
        ("label_details", "Details".to_string()),
        ("from", minipage_lines(&from, Some(from_gap))),
        ("to", minipage_lines(&to, Some(to_gap))),
        ("details", minipage_lines(&details, Some(details_gap))),
        (
            "columns",
            pick(
                r"\textbf{Item} & \textbf{Price} & \textbf{Qty} & \textbf{Date} & \textbf{VAT} & \textbf{Amount}",
                r"\textbf{Position} & \textbf{Preis} & \textbf{Menge} & \textbf{Datum} & \textbf{MwSt.} & \textbf{Betrag}",
            )
            .to_string(),
        ),
        ("rows", rows),
        ("sums", sums),
        ("tax_note", tax_note.to_string()),
        ("label_total", pick("Total amount:", "Gesamtbetrag:").to_string()),
        ("total", amount(inv.amount_cents)),
        ("currency", cur),
        ("note", note),
        ("regards", pick("Kind regards", "Mit freundlichen Grüßen").to_string()),
        ("signer", signer),
        ("payment", payment_block),
        ("invoice_number", esc(&inv.invoice_number)),
        ("issue_date", fmt_date(&inv.issue_date)),
        ("due_date", fmt_date(&inv.due_date)),
        ("company_name", esc(s.company_name.trim())),
        ("company_email", esc(s.company_email.trim())),
        ("company_phone", esc(s.company_phone.trim())),
        ("company_website", esc(s.company_website.trim())),
        ("iban", esc(s.company_iban.trim())),
        ("bic", esc(s.company_bic.trim())),
        ("bank_name", esc(s.company_bank_name.trim())),
    ];
    fill(template, INVOICE, &vars)
}

pub struct TimesheetEntry {
    pub date: String,
    pub start: String,
    pub duration: i64,
    pub description: String,
    pub customer: String,
}

pub fn timesheet_tex(template: &str, entries: &[TimesheetEntry], label: &str, from: NaiveDate, s: &Settings) -> AppResult<String> {
    let owner = esc(if s.company_owner.trim().is_empty() { &s.company_name } else { &s.company_owner });

    let mut weeks: BTreeMap<NaiveDate, BTreeMap<String, Vec<&TimesheetEntry>>> = BTreeMap::new();
    for e in entries {
        let Some(d) = parse_date(&e.date) else { continue };
        let monday = d - chrono::Duration::days(d.weekday().num_days_from_monday() as i64);
        weeks.entry(monday).or_default().entry(e.customer.clone()).or_default().push(e);
    }

    let mut body = String::new();
    let mut overview = String::new();
    let mut grand = 0;
    let mut current_month: Option<(i32, u32)> = None;
    for (monday, by_customer) in &weeks {
        let sunday = *monday + chrono::Duration::days(6);
        let section = if *monday < from { (from.year(), from.month()) } else { (monday.year(), monday.month()) };
        if current_month != Some(section) {
            let months = if is_de() { MONTHS_DE } else { MONTHS };
            body.push_str(&format!("\\section{{{} {}}}\n\n", months[section.1 as usize - 1], section.0));
            current_month = Some(section);
        }
        let kw = monday.iso_week().week();
        body.push_str(&tr!("\\subsection{{Calendar week {kw:02}}}\n", "\\subsection{{Kalenderwoche {kw:02}}}\n"));

        let mut week_total = 0;
        for (customer, list) in by_customer {
            let mut list = list.clone();
            list.sort_by(|a, b| a.start.cmp(&b.start));
            let total: i64 = list.iter().map(|e| e.duration).sum();
            week_total += total;
            let head = if is_de() {
                format!("{} – {} | Klient: {}", monday.format("%d.%m."), sunday.format("%d.%m.%Y"), esc(customer))
            } else {
                format!("{} – {} | Client: {}", monday.format("%d/%m"), sunday.format("%d/%m/%Y"), esc(customer))
            };
            let rows: String = list
                .iter()
                .filter_map(|e| {
                    let d = parse_date(&e.date)?;
                    Some(format!(
                        "{}, {} & {} & {} \\\\\n\\hline\n",
                        (if is_de() { WEEKDAYS_DE } else { WEEKDAYS })[d.weekday().num_days_from_monday() as usize],
                        fmt_date(&e.date),
                        esc(&e.description),
                        hm(e.duration)
                    ))
                })
                .collect();
            body.push_str(&format!(
                r"\begin{{table}}[H]
\centering
\renewcommand{{\arraystretch}}{{1.2}}
\begin{{tabularx}}{{\textwidth}}{{|p{{2.8cm}}|X|p{{2.2cm}}|}}
\hline
\multicolumn{{3}}{{|p{{\dimexpr\textwidth-2\tabcolsep-2\arrayrulewidth\relax}}|}}{{\centering\textbf{{{head}}}}} \\
\hline
\textbf{{{l_date}}} & \textbf{{{l_activity}}} & \textbf{{{l_duration}}} \\
\hline
{rows}\multicolumn{{2}}{{|r|}}{{\textbf{{{l_hours_total}}}}} & \textbf{{{total}}} \\
\hline
\end{{tabularx}}
\end{{table}}

",
                total = hm(total),
                l_date = pick("Date", "Datum"),
                l_activity = pick("Activity", "Tätigkeitsbeschreibung"),
                l_duration = pick("Duration", "Dauer"),
                l_hours_total = pick("Total hours", "Gesamtstunden"),
            ));
        }
        grand += week_total;
        overview.push_str(&tr!("CW {kw:02} & {} \\\\\n\\hline\n", "KW {kw:02} & {} \\\\\n\\hline\n", hm(week_total)));
    }

    let vars = [
        ("language", pick("british", "ngerman").to_string()),
        ("title", pick("Time tracking", "Zeiterfassung").to_string()),
        ("subtitle", pick("Record of hours worked.", "Dokumentation der geleisteten Arbeitsstunden.").to_string()),
        ("owner", owner),
        ("email", esc(s.company_email.trim())),
        ("label_by", pick("by", "von").to_string()),
        ("label_email", pick("Email", "E-Mail").to_string()),
        ("period", esc(label)),
        ("body", body),
        ("label_summary", pick("Summary", "Gesamtübersicht").to_string()),
        ("label_week", pick("Calendar week", "Kalenderwoche").to_string()),
        ("label_hours", pick("Hours", "Stunden").to_string()),
        ("label_hours_total", pick("Total hours", "Gesamtstunden").to_string()),
        ("overview", overview),
        ("grand_total", hm(grand)),
    ];
    fill(template, TIMESHEET, &vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_latex_specials() {
        assert_eq!(esc("Design & Concept 100% #1 a_b"), "Design \\& Concept 100\\% \\#1 a\\_b");
    }

    #[test]
    fn formats_like_the_template() {
        assert_eq!(amount(102000), "1020.00");
        assert_eq!(short_amount(6000), "60");
        assert_eq!(short_amount(6050), "60.50");
        assert_eq!(qty(15.0), "15");
        assert_eq!(qty(16.5), "16.5");
        assert_eq!(hm(9000), "2:30 h");
        assert_eq!(split_period("Website – UI-Design (30.08.–09.09.26)"), ("Website – UI-Design".into(), Some("30.08.–09.09.26".into())));
        assert_eq!(split_period("Maintenance (monthly)"), ("Maintenance (monthly)".into(), None), "only dates count as period");
        assert_eq!(break_after_last_comma("12345, Example City, Germany"), ["12345, Example City,", "Germany"]);
    }

    #[test]
    fn timesheet_groups_weeks_by_client() {
        let s = Settings { company_owner: "Jane Doe".into(), ..Settings::default() };
        let e = |date: &str| TimesheetEntry { date: date.into(), start: format!("{date}T09:00:00Z"), duration: 7200, description: "Design & Code".into(), customer: "Acme GmbH".into() };
        let template = templates::default_content(TIMESHEET).unwrap();
        let tex = timesheet_tex(template, &[e("2026-04-14"), e("2026-07-29")], "Fiscal year 2026", NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), &s).unwrap();
        assert!(tex.contains("13/04 – 19/04/2026 | Client: Acme GmbH"));
        assert!(tex.contains("27/07 – 02/08/2026 | Client: Acme GmbH"));
        assert!(tex.contains("Tue, 14/04/2026 & Design \\& Code & 2:00 h"));
        assert!(tex.contains("\\textbf{4:00 h}"), "grand total in the overview");
        assert!(tex.lines().filter(|l| !l.starts_with('%')).all(|l| !l.contains("<<")), "every placeholder is filled");
    }

    #[test]
    fn fills_placeholders_and_rejects_unknown_ones() {
        let vars = [("name", "Jane".to_string())];
        assert_eq!(fill(r"Hi <<name>>, \draw[<<-] a << b", "t.tex", &vars).unwrap(), r"Hi Jane, \draw[<<-] a << b");
        assert_eq!(fill("<<name>> % see <<rows>>\n50\\% <<name>>", "t.tex", &vars).unwrap(), "Jane % see <<rows>>\n50\\% Jane", "comments stay untouched");
        let err = fill("<<nmae>>", "t.tex", &vars).unwrap_err().to_string();
        assert!(err.contains("<<nmae>>") && err.contains("t.tex"), "{err}");
    }

    #[test]
    fn default_invoice_template_uses_exactly_the_known_placeholders() {
        let inv = crate::invoices::Invoice { invoice_number: "INV-1".into(), issue_date: "2026-10-03".into(), due_date: "2026-10-17".into(), currency: "EUR".into(), ..Default::default() };
        let tex = invoice_tex(templates::default_content(INVOICE).unwrap(), &inv, None, &Settings::default()).unwrap();
        assert!(tex.contains("INV-1"));
        assert!(tex.lines().filter(|l| !l.starts_with('%')).all(|l| !l.contains("<<")), "every placeholder is filled");
    }
}
