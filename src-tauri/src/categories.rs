use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::i18n::tr;
use crate::db::AppState;
use crate::error::{msg, AppResult};

#[derive(Debug, Serialize)]
pub struct Category {
    pub slug: String,
    pub name: String,
    pub color: String,
    pub description: Option<String>,
    pub expense_count: i64,
}

pub async fn list_categories(state: &AppState) -> AppResult<Vec<Category>> {
    let conn = state.conn()?;
    let mut stmt = conn.prepare(
        "SELECT c.slug, c.name, c.color, c.description, (SELECT COUNT(*) FROM expenses e WHERE e.category_slug = c.slug)
         FROM categories c ORDER BY c.sort, c.name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Category { slug: r.get(0)?, name: r.get(1)?, color: r.get(2)?, description: r.get(3)?, expense_count: r.get(4)? })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        match c {
            'ä' => out.push_str("ae"),
            'ö' => out.push_str("oe"),
            'ü' => out.push_str("ue"),
            'ß' => out.push_str("ss"),
            c if c.is_ascii_alphanumeric() => out.push(c),
            _ => {
                if !out.ends_with('-') {
                    out.push('-')
                }
            }
        }
    }
    out.trim_matches('-').to_string()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CategoryInput {
    pub slug: Option<String>,
    pub name: String,
    pub color: String,
    pub description: Option<String>,
}

pub async fn save_category(state: &AppState, input: CategoryInput) -> AppResult<String> {
    let conn = state.conn()?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(msg(tr!("Name is missing", "Name fehlt")));
    }
    let description = input.description.filter(|d| !d.trim().is_empty());
    if let Some(slug) = input.slug {
        conn.execute(
            "UPDATE categories SET name = ?2, color = ?3, description = ?4 WHERE slug = ?1",
            params![slug, name, input.color, description],
        )?;
        return Ok(slug);
    }
    let base = Some(slugify(name)).filter(|s| !s.is_empty()).unwrap_or_else(|| "kategorie".into());
    let mut slug = base.clone();
    let mut n = 2;
    while conn.query_row("SELECT 1 FROM categories WHERE slug = ?1", params![slug], |_| Ok(())).optional()?.is_some() {
        slug = format!("{base}-{n}");
        n += 1;
    }
    conn.execute(
        "INSERT INTO categories (slug, name, color, sort, description)
         VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(sort), 0) + 1 FROM categories), ?4)",
        params![slug, name, input.color, description],
    )?;
    Ok(slug)
}

pub async fn delete_category(state: &AppState, slug: String) -> AppResult<()> {
    let conn = state.conn()?;
    conn.execute("DELETE FROM categories WHERE slug = ?1", params![slug])?;
    Ok(())
}
