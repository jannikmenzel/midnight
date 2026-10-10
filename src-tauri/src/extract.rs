use std::sync::OnceLock;

use regex::Regex;

use crate::util::{iso, parse_amount, parse_any_date};

#[derive(Debug, Default, PartialEq)]
pub struct Extracted {
    pub amount_cents: Option<i64>,
    pub currency: Option<String>,
    pub date: Option<String>,
    pub invoice_number: Option<String>,
    pub vendor: Option<String>,
}

fn amount_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"-?(?:\d{1,3}(?:[.\u{a0} ]\d{3})+,\d{2}|\d{1,3}(?:,\d{3})+\.\d{2}|\d+,\d{2}|\d+\.\d{2})").unwrap()
    })
}

fn date_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2}\.\d{1,2}\.\d{2,4}|\d{4}-\d{2}-\d{2})\b").unwrap())
}

fn number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:rechnungs?\s*-?\s*(?:nr|nummer|no)\.?|invoice\s*(?:no\.?|number|#)|beleg\s*-?\s*nr\.?|rg\.?\s*-?\s*nr\.?)\s*[:#]?\s*([A-Z0-9][A-Z0-9\-/_.]{2,30})",
        )
        .unwrap()
    })
}

const TOTAL_KEYWORDS: &[&str] = &[
    "gesamtbetrag", "rechnungsbetrag", "zu zahlen", "zahlbetrag", "endbetrag", "bruttobetrag", "gesamtsumme",
    "summe brutto", "total due", "amount due", "grand total", "total (eur)", "gesamt", "total", "summe", "betrag",
];
const NOT_TOTAL: &[&str] = &["netto", "zwischensumme", "subtotal", "mwst", "ust.", "umsatzsteuer", "vat", "steuer"];

fn amounts_in(line: &str) -> Vec<i64> {
    amount_re()
        .find_iter(line)
        .filter_map(|m| parse_amount(m.as_str()))
        .collect()
}

fn find_total(lines: &[&str]) -> Option<i64> {
    for keyword in TOTAL_KEYWORDS {
        for (i, line) in lines.iter().enumerate() {
            let lower = line.to_lowercase();
            if !lower.contains(keyword) {
                continue;
            }
            let generic = matches!(*keyword, "gesamt" | "total" | "summe" | "betrag");
            if generic && NOT_TOTAL.iter().any(|n| lower.contains(n)) {
                continue;
            }
            let mut found = amounts_in(line);
            if found.is_empty() {
                if let Some(next) = lines.get(i + 1) {
                    found = amounts_in(next);
                }
            }
            if let Some(last) = found.last() {
                return Some(last.abs());
            }
        }
    }
    lines.iter().flat_map(|l| amounts_in(l)).map(i64::abs).max()
}

fn find_date(lines: &[&str]) -> Option<String> {
    let valid = |s: &str| parse_any_date(s).filter(|d| (2000..=2100).contains(&chrono::Datelike::year(d)));
    for line in lines {
        let lower = line.to_lowercase();
        if ["rechnungsdatum", "belegdatum", "invoice date", "datum", "date"].iter().any(|k| lower.contains(k)) {
            if let Some(d) = date_re().find_iter(line).find_map(|m| valid(m.as_str())) {
                return Some(iso(d));
            }
        }
    }
    lines.iter().flat_map(|l| date_re().find_iter(l)).find_map(|m| valid(m.as_str())).map(iso)
}

fn find_currency(text: &str) -> Option<String> {
    let counts = [
        ("EUR", text.matches('€').count() + text.matches("EUR").count()),
        ("USD", text.matches('$').count() + text.matches("USD").count()),
        ("GBP", text.matches('£').count() + text.matches("GBP").count()),
        ("CHF", text.matches("CHF").count()),
    ];
    counts.iter().filter(|(_, n)| *n > 0).max_by_key(|(_, n)| *n).map(|(c, _)| c.to_string())
}

fn find_vendor(lines: &[&str]) -> Option<String> {
    const SKIP: &[&str] = &["rechnung", "invoice", "seite", "page", "datum", "date", "quittung", "receipt", "kunde"];
    lines
        .iter()
        .map(|l| l.trim())
        .find(|l| {
            let lower = l.to_lowercase();
            l.chars().filter(|c| c.is_alphabetic()).count() >= 3
                && l.len() <= 60
                && !SKIP.iter().any(|s| lower.starts_with(s))
                && !l.contains('@')
        })
        .map(String::from)
}

pub fn extract(text: &str) -> Extracted {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    Extracted {
        amount_cents: find_total(&lines),
        currency: find_currency(text),
        date: find_date(&lines),
        invoice_number: number_re().captures(text).and_then(|c| c.get(1)).map(|m| m.as_str().trim_end_matches('.').to_string()),
        vendor: find_vendor(&lines),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn german_invoice() {
        let text = "Telekom Deutschland GmbH\nRechnung\nRechnungsnummer: 4711-2026/09\nRechnungsdatum 01.09.2026\n\
                    Mobilfunk 33,60 €\nNettobetrag 33,60 €\nUmsatzsteuer 19 % 6,39 €\nRechnungsbetrag 39,99 €";
        let e = extract(text);
        assert_eq!(e.amount_cents, Some(3999));
        assert_eq!(e.date.as_deref(), Some("2026-09-01"));
        assert_eq!(e.currency.as_deref(), Some("EUR"));
        assert_eq!(e.invoice_number.as_deref(), Some("4711-2026/09"));
        assert_eq!(e.vendor.as_deref(), Some("Telekom Deutschland GmbH"));
    }

    #[test]
    fn english_receipt_with_thousands() {
        let text = "Acme Cloud Inc.\nInvoice number INV-0042\nDate 2026-08-15\nSubtotal $1,000.00\nTax $0.00\nTotal due $1,000.00";
        let e = extract(text);
        assert_eq!(e.amount_cents, Some(100000));
        assert_eq!(e.currency.as_deref(), Some("USD"));
        assert_eq!(e.invoice_number.as_deref(), Some("INV-0042"));
    }
}
