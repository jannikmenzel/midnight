use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::config::DbConfig;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{msg, AppResult};
use crate::i18n::tr;
use crate::migrations::ALL as MIGRATIONS;

pub type Db = Arc<Mutex<Connection>>;

pub struct AppState {
    pub db: Db,
    pub files_dir: PathBuf,
}

impl AppState {
    pub fn new(conn: Connection, files_dir: PathBuf) -> AppResult<Self> {
        ensure_file_dirs(&files_dir)?;
        rebase_file_paths(&conn, &files_dir)?;
        Ok(Self { db: Arc::new(Mutex::new(conn)), files_dir })
    }

    pub fn conn(&self) -> AppResult<MutexGuard<'_, Connection>> {
        lock(&self.db)
    }
}

fn rebase_file_paths(conn: &Connection, files_dir: &Path) -> AppResult<()> {
    for (table, col) in [("documents", "path"), ("receipts", "path"), ("invoices", "file_path")] {
        let rows: Vec<(String, String)> = conn
            .prepare(&format!("SELECT id, {col} FROM {table} WHERE {col} IS NOT NULL"))?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        for (id, path) in rows {
            if Path::new(&path).exists() {
                continue;
            }
            let Some(i) = path.rfind("/files/") else { continue };
            let rebased = files_dir.join(&path[i + "/files/".len()..]);
            if rebased.exists() {
                conn.execute(
                    &format!("UPDATE {table} SET {col} = ?2 WHERE id = ?1"),
                    rusqlite::params![id, rebased.to_string_lossy()],
                )?;
            }
        }
    }
    Ok(())
}

pub fn ensure_file_dirs(files_dir: &Path) -> AppResult<()> {
    for sub in ["vault", "receipts", "invoices", "previews"] {
        std::fs::create_dir_all(files_dir.join(sub))?;
    }
    Ok(())
}

pub fn reset(conn: &Connection) -> AppResult<()> {
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_RESET_DATABASE, true)?;
    let vacuumed = conn.execute_batch("VACUUM");
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_RESET_DATABASE, false)?;
    vacuumed?;
    configure(conn)?;
    migrate(conn)
}

pub fn lock(db: &Db) -> AppResult<MutexGuard<'_, Connection>> {
    db.lock().map_err(|_| msg(tr!("Database is locked, please restart the app", "Datenbank ist blockiert, bitte die App neu starten")))
}

pub fn open(path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> AppResult<Connection> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> AppResult<()> {
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
    Ok(())
}

fn migrate(conn: &Connection) -> AppResult<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version as usize >= MIGRATIONS.len() {
        return Ok(());
    }
    conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
    let result = (|| -> AppResult<()> {
        for (i, migration) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(migration.sql)?;
            if let Some(data) = migration.data {
                data(&tx)?;
            }
            tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1))?;
            let broken: i64 = tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?;
            if broken > 0 {
                return Err(msg(tr!("Migration {} created {broken} invalid references", "Migration {} hat {broken} ungültige Verknüpfungen erzeugt", i + 1)));
            }
            tx.commit()?;
        }
        Ok(())
    })();
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    result
}

const DEFAULT_CATEGORIES: &[(&str, &str, &str, &str, &str)] = &[
    ("software", "Software & subscriptions", "Licences, SaaS, hosting, domains", "Software & Abos", "Lizenzen, SaaS, Hosting, Domains"),
    ("hardware", "Hardware & equipment", "Computers, monitors, accessories", "Hardware & Technik", "Computer, Monitore, Zubehör"),
    ("office", "Office & supplies", "Office supplies, furniture, postage", "Büro & Arbeitsmittel", "Büromaterial, Möbel, Porto"),
    ("telecom", "Phone & internet", "Mobile and internet (business share)", "Telefon & Internet", "Mobilfunk und Internet (beruflicher Anteil)"),
    ("training", "Training & literature", "Courses, conferences, books", "Fortbildung & Fachliteratur", "Kurse, Konferenzen, Bücher"),
    ("travel", "Travel", "Train, public transport, mileage, accommodation", "Reisen & Fahrten", "Bahn, ÖPNV, Fahrtkosten, Übernachtung"),
    ("meals", "Business meals", "Business meals with customers", "Bewirtung", "Geschäftsessen mit Kunden"),
    ("marketing", "Advertising & marketing", "Ads, print, website", "Werbung & Marketing", "Anzeigen, Druck, Website"),
    ("contractors", "Contractors", "Freelancers and service providers you hire", "Fremdleistungen", "Beauftragte Freelancer und Dienstleister"),
    ("insurance", "Insurance & memberships", "Business insurance, chambers, associations", "Versicherungen & Beiträge", "Berufliche Versicherungen, Kammern, Verbände"),
    ("banking-fees", "Bank & fees", "Account fees, payment services", "Bank & Gebühren", "Kontoführung, Zahlungsdienste"),
    ("workspace", "Workspace", "Home office room, coworking", "Raumkosten", "Arbeitszimmer, Coworking"),
    ("other", "Other", "", "Sonstiges", ""),
];

