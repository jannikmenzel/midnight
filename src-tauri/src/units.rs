use serde::Serialize;

use crate::error::{msg, AppResult};
use crate::i18n::{pick, tr};

pub const HOURS: &str = "HUR";
pub const PIECES: &str = "H87";
const UNITS: &[(&str, &str, &str)] = &[
    (HOURS, "hrs", "Std."),
    ("MIN", "min", "Min."),
    ("DAY", "days", "Tage"),
    ("WEE", "weeks", "Wochen"),
    ("MON", "months", "Monate"),
    ("ANN", "years", "Jahre"),
    (PIECES, "pcs", "Stk."),
    ("LS", "flat rate", "pauschal"),
    ("KMT", "km", "km"),
    ("C62", "units", "Einh."),
];

#[derive(Debug, Serialize)]
pub struct Unit {
    pub code: &'static str,
    pub label: &'static str,
}

pub fn is_known(code: &str) -> bool {
    UNITS.iter().any(|u| u.0 == code)
}

pub fn label(code: &str) -> &str {
    UNITS.iter().find(|u| u.0 == code).map_or(code, |u| pick(u.1, u.2))
}

pub fn parse(raw: Option<&str>) -> AppResult<Option<String>> {
    let Some(code) = raw.map(|u| u.trim().to_ascii_uppercase()).filter(|u| !u.is_empty()) else { return Ok(None) };
    if is_known(&code) {
        Ok(Some(code))
    } else {
        let allowed = UNITS.iter().map(|u| u.0).collect::<Vec<_>>().join(", ");
        Err(msg(tr!("Unknown unit {code}, allowed are the UN/ECE codes {allowed}", "Unbekannte Einheit {code}, erlaubt sind die UN/ECE-Codes {allowed}")))
    }
}

pub async fn list_units() -> AppResult<Vec<Unit>> {
    Ok(UNITS.iter().map(|&(code, en, de)| Unit { code, label: pick(en, de) }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_un_ece_codes() {
        assert_eq!(parse(Some(" hur ")).unwrap().as_deref(), Some(HOURS));
        assert_eq!(parse(Some("")).unwrap(), None);
        assert!(parse(Some("Std.")).is_err());
    }
}
