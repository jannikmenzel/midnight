use chrono::{Datelike, Duration, Local, Months, NaiveDate, Utc};

pub fn trimmed(s: &str) -> Option<String> {
    Some(s.trim()).filter(|s| !s.is_empty()).map(String::from)
}

pub fn clean(v: &Option<String>) -> Option<String> {
    v.as_deref().and_then(trimmed)
}

pub fn amount(cents: i64) -> String {
    format!("{}{}.{:02}", if cents < 0 { "-" } else { "" }, cents.abs() / 100, cents.abs() % 100)
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn now_ts() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

pub fn today_date() -> NaiveDate {
    Local::now().date_naive()
}

pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10).unwrap_or(s), "%Y-%m-%d").ok()
}

pub fn parse_any_date(raw: &str) -> Option<NaiveDate> {
    let s = raw.trim();
    for fmt in ["%d.%m.%Y", "%Y-%m-%d", "%d.%m.%y", "%d/%m/%Y", "%Y/%m/%d"] {
        if let Ok(d) = NaiveDate::parse_from_str(s, fmt) {
            return Some(d);
        }
    }
    None
}

pub fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn fmt_date_de(iso_date: &str) -> String {
    parse_date(iso_date)
        .map(|d| d.format("%d.%m.%Y").to_string())
        .unwrap_or_else(|| iso_date.to_string())
}

pub fn currency_symbol(currency: &str) -> &str {
    match currency {
        "EUR" => "€",
        "USD" => "$",
        "GBP" => "£",
        other => other,
    }
}

pub fn fmt_money(cents: i64, currency: &str) -> String {
    let neg = cents < 0;
    let abs = cents.unsigned_abs();
    let whole = (abs / 100).to_string();
    let de = crate::i18n::is_de();
    let mut grouped = String::new();
    for (i, ch) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i).is_multiple_of(3) {
            grouped.push(if de { '.' } else { ',' });
        }
        grouped.push(ch);
    }
    let sign = if neg { "-" } else { "" };
    if de {
        format!("{sign}{grouped},{:02} {}", abs % 100, currency_symbol(currency))
    } else {
        format!("{sign}{}{grouped}.{:02}", currency_symbol(currency), abs % 100)
    }
}

pub fn parse_amount(raw: &str) -> Option<i64> {
    let trimmed = raw.trim();
    let neg = trimmed.starts_with('-') || trimmed.ends_with('-');
    let s: String = trimmed
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
        .collect();
    if !s.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let normalized = match (s.rfind(','), s.rfind('.')) {
        (Some(c), Some(d)) if c > d => s.replace('.', "").replace(',', "."),
        (Some(_), Some(_)) => s.replace(',', ""),
        (Some(c), None) => {
            if s.matches(',').count() > 1 || s.len() - c - 1 == 3 {
                s.replace(',', "")
            } else {
                s.replace(',', ".")
            }
        }
        (None, Some(d)) => {
            if s.matches('.').count() > 1 || s.len() - d - 1 == 3 {
                s.replace('.', "")
            } else {
                s
            }
        }
        (None, None) => s,
    };
    let value: f64 = normalized.parse().ok()?;
    let cents = (value * 100.0).round() as i64;
    Some(if neg { -cents } else { cents })
}

pub fn last_day_of_month(d: NaiveDate) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).expect("valid month");
    first + Months::new(1) - Duration::days(1)
}

pub fn add_months_anchored(d: NaiveDate, months: u32, anchor_day: u32) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).expect("valid month") + Months::new(months);
    let last = last_day_of_month(first).day();
    first.with_day(anchor_day.clamp(1, last)).expect("clamped day")
}

pub fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') { c } else { '_' })
        .collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() { "datei".into() } else { cleaned }
}

pub fn content_type_for(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".pdf") {
        "application/pdf"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".txt") || lower.ends_with(".md") {
        "text/plain"
    } else if lower.ends_with(".csv") {
        "text/csv"
    } else if lower.ends_with(".xml") {
        "application/xml"
    } else {
        "application/octet-stream"
    }
}

pub fn fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split_whitespace()
        .map(|t| t.chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{t}\"*"))
        .collect();
    if terms.is_empty() { None } else { Some(terms.join(" ")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_german_and_english_amounts() {
        assert_eq!(parse_amount("1.234,56"), Some(123456));
        assert_eq!(parse_amount("-49,99"), Some(-4999));
        assert_eq!(parse_amount("1,234.56"), Some(123456));
        assert_eq!(parse_amount("12.5"), Some(1250));
        assert_eq!(parse_amount("1.234"), Some(123400));
        assert_eq!(parse_amount("119,00 €"), Some(11900));
        assert_eq!(parse_amount("abc"), None);
    }

    #[test]
    fn formats_money_in_english_by_default() {
        assert_eq!(fmt_money(123456, "EUR"), "€1,234.56");
        assert_eq!(fmt_money(-5, "EUR"), "-€0.05");
        assert_eq!(fmt_money(100000000, "USD"), "$1,000,000.00");
    }

    #[test]
    fn anchored_months_clamp_and_recover() {
        let jan31 = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        let feb = add_months_anchored(jan31, 1, 31);
        assert_eq!(feb, NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
        assert_eq!(add_months_anchored(feb, 1, 31), NaiveDate::from_ymd_opt(2026, 3, 31).unwrap());
    }
}
