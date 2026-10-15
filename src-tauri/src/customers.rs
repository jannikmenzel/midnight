use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::i18n::tr;
use crate::db::{AppState, Settings};
use crate::error::{msg, AppResult};
use crate::util::{clean, new_id};
use crate::activities;

#[derive(Debug, Serialize, Deserialize, Default, Clone, schemars::JsonSchema)]
#[serde(default)]
pub struct CustomerContact {
    pub id: Option<String>,
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, schemars::JsonSchema)]
#[serde(default)]
pub struct Customer {
    pub id: Option<String>,
    pub name: String,
    pub contact: Option<String>,
    pub email: Option<String>,
    pub billing_email: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub address_line1: Option<String>,
    pub address_line2: Option<String>,
    pub zip: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub vat_number: Option<String>,
    pub note: Option<String>,
    pub preferred_currency: Option<String>,
    pub default_payment_terms: Option<i64>,
    pub is_archived: bool,
    pub customer_number: Option<String>,
    pub leitweg_id: Option<String>,
    pub invoice_count: i64,
    pub paid_cents: i64,
    pub open_cents: i64,
    pub tracked_seconds: i64,
    pub contacts: Option<Vec<CustomerContact>>,
}

const SELECT: &str = "SELECT c.id, c.name, c.contact, c.email, c.billing_email, c.phone, c.website, c.address_line1,
    c.address_line2, c.zip, c.city, c.country, c.vat_number, c.note, c.preferred_currency, c.default_payment_terms,
    c.is_archived, c.customer_number, c.leitweg_id,
    (SELECT COUNT(*) FROM invoices i WHERE i.customer_id = c.id AND i.status != 'canceled'),
    (SELECT COALESCE(SUM(amount_cents), 0) FROM invoices i WHERE i.customer_id = c.id AND i.status = 'paid'),
    (SELECT COALESCE(SUM(amount_cents), 0) FROM invoices i WHERE i.customer_id = c.id AND i.status IN ('unpaid','overdue')),
    (SELECT COALESCE(SUM(e.duration), 0) FROM tracker_entries e JOIN tracker_projects p ON p.id = e.project_id WHERE p.customer_id = c.id)
  FROM customers c";

fn map_row(r: &rusqlite::Row) -> rusqlite::Result<Customer> {
    Ok(Customer {
        id: r.get(0)?,
        name: r.get(1)?,
        contact: r.get(2)?,
        email: r.get(3)?,
        billing_email: r.get(4)?,
        phone: r.get(5)?,
        website: r.get(6)?,
        address_line1: r.get(7)?,
        address_line2: r.get(8)?,
        zip: r.get(9)?,
        city: r.get(10)?,
        country: r.get(11)?,
        vat_number: r.get(12)?,
        note: r.get(13)?,
        preferred_currency: r.get(14)?,
        default_payment_terms: r.get(15)?,
        is_archived: r.get(16)?,
        customer_number: r.get(17)?,
        leitweg_id: r.get(18)?,
        invoice_count: r.get(19)?,
        paid_cents: r.get(20)?,
        open_cents: r.get(21)?,
        tracked_seconds: r.get(22)?,
        contacts: Some(Vec::new()),
    })
}

fn attach_contacts(conn: &Connection, customers: &mut [Customer]) -> AppResult<()> {
    let mut stmt = conn.prepare("SELECT customer_id, id, name, email, phone FROM customer_contacts ORDER BY position, rowid")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, CustomerContact { id: r.get(1)?, name: r.get(2)?, email: r.get(3)?, phone: r.get(4)? }))
    })?;
    let mut by_customer: HashMap<String, Vec<CustomerContact>> = HashMap::new();
    for row in rows {
        let (customer_id, contact) = row?;
        by_customer.entry(customer_id).or_default().push(contact);
    }
    for c in customers.iter_mut() {
        c.contacts = Some(c.id.as_ref().and_then(|id| by_customer.remove(id)).unwrap_or_default());
    }
    Ok(())
}

