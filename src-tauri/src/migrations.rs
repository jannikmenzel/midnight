use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::db::{load_settings, store_settings};
use crate::error::AppResult;
use crate::places::country_from_text;

pub struct Migration {
    pub sql: &'static str,
    pub data: Option<fn(&Connection) -> AppResult<()>>,
}

pub const ALL: &[Migration] = &[
    Migration { sql: include_str!("../migrations/0.1.0.sql"), data: None },
    Migration { sql: include_str!("../migrations/0.1.1.sql"), data: Some(v0_1_1) },
];

fn v0_1_1(conn: &Connection) -> AppResult<()> {
    customer_countries(conn)?;
    units(conn)?;
    company_address(conn)
}

fn customer_countries(conn: &Connection) -> AppResult<()> {
    let rows: Vec<(String, Option<String>)> =
        conn.prepare("SELECT id, country FROM customers")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    for (id, country) in rows {
        let code = country.as_deref().and_then(country_from_text);
        if code != country {
            conn.execute("UPDATE customers SET country = ?2 WHERE id = ?1", params![id, code])?;
        }
    }
    Ok(())
}

fn unit(raw: Option<&str>) -> Option<String> {
    let raw = raw.map(str::trim).filter(|u| !u.is_empty())?;
    let code = match raw.trim_end_matches('.').to_lowercase().as_str() {
        "h" | "std" | "stunde" | "stunden" | "hr" | "hrs" | "hour" | "hours" => "HUR",
        "min" | "minute" | "minuten" | "minutes" => "MIN",
        "d" | "tag" | "tage" | "day" | "days" | "pt" => "DAY",
        "wo" | "woche" | "wochen" | "week" | "weeks" => "WEE",
        "monat" | "monate" | "month" | "months" => "MON",
        "jahr" | "jahre" | "year" | "years" => "ANN",
        "st" | "stk" | "stück" | "pc" | "pcs" | "piece" | "pieces" => "H87",
        "pauschal" | "pauschale" | "psch" | "pausch" | "flat rate" | "lump sum" => "LS",
        "km" => "KMT",
        _ => return crate::units::parse(Some(raw)).ok().flatten(),
    };
    Some(code.to_string())
}

fn units(conn: &Connection) -> AppResult<()> {
    let products: Vec<(String, Option<String>)> =
        conn.prepare("SELECT id, unit FROM invoice_products")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    for (id, raw) in products {
        conn.execute("UPDATE invoice_products SET unit = ?2 WHERE id = ?1", params![id, unit(raw.as_deref())])?;
    }
    let invoices: Vec<(String, String)> =
        conn.prepare("SELECT id, line_items FROM invoices")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    for (id, json) in invoices {
        let Ok(mut lines) = serde_json::from_str::<Vec<Value>>(&json) else { continue };
        for line in lines.iter_mut().filter_map(Value::as_object_mut) {
            let code = unit(line.get("unit").and_then(Value::as_str));
            line.insert("unit".into(), code.map_or(Value::Null, Value::String));
        }
        conn.execute("UPDATE invoices SET line_items = ?2 WHERE id = ?1", params![id, serde_json::to_string(&lines)?])?;
    }
    Ok(())
}

fn split_place(line: &str) -> Option<(&str, &str, Option<&str>)> {
    let (zip, rest) = line.split_once([' ', ','])?;
    if zip.len() < 4 || !zip.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut parts = rest.split(',').map(str::trim).filter(|p| !p.is_empty());
    Some((zip, parts.next()?, parts.next()))
}

