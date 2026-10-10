use rusqlite::{params, Connection};
use serde::Serialize;

use crate::db::AppState;
use crate::error::AppResult;
use crate::util::fts_query;
use crate::vault::search_documents;

const QUERIES: [(&str, &str); 4] = [
    (
        "expense",
        "SELECT e.id, e.name, COALESCE(c.name, e.description), -e.amount_cents, e.currency, e.date
         FROM expenses e LEFT JOIN categories c ON c.slug = e.category_slug
         WHERE e.name LIKE ?1 OR COALESCE(e.description, '') LIKE ?1 OR COALESCE(e.note, '') LIKE ?1
         ORDER BY e.date DESC LIMIT 6",
    ),
    (
        "invoice",
        "SELECT id, invoice_number, customer_name, amount_cents, currency, issue_date FROM invoices
         WHERE invoice_number LIKE ?1 OR COALESCE(customer_name, '') LIKE ?1 ORDER BY issue_date DESC LIMIT 6",
    ),
    (
        "customer",
        "SELECT id, name, COALESCE(email, city), NULL, NULL, NULL FROM customers c
         WHERE name LIKE ?1 OR COALESCE(email, '') LIKE ?1 OR COALESCE(contact, '') LIKE ?1
            OR EXISTS (SELECT 1 FROM customer_contacts k WHERE k.customer_id = c.id
                       AND (COALESCE(k.name, '') LIKE ?1 OR COALESCE(k.email, '') LIKE ?1 OR COALESCE(k.phone, '') LIKE ?1))
         LIMIT 6",
    ),
    (
        "project",
        "SELECT p.id, p.name, c.name, NULL, NULL, NULL FROM tracker_projects p LEFT JOIN customers c ON c.id = p.customer_id
         WHERE p.name LIKE ?1 LIMIT 4",
    ),
];

#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub amount_cents: Option<i64>,
    pub currency: Option<String>,
    pub date: Option<String>,
}

pub async fn global_search(state: &AppState, query: String) -> AppResult<Vec<SearchHit>> {
    let q = query.trim();
    if q.len() < 2 || fts_query(q).is_none() {
        return Ok(Vec::new());
    }
    let conn = state.conn()?;
    let like = format!("%{q}%");
    let mut hits = Vec::new();

    for (kind, sql) in QUERIES {
        hits.extend(query_hits(&conn, kind, sql, &like)?);
    }

    hits.extend(search_documents(&conn, Some(q), None, 6)?.into_iter().map(|d| SearchHit {
        kind: "document".into(),
        id: d.id,
        title: d.title.unwrap_or(d.name),
        subtitle: d.snippet,
        amount_cents: None,
        currency: None,
        date: d.date,
    }));

    Ok(hits)
}

fn query_hits(conn: &Connection, kind: &str, sql: &str, like: &str) -> AppResult<Vec<SearchHit>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![like], |r| {
        Ok(SearchHit {
            kind: kind.into(),
            id: r.get(0)?,
            title: r.get(1)?,
            subtitle: r.get(2)?,
            amount_cents: r.get(3)?,
            currency: r.get(4)?,
            date: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn hits(conn: &Connection, kind: &str, like: &str) -> Vec<SearchHit> {
        let (_, sql) = QUERIES.iter().find(|(k, _)| *k == kind).unwrap();
        query_hits(conn, kind, sql, like).unwrap()
    }

    #[test]
    fn expense_hits_are_negative_with_category_subtitle() {
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO expenses (id, date, name, amount_cents, currency, category_slug) VALUES ('e1', '2026-01-02', 'Telekom', 4999, 'EUR', 'other')",
            [],
        )
        .unwrap();
        let found = hits(&conn, "expense", "%telek%");
        assert_eq!(found.len(), 1);
        let h = &found[0];
        assert_eq!((h.kind.as_str(), h.id.as_str(), h.title.as_str()), ("expense", "e1", "Telekom"));
        assert_eq!(h.subtitle.as_deref(), Some("Other"));
        assert_eq!(h.amount_cents, Some(-4999));
        assert_eq!(h.currency.as_deref(), Some("EUR"));
        assert_eq!(h.date.as_deref(), Some("2026-01-02"));
    }

    #[test]
    fn customer_hits_have_no_amount_currency_or_date() {
        let conn = open_in_memory().unwrap();
        conn.execute("INSERT INTO customers (id, name, city) VALUES ('c1', 'Acme GmbH', 'Berlin')", []).unwrap();
        let found = hits(&conn, "customer", "%acme%");
        assert_eq!(found.len(), 1);
        let h = &found[0];
        assert_eq!((h.kind.as_str(), h.title.as_str()), ("customer", "Acme GmbH"));
        assert_eq!(h.subtitle.as_deref(), Some("Berlin"));
        assert_eq!((h.amount_cents, h.currency.as_deref(), h.date.as_deref()), (None, None, None));
    }
}