fn store_contacts(conn: &Connection, customer_id: &str, contacts: &[CustomerContact]) -> AppResult<()> {
    conn.execute("DELETE FROM customer_contacts WHERE customer_id = ?1", params![customer_id])?;
    let mut stmt = conn.prepare("INSERT INTO customer_contacts (id, customer_id, position, name, email, phone) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
    for (i, k) in contacts.iter().enumerate() {
        stmt.execute(params![clean(&k.id).unwrap_or_else(new_id), customer_id, i as i64, clean(&k.name), clean(&k.email), clean(&k.phone)])?;
    }
    Ok(())
}

pub fn get_customer(conn: &Connection, id: &str) -> AppResult<Customer> {
    let mut c = conn
        .query_row(&format!("{SELECT} WHERE c.id = ?1"), params![id], map_row)
        .optional()?
        .ok_or_else(|| msg(tr!("Customer not found", "Kunde nicht gefunden")))?;
    attach_contacts(conn, std::slice::from_mut(&mut c))?;
    Ok(c)
}

const NUMBER_PREFIX: &str = "CUS-";

pub fn next_customer_number(conn: &Connection) -> AppResult<String> {
    let mut stmt = conn.prepare("SELECT customer_number FROM customers WHERE customer_number LIKE 'CUS-%'")?;
    let max = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .filter_map(|n| n.strip_prefix(NUMBER_PREFIX).and_then(|d| d.parse::<u32>().ok()))
        .max()
        .unwrap_or(0);
    Ok(format!("{NUMBER_PREFIX}{:03}", max + 1))
}

pub fn country_of(c: &Customer, s: &Settings) -> String {
    clean(&c.country).unwrap_or_else(|| s.company_country.clone())
}

pub fn address_block(c: &Customer, s: &Settings) -> String {
    let mut lines = vec![c.name.clone()];
    for l in [&c.contact, &c.address_line1, &c.address_line2] {
        if let Some(v) = l.as_ref().filter(|v| !v.trim().is_empty()) {
            lines.push(v.clone());
        }
    }
    let city = [c.zip.clone().unwrap_or_default(), c.city.clone().unwrap_or_default()].join(" ");
    if !city.trim().is_empty() {
        lines.push(city.trim().to_string());
    }
    let country = country_of(c, s);
    if country != s.company_country {
        lines.push(crate::places::country_name(&country));
    }
    if let Some(vat) = c.vat_number.as_ref().filter(|v| !v.trim().is_empty()) {
        lines.push(format!("USt-IdNr.: {vat}"));
    }
    lines.join("\n")
}

pub async fn list_customers(state: &AppState, search: Option<String>, include_archived: Option<bool>) -> AppResult<Vec<Customer>> {
    let conn = state.conn()?;
    let pattern = format!("%{}%", search.unwrap_or_default().trim());
    let archived = include_archived.unwrap_or(false);
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE (c.name LIKE ?1 OR COALESCE(c.email, '') LIKE ?1 OR COALESCE(c.contact, '') LIKE ?1
             OR EXISTS (SELECT 1 FROM customer_contacts k WHERE k.customer_id = c.id
                        AND (COALESCE(k.name, '') LIKE ?1 OR COALESCE(k.email, '') LIKE ?1 OR COALESCE(k.phone, '') LIKE ?1)))
           AND (?2 OR c.is_archived = 0)
         ORDER BY c.is_archived, c.customer_number IS NULL, CAST(SUBSTR(c.customer_number, 5) AS INTEGER), c.name COLLATE NOCASE"
    ))?;
    let mut customers = stmt.query_map(params![pattern, archived], map_row)?.collect::<Result<Vec<_>, _>>()?;
    attach_contacts(&conn, &mut customers)?;
    Ok(customers)
}