fn company_address(conn: &Connection) -> AppResult<()> {
    let raw: Option<String> = conn.query_row("SELECT value FROM settings WHERE key = 'app'", [], |r| r.get(0)).optional()?;
    let Some(address) = raw.and_then(|json| serde_json::from_str::<Value>(&json).ok()).and_then(|v| v["company_address"].as_str().map(String::from)) else {
        return Ok(());
    };
    let lines: Vec<&str> = address.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let mut settings = load_settings(conn)?;
    let place = lines.iter().enumerate().skip(1).find_map(|(i, line)| split_place(line).map(|p| (i, p)));
    match place.filter(|(i, _)| *i <= 2 && lines.len() <= i + 2) {
        Some((i, (zip, city, country))) => {
            settings.company_address_line1 = lines[0].to_string();
            settings.company_address_line2 = lines[1..i].join(", ");
            settings.company_zip = zip.to_string();
            settings.company_city = city.to_string();
            if let Some(code) = country.or(lines.get(i + 1).copied()).and_then(country_from_text) {
                settings.company_country = code;
            }
        }
        None => settings.company_address_line1 = lines.join(", "),
    }
    store_settings(conn, &settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Settings;

    fn upgrade(setup: impl FnOnce(&Connection)) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(ALL[0].sql).unwrap();
        setup(&conn);
        conn.execute_batch(ALL[1].sql).unwrap();
        v0_1_1(&conn).unwrap();
        conn
    }

    fn column(conn: &Connection, sql: &str) -> Vec<Option<String>> {
        conn.prepare(sql).unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
    }

    fn company(address: &str) -> (String, String, String, String, String) {
        let conn = upgrade(|c| {
            c.execute("INSERT INTO settings (key, value) VALUES ('app', ?1)", [serde_json::json!({ "company_address": address }).to_string()]).unwrap();
        });
        let Settings { company_address_line1, company_address_line2, company_zip, company_city, company_country, .. } = load_settings(&conn).unwrap();
        (company_address_line1, company_address_line2, company_zip, company_city, company_country)
    }

    fn expect(line1: &str, line2: &str, zip: &str, city: &str, country: &str) -> (String, String, String, String, String) {
        (line1.into(), line2.into(), zip.into(), city.into(), country.into())
    }

    fn has(conn: &Connection, table: &str, column: &str) -> bool {
        conn.query_row("SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2", [table, column], |r| r.get::<_, i64>(0)).unwrap() > 0
    }

    fn version(conn: &Connection) -> usize {
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap() as usize
    }

    #[test]
    fn updates_a_v0_1_0_database_when_the_app_opens_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("midnight.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(ALL[0].sql).unwrap();
            conn.execute_batch("PRAGMA user_version = 1").unwrap();
            let settings = serde_json::json!({ "company_name": "Alt GmbH", "company_address": "Musterstraße 1\n10115 Berlin" });
            conn.execute("INSERT INTO settings (key, value) VALUES ('app', ?1)", [settings.to_string()]).unwrap();
            conn.execute_batch(
                "INSERT INTO customers (id, name, country) VALUES ('c', 'Acme GmbH', 'Deutschland');
                 INSERT INTO invoice_products (id, name, unit) VALUES ('p', 'Beratung', 'Std.');",
            )
            .unwrap();
        }

        let conn = crate::db::open(&path).unwrap();
        assert_eq!(version(&conn), ALL.len());
        assert!(has(&conn, "customers", "leitweg_id") && has(&conn, "home_office_exclusions", "date"));
        let s = load_settings(&conn).unwrap();
        assert_eq!((s.company_name.as_str(), s.company_zip.as_str(), s.company_city.as_str()), ("Alt GmbH", "10115", "Berlin"));
        assert_eq!(column(&conn, "SELECT country FROM customers"), [Some("DE".into())]);
        assert_eq!(column(&conn, "SELECT unit FROM invoice_products"), [Some("HUR".into())]);
        crate::tax::compute(&conn, 2026).unwrap();
        drop(conn);
        assert_eq!(version(&crate::db::open(&path).unwrap()), ALL.len(), "opening again changes nothing");
    }

    #[test]
    fn a_fresh_install_gets_the_latest_schema() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("midnight.db")).unwrap();
        assert_eq!(version(&conn), ALL.len());
        assert!(has(&conn, "customers", "leitweg_id") && has(&conn, "home_office_exclusions", "date"));
        assert_eq!(load_settings(&conn).unwrap().company_country, "DE");
    }

    #[test]
    fn converts_known_units_and_clears_unknown_ones() {
        let conn = upgrade(|c| {
            c.execute_batch(
                r#"INSERT INTO invoice_products (id, name, unit) VALUES ('p', 'P', 'Std.'), ('q', 'Q', 'Seiten'), ('r', 'R', ''), ('s', 'S', 'STÜCK');
                   INSERT INTO invoices (id, invoice_number, issue_date, due_date, line_items) VALUES ('i', 'INV-1', '2026-01-01', '2026-01-15',
                     '[{"name":"a","quantity":1,"price_cents":1,"unit":"Stk."},{"name":"b","quantity":2,"price_cents":1,"unit":null},{"name":"c","quantity":3,"price_cents":1,"unit":"Seiten"}]');"#,
            )
            .unwrap();
        });
        assert_eq!(column(&conn, "SELECT unit FROM invoice_products ORDER BY id"), [Some("HUR".into()), None, None, Some("H87".into())]);
        let lines = crate::invoices::load_invoice(&conn, "i").unwrap().line_items;
        assert_eq!(lines.iter().map(|l| l.unit.as_deref()).collect::<Vec<_>>(), [Some("H87"), None, None]);
    }

    #[test]
    fn converts_country_names_and_clears_unknown_ones() {
        let conn = upgrade(|c| {
            c.execute_batch("INSERT INTO customers (id, name, country) VALUES ('a', 'A', 'Frankreich'), ('b', 'B', NULL), ('c', 'C', 'Atlantis'), ('d', 'D', 'nl');").unwrap();
        });
        assert_eq!(column(&conn, "SELECT country FROM customers ORDER BY id"), [Some("FR".into()), None, None, Some("NL".into())]);
    }

    #[test]
    fn splits_legacy_company_addresses() {
        assert_eq!(company("Musterstraße 1\n10115 Berlin"), expect("Musterstraße 1", "", "10115", "Berlin", "DE"));
        assert_eq!(company("Musterstr. 1\n12345, Example City, Germany"), expect("Musterstr. 1", "", "12345", "Example City", "DE"));
        assert_eq!(company("Weg 2\n\n1010 Wien\nÖsterreich"), expect("Weg 2", "", "1010", "Wien", "AT"));
        assert_eq!(company("c/o Coworking\nMusterstr. 1\n10115 Berlin"), expect("c/o Coworking", "Musterstr. 1", "10115", "Berlin", "DE"));
        assert_eq!(company("Postfach 12\nBerlin\nGermany"), expect("Postfach 12, Berlin, Germany", "", "", "", "DE"));
    }
}
