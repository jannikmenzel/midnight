use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::i18n::tr;
use crate::db::AppState;
use crate::error::{msg, AppResult};
use crate::util::new_id;

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Product {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub price_cents: i64,
    pub currency: String,
    pub unit: Option<String>,
    pub tax_rate: Option<f64>,
    #[serde(default)]
    pub usage_count: i64,
}

pub async fn list_products(state: &AppState, search: Option<String>) -> AppResult<Vec<Product>> {
    let conn = state.conn()?;
    let pattern = format!("%{}%", search.unwrap_or_default().trim());
    let mut stmt = conn.prepare(
        "SELECT id, name, description, price_cents, currency, unit, tax_rate, usage_count FROM invoice_products
         WHERE name LIKE ?1 OR COALESCE(description, '') LIKE ?1 ORDER BY usage_count DESC, name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map(params![pattern], |r| {
        Ok(Product {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            price_cents: r.get(3)?,
            currency: r.get(4)?,
            unit: r.get(5)?,
            tax_rate: r.get(6)?,
            usage_count: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub async fn save_product(state: &AppState, product: Product) -> AppResult<String> {
    if product.name.trim().is_empty() {
        return Err(msg(tr!("Name is missing", "Name fehlt")));
    }
    let id = product.id.clone().unwrap_or_else(new_id);
    state.conn()?.execute(
        "INSERT INTO invoice_products (id, name, description, price_cents, currency, unit, tax_rate)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, description = excluded.description,
            price_cents = excluded.price_cents, currency = excluded.currency, unit = excluded.unit, tax_rate = excluded.tax_rate",
        params![id, product.name.trim(), product.description, product.price_cents, product.currency, product.unit, product.tax_rate],
    )?;
    Ok(id)
}

pub async fn touch_product(state: &AppState, id: String) -> AppResult<()> {
    state.conn()?.execute("UPDATE invoice_products SET usage_count = usage_count + 1 WHERE id = ?1", params![id])?;
    Ok(())
}

pub async fn delete_product(state: &AppState, id: String) -> AppResult<()> {
    state.conn()?.execute("DELETE FROM invoice_products WHERE id = ?1", params![id])?;
    Ok(())
}