pub async fn save_customer(state: &AppState, customer: Customer) -> AppResult<Customer> {
    let mut c = customer;
    if c.name.trim().is_empty() {
        return Err(msg(tr!("The customer name must not be empty", "Der Kundenname darf nicht leer sein")));
    }
    let contacts = c.contacts.take().map(|list| {
        list.into_iter()
            .filter(|k| clean(&k.name).is_some() || clean(&k.email).is_some() || clean(&k.phone).is_some())
            .collect::<Vec<_>>()
    });
    if let Some(list) = &contacts {
        let primary = list.first();
        c.contact = primary.and_then(|k| clean(&k.name));
        c.email = primary.and_then(|k| clean(&k.email));
        c.phone = primary.and_then(|k| clean(&k.phone));
    }
    c.country = crate::places::parse_country(c.country.as_deref())?;
    let conn = state.conn()?;
    let id = c.id.clone().unwrap_or_else(new_id);
    let is_new = c.id.is_none();
    let number = if is_new { Some(next_customer_number(&conn)?) } else { None };
    conn.execute(
        "INSERT INTO customers (id, name, contact, email, billing_email, phone, website, address_line1, address_line2,
            zip, city, country, vat_number, note, preferred_currency, default_payment_terms, is_archived,
            customer_number, leitweg_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, contact = excluded.contact, email = excluded.email,
            billing_email = excluded.billing_email, phone = excluded.phone, website = excluded.website,
            address_line1 = excluded.address_line1, address_line2 = excluded.address_line2, zip = excluded.zip,
            city = excluded.city, country = excluded.country, vat_number = excluded.vat_number, note = excluded.note,
            preferred_currency = excluded.preferred_currency, default_payment_terms = excluded.default_payment_terms,
            is_archived = excluded.is_archived, leitweg_id = excluded.leitweg_id",
        params![
            id, c.name.trim(), c.contact, c.email, c.billing_email, c.phone, c.website, c.address_line1, c.address_line2,
            c.zip, c.city, c.country, c.vat_number, c.note, c.preferred_currency, c.default_payment_terms, c.is_archived,
            number, clean(&c.leitweg_id)
        ],
    )?;
    match contacts {
        Some(list) => store_contacts(&conn, &id, &list)?,
        None => {
            let existing: i64 = conn.query_row("SELECT COUNT(*) FROM customer_contacts WHERE customer_id = ?1", params![id], |r| r.get(0))?;
            let legacy = CustomerContact { id: None, name: c.contact.clone(), email: c.email.clone(), phone: c.phone.clone() };
            if existing == 0 && (clean(&legacy.name).is_some() || clean(&legacy.email).is_some() || clean(&legacy.phone).is_some()) {
                store_contacts(&conn, &id, &[legacy])?;
            }
        }
    }
    if is_new {
        activities::log(&conn, "customer_created", &tr!("Customer \"{}\" created", "Kunde „{}“ angelegt", c.name.trim()), None)?;
    }
    get_customer(&conn, &id)
}

pub async fn delete_customer(state: &AppState, id: String) -> AppResult<()> {
    state.conn()?.execute("DELETE FROM customers WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(name: &str, email: &str, phone: &str) -> CustomerContact {
        CustomerContact { id: None, name: Some(name.into()), email: Some(email.into()), phone: Some(phone.into()) }
    }

    #[test]
    fn contacts_keep_their_order_and_are_removed_with_the_customer() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute("INSERT INTO customers (id, name) VALUES ('c', 'Acme GmbH')", []).unwrap();
        store_contacts(&conn, "c", &[contact("Anna", "anna@acme.de", "030 1"), contact("Ben", "", " 030 2 ")]).unwrap();

        let c = get_customer(&conn, "c").unwrap();
        let contacts = c.contacts.unwrap();
        assert_eq!(contacts.iter().map(|k| k.name.as_deref().unwrap()).collect::<Vec<_>>(), ["Anna", "Ben"]);
        assert_eq!(contacts[1].email, None);
        assert_eq!(contacts[1].phone.as_deref(), Some("030 2"));

        conn.execute("DELETE FROM customers WHERE id = 'c'", []).unwrap();
        let left: i64 = conn.query_row("SELECT COUNT(*) FROM customer_contacts", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 0);
    }
}