pub fn localize_default_categories(conn: &Connection) -> AppResult<()> {
    let de = crate::i18n::is_de();
    for &(slug, en_name, en_desc, de_name, de_desc) in DEFAULT_CATEGORIES {
        let (from_name, from_desc, to_name, to_desc) = if de { (en_name, en_desc, de_name, de_desc) } else { (de_name, de_desc, en_name, en_desc) };
        conn.execute("UPDATE categories SET name = ?1 WHERE slug = ?2 AND name = ?3", params![to_name, slug, from_name])?;
        conn.execute(
            "UPDATE categories SET description = ?1 WHERE slug = ?2 AND COALESCE(description, '') = ?3",
            params![(!to_desc.is_empty()).then_some(to_desc), slug, from_desc],
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Settings {
    pub onboarded: bool,
    pub company_name: String,
    pub company_owner: String,
    pub company_email: String,
    pub company_phone: String,
    pub company_address_line1: String,
    pub company_address_line2: String,
    pub company_zip: String,
    pub company_city: String,
    pub company_country: String,
    pub company_vat_id: String,
    pub company_tax_number: String,
    pub company_iban: String,
    pub company_bic: String,
    pub company_bank_name: String,
    pub company_website: String,
    pub base_currency: String,
    pub language: String,
    pub locale: String,
    pub date_format: String,
    pub week_starts_on_monday: bool,
    pub small_business: bool,
    pub invoice_prefix: String,
    pub invoice_number_digits: u32,
    pub default_vat_rate: f64,
    pub default_payment_terms: i64,
    pub invoice_note: String,
    pub e_invoice: bool,
    pub notify_overdue: bool,
    pub menu_bar_icon: bool,
    pub menu_bar_show_time: bool,
    pub menu_bar_keep_running: bool,
    pub check_updates: bool,
    pub weekly_hours_limit: f64,
    pub limits: Limits,
    pub tax_overrides: BTreeMap<i32, crate::tax::ParamOverrides>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Limits {
    pub small_business_cents: i64,
    pub small_business_upper_cents: i64,
    pub gwg_gross_cents: i64,
    pub meals_deductible_percent: i64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { small_business_cents: 2_500_000, small_business_upper_cents: 10_000_000, gwg_gross_cents: 95_200, meals_deductible_percent: 70 }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            onboarded: false,
            company_name: String::new(),
            company_owner: String::new(),
            company_email: String::new(),
            company_phone: String::new(),
            company_address_line1: String::new(),
            company_address_line2: String::new(),
            company_zip: String::new(),
            company_city: String::new(),
            company_country: crate::places::HOME_COUNTRY.into(),
            company_vat_id: String::new(),
            company_tax_number: String::new(),
            company_iban: String::new(),
            company_bic: String::new(),
            company_bank_name: String::new(),
            company_website: String::new(),
            base_currency: "EUR".into(),
            language: "en".into(),
            locale: "en-GB".into(),
            date_format: "dd/MM/yyyy".into(),
            week_starts_on_monday: true,
            small_business: false,
            invoice_prefix: "INV-{YYYY}-".into(),
            invoice_number_digits: 4,
            default_vat_rate: 19.0,
            default_payment_terms: 14,
            invoice_note: "Thank you for your business.".into(),
            e_invoice: true,
            notify_overdue: true,
            menu_bar_icon: true,
            menu_bar_show_time: true,
            menu_bar_keep_running: true,
            check_updates: true,
            weekly_hours_limit: 0.0,
            limits: Limits::default(),
            tax_overrides: BTreeMap::new(),
        }
    }
}

pub fn legacy_latex_dir(conn: &Connection) -> AppResult<Option<String>> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'app'", [], |r| r.get(0))
        .optional()?;
    let value: Option<serde_json::Value> = raw.and_then(|json| serde_json::from_str(&json).ok());
    Ok(value.and_then(|v| v["latex_dir"].as_str().map(str::trim).filter(|d| !d.is_empty()).map(String::from)))
}

pub fn load_settings(conn: &Connection) -> AppResult<Settings> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'app'", [], |r| r.get(0))
        .optional()?;
    Ok(match raw {
        Some(json) => serde_json::from_str(&json).unwrap_or_default(),
        None => Settings::default(),
    })
}

pub fn store_settings(conn: &Connection, settings: &Settings) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('app', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![serde_json::to_string(settings)?],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn migrates_with_default_categories() {
        let conn = open_in_memory().unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM categories"), 13);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM categories WHERE color IN ('#14B8A6', '#22C55E', '#84CC16')"), 0, "no green categories");
        let s = load_settings(&conn).unwrap();
        assert_eq!(s.base_currency, "EUR");
    }

    #[test]
    fn menu_bar_is_on_for_settings_saved_before_it_existed() {
        let conn = open_in_memory().unwrap();
        conn.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('app', '{\"company_name\":\"Alt GmbH\"}')", []).unwrap();
        let s = load_settings(&conn).unwrap();
        assert_eq!(s.company_name, "Alt GmbH");
        assert!(s.menu_bar_icon && s.menu_bar_show_time && s.menu_bar_keep_running);
        assert!(s.e_invoice, "e-invoicing is on for settings saved before it existed");
        assert!(s.check_updates, "update checks are on for settings saved before they existed");
    }

    #[test]
    fn reset_restores_fresh_state() {
        let conn = open_in_memory().unwrap();
        let mut s = load_settings(&conn).unwrap();
        s.onboarded = true;
        s.company_name = "Test GmbH".into();
        store_settings(&conn, &s).unwrap();
        conn.execute("DELETE FROM categories", []).unwrap();

        reset(&conn).unwrap();

        let s = load_settings(&conn).unwrap();
        assert!(!s.onboarded);
        assert!(s.company_name.is_empty());
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM categories"), 13);
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
    }
}
