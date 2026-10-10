use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::i18n::tr;
use crate::db::AppState;
use crate::error::{msg, AppResult, Context};
use crate::extract::extract;
use crate::files::{extract_text, remove_file_quietly, store_file};
use crate::util::{content_type_for, fts_query, new_id, safe_file_name};
use crate::activities;

#[derive(Debug, Serialize)]
pub struct Document {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
    pub path: String,
    pub content_type: Option<String>,
    pub size: i64,
    pub date: Option<String>,
    pub created_at: String,
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
    pub doc_type: Option<String>,
    pub tags: Vec<String>,
    pub snippet: Option<String>,
    pub has_text: bool,
    pub invoice_id: Option<String>,
    pub invoice_number: Option<String>,
    pub expense_id: Option<String>,
    pub expense_name: Option<String>,
}

const COLUMNS: &str = "d.id, d.name, d.title, d.path, d.content_type, d.size, d.date, d.created_at,
    (SELECT group_concat(t.name, char(31)) FROM document_tags dt JOIN tags t ON t.id = dt.tag_id WHERE dt.document_id = d.id),
    d.content IS NOT NULL, d.customer_id, (SELECT c.name FROM customers c WHERE c.id = d.customer_id), d.doc_type,
    d.invoice_id, (SELECT i.invoice_number FROM invoices i WHERE i.id = d.invoice_id),
    (SELECT r.expense_id FROM receipts r WHERE r.id = d.receipt_id),
    (SELECT e.name FROM receipts r JOIN expenses e ON e.id = r.expense_id WHERE r.id = d.receipt_id)";

fn map_row(r: &rusqlite::Row, snippet: Option<String>) -> rusqlite::Result<Document> {
    let tags: Option<String> = r.get(8)?;
    Ok(Document {
        id: r.get(0)?,
        name: r.get(1)?,
        title: r.get(2)?,
        path: r.get(3)?,
        content_type: r.get(4)?,
        size: r.get(5)?,
        date: r.get(6)?,
        created_at: r.get(7)?,
        customer_id: r.get(10)?,
        customer_name: r.get(11)?,
        doc_type: r.get(12)?,
        tags: tags.map(|s| s.split('\u{1f}').map(String::from).collect()).unwrap_or_default(),
        snippet,
        has_text: r.get(9)?,
        invoice_id: r.get(13)?,
        invoice_number: r.get(14)?,
        expense_id: r.get(15)?,
        expense_name: r.get(16)?,
    })
}

pub async fn vault_upload(state: &AppState, paths: Vec<String>) -> AppResult<usize> {
    let stored = paths
        .iter()
        .map(|p| store_file(&state.files_dir, "vault", p))
        .collect::<AppResult<Vec<_>>>()?;
    let conn = state.conn()?;
    for f in &stored {
        let content = extract_text(&f.path);
        let date = content.as_deref().and_then(|t| extract(t).date);
        conn.execute(
            "INSERT INTO documents (id, name, path, content_type, size, content, date) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![new_id(), f.name, f.path, f.content_type, f.size, content, date],
        )?;
    }
    activities::log(&conn, "document_uploaded", &tr!("{} document(s) added to the vault", "{} Dokument(e) im Vault abgelegt", stored.len()), None)?;
    Ok(stored.len())
}

pub fn search_documents(conn: &Connection, search: Option<&str>, tag: Option<&str>, limit: i64) -> AppResult<Vec<Document>> {
    let tag = tag.filter(|t| !t.is_empty());
    let tag_clause = "(?2 IS NULL OR d.id IN (SELECT dt.document_id FROM document_tags dt JOIN tags t ON t.id = dt.tag_id WHERE t.name = ?2))";
    match search.and_then(fts_query) {
        Some(q) => {
            let mut stmt = conn.prepare(&format!(
                "SELECT {COLUMNS}, snippet(documents_fts, 2, '«', '»', '…', 12)
                 FROM documents_fts f JOIN documents d ON d.rowid = f.rowid
                 WHERE documents_fts MATCH ?1 AND {tag_clause}
                 ORDER BY rank LIMIT ?3"
            ))?;
            let rows = stmt.query_map(params![q, tag, limit], |r| {
                let snippet: Option<String> = r.get(17)?;
                map_row(r, snippet)
            })?;
            Ok(rows.collect::<Result<_, _>>()?)
        }
        None => {
            let mut stmt = conn.prepare(&format!(
                "SELECT {COLUMNS} FROM documents d WHERE (?1 IS NULL OR 1) AND {tag_clause}
                 ORDER BY d.created_at DESC LIMIT ?3"
            ))?;
            let rows = stmt.query_map(params![Option::<String>::None, tag, limit], |r| map_row(r, None))?;
            Ok(rows.collect::<Result<_, _>>()?)
        }
    }
}

pub async fn list_documents(state: &AppState, search: Option<String>, tag: Option<String>) -> AppResult<Vec<Document>> {
    let conn = state.conn()?;
    search_documents(&conn, search.as_deref(), tag.as_deref(), 500)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DocumentUpdate {
    pub id: String,
    pub title: Option<String>,
    pub date: Option<String>,
    pub customer_id: Option<String>,
    pub doc_type: Option<String>,
    pub tags: Vec<String>,
}

pub async fn update_document(state: &AppState, update: DocumentUpdate) -> AppResult<()> {
    let conn = state.conn()?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE documents SET title = ?2, date = ?3, customer_id = ?4, doc_type = ?5 WHERE id = ?1",
        params![
            update.id,
            update.title.filter(|t| !t.trim().is_empty()),
            update.date.filter(|d| !d.is_empty()),
            update.customer_id.filter(|c| !c.is_empty()),
            update.doc_type.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()),
        ],
    )?;
    tx.execute("DELETE FROM document_tags WHERE document_id = ?1", params![update.id])?;
    for tag in update.tags.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        let tag_id = upsert_tag(&tx, tag)?;
        tx.execute(
            "INSERT OR IGNORE INTO document_tags (document_id, tag_id) VALUES (?1, ?2)",
            params![update.id, tag_id],
        )?;
    }
    prune_tags(&tx)?;
    tx.commit()?;
    Ok(())
}

fn with_extension(name: &str, old_name: &str) -> String {
    let ext = std::path::Path::new(old_name).extension().map(|e| e.to_string_lossy().to_string());
    match ext {
        Some(ext) if std::path::Path::new(name).extension().is_none() => format!("{name}.{ext}"),
        _ => name.to_string(),
    }
}

fn free_path(dir: &std::path::Path, name: &str, current: &std::path::Path) -> std::path::PathBuf {
    let p = std::path::Path::new(name);
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| name.to_string());
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let current = std::fs::canonicalize(current).ok();
    let is_free = |c: &std::path::Path| !c.exists() || std::fs::canonicalize(c).ok() == current;
    let first = dir.join(name);
    if is_free(&first) {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|c| is_free(c))
        .expect("unbounded range")
}

fn ensure_unlinked(conn: &Connection, id: &str) -> AppResult<()> {
    let linked: bool = conn.query_row(
        "SELECT invoice_id IS NOT NULL OR receipt_id IS NOT NULL FROM documents WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    if linked {
        return Err(msg(tr!(
            "This document is synced with an invoice or expense. Change or delete it there.",
            "Dieses Dokument ist mit einer Rechnung oder Ausgabe synchronisiert. Ändere oder lösche es dort."
        )));
    }
    Ok(())
}

pub fn is_issued(status: &str) -> bool {
    !matches!(status, "draft" | "scheduled")
}

struct Linked {
    name: String,
    path: String,
    title: String,
    doc_type: String,
    date: Option<String>,
    customer_id: Option<String>,
}

fn upsert_linked(conn: &Connection, column: &str, source_id: &str, l: Linked) -> AppResult<()> {
    let size = std::fs::metadata(&l.path).map(|m| m.len() as i64).unwrap_or(0);
    let content = extract_text(&l.path);
    let content_type = content_type_for(&l.name);
    let updated = conn.execute(
        &format!(
            "UPDATE documents SET name = ?2, path = ?3, content_type = ?4, size = ?5, content = ?6, date = ?7,
                customer_id = COALESCE(?8, customer_id)
             WHERE {column} = ?1"
        ),
        params![source_id, l.name, l.path, content_type, size, content, l.date, l.customer_id],
    )?;
    if updated == 0 {
        conn.execute(
            &format!(
                "INSERT INTO documents (id, name, title, path, content_type, size, content, date, customer_id, doc_type, {column})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
            ),
            params![new_id(), l.name, l.title, l.path, content_type, size, content, l.date, l.customer_id, l.doc_type, source_id],
        )?;
    }
    Ok(())
}

pub fn has_invoice(conn: &Connection, invoice_id: &str) -> AppResult<bool> {
    Ok(conn.query_row("SELECT EXISTS (SELECT 1 FROM documents WHERE invoice_id = ?1)", params![invoice_id], |r| r.get(0))?)
}

type InvoiceRow = (String, String, Option<String>, String, Option<String>);

pub fn sync_invoice(conn: &Connection, invoice_id: &str) -> AppResult<()> {
    let row: Option<InvoiceRow> = conn
        .query_row(
            "SELECT invoice_number, status, file_path, issue_date, customer_id FROM invoices WHERE id = ?1",
            params![invoice_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((number, status, file_path, issue_date, customer_id)) = row else { return Ok(()) };
    match file_path.filter(|p| is_issued(&status) && std::path::Path::new(p).exists()) {
        Some(path) => {
            let name = std::path::Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("{number}.pdf"));
            upsert_linked(
                conn,
                "invoice_id",
                invoice_id,
                Linked {
                    name,
                    path,
                    title: tr!("Invoice {number}", "Rechnung {number}"),
                    doc_type: tr!("Invoice", "Rechnung"),
                    date: Some(issue_date),
                    customer_id,
                },
            )
        }
        None => {
            conn.execute("DELETE FROM documents WHERE invoice_id = ?1", params![invoice_id])?;
            prune_tags(conn)
        }
    }
}

pub fn sync_receipt(conn: &Connection, receipt_id: &str) -> AppResult<()> {
    let (name, path, title, date): (String, String, String, String) = conn.query_row(
        "SELECT r.name, r.path, e.name, e.date FROM receipts r JOIN expenses e ON e.id = r.expense_id WHERE r.id = ?1",
        params![receipt_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    upsert_linked(
        conn,
        "receipt_id",
        receipt_id,
        Linked { name, path, title, doc_type: tr!("Receipt", "Beleg"), date: Some(date), customer_id: None },
    )
}

fn ids(conn: &Connection, sql: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn backfill(conn: &Connection, files_dir: &std::path::Path) -> AppResult<()> {
    for id in ids(conn, "SELECT r.id FROM receipts r WHERE NOT EXISTS (SELECT 1 FROM documents d WHERE d.receipt_id = r.id)")? {
        sync_receipt(conn, &id)?;
    }
    for id in ids(
        conn,
        "SELECT i.id FROM invoices i WHERE i.status NOT IN ('draft', 'scheduled')
           AND NOT EXISTS (SELECT 1 FROM documents d WHERE d.invoice_id = i.id)",
    )? {
        crate::invoices::sync_vault(conn, files_dir, &id, false);
    }
    Ok(())
}

pub async fn rename_document(state: &AppState, id: String, name: String) -> AppResult<String> {
    let conn = state.conn()?;
    ensure_unlinked(&conn, &id)?;
    let (old_name, old_path): (String, String) =
        conn.query_row("SELECT name, path FROM documents WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(msg(tr!("File name must not be empty", "Dateiname darf nicht leer sein")));
    }
    let new_name = with_extension(&safe_file_name(trimmed), &old_name);
    if new_name == old_name {
        return Ok(old_name);
    }
    let old = std::path::PathBuf::from(&old_path);
    let dir = old.parent().ok_or_else(|| msg(tr!("Invalid file path", "Ungültiger Dateipfad")))?;
    let new = free_path(dir, &new_name, &old);
    std::fs::rename(&old, &new).ctx(&tr!("Renaming file", "Datei umbenennen"))?;
    let final_name = new.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(new_name);
    let new_path = new.to_string_lossy().to_string();
    if let Err(e) = conn.execute(
        "UPDATE documents SET name = ?2, path = ?3, content_type = ?4 WHERE id = ?1",
        params![id, final_name, new_path, content_type_for(&final_name)],
    ) {
        let _ = std::fs::rename(&new, &old);
        return Err(e.into());
    }
    Ok(final_name)
}

pub async fn delete_document(state: &AppState, id: String) -> AppResult<()> {
    let conn = state.conn()?;
    ensure_unlinked(&conn, &id)?;
    let path: String = conn.query_row("SELECT path FROM documents WHERE id = ?1", params![id], |r| r.get(0))?;
    conn.execute("DELETE FROM documents WHERE id = ?1", params![id])?;
    prune_tags(&conn)?;
    remove_file_quietly(&path);
    Ok(())
}

pub fn upsert_tag(conn: &Connection, name: &str) -> AppResult<String> {
    conn.execute("INSERT OR IGNORE INTO tags (id, name) VALUES (?1, ?2)", params![new_id(), name.trim()])?;
    Ok(conn.query_row("SELECT id FROM tags WHERE name = ?1", params![name.trim()], |r| r.get(0))?)
}

pub fn prune_tags(conn: &Connection) -> AppResult<()> {
    conn.execute("DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM document_tags)", [])?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct TagInfo {
    pub id: String,
    pub name: String,
    pub usage: i64,
}

pub async fn list_tags(state: &AppState) -> AppResult<Vec<TagInfo>> {
    let conn = state.conn()?;
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, (SELECT COUNT(*) FROM document_tags WHERE tag_id = t.id) FROM tags t ORDER BY t.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| Ok(TagInfo { id: r.get(0)?, name: r.get(1)?, usage: r.get(2)? }))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_keeps_extension_and_avoids_collisions() {
        assert_eq!(with_extension("Mietvertrag", "a.pdf"), "Mietvertrag.pdf");
        assert_eq!(with_extension("Mietvertrag.pdf", "a.pdf"), "Mietvertrag.pdf");
        let dir = std::env::temp_dir().join(format!("midnight-rename-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let current = dir.join("x.pdf");
        std::fs::write(&current, b"x").unwrap();
        std::fs::write(dir.join("Vertrag.pdf"), b"y").unwrap();
        assert_eq!(free_path(&dir, "Vertrag.pdf", &current), dir.join("Vertrag (2).pdf"));
        assert_eq!(free_path(&dir, "x.pdf", &current), dir.join("x.pdf"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn full_text_search_finds_content() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO documents (id, name, path, content) VALUES ('d1', 'vertrag.pdf', '/x', 'Mietvertrag für Büroräume in München')",
            [],
        )
        .unwrap();
        let hits = search_documents(&conn, Some("buero"), None, 10).unwrap();
        assert!(hits.is_empty(), "no transliteration: ü is not ue");
        let hits = search_documents(&conn, Some("Büro"), None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        let hits = search_documents(&conn, Some("munchen"), None, 10).unwrap();
        assert_eq!(hits.len(), 1, "diacritics are folded by the tokenizer");
    }

    #[test]
    fn invoices_and_receipts_follow_their_source() {
        let conn = crate::db::open_in_memory().unwrap();
        let dir = std::env::temp_dir().join(format!("midnight-vault-sync-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("RE-1.pdf");
        let receipt = dir.join("beleg.txt");
        std::fs::write(&pdf, b"x").unwrap();
        std::fs::write(&receipt, b"Figma Rechnung").unwrap();
        conn.execute(
            "INSERT INTO invoices (id, invoice_number, status, issue_date, due_date, file_path) VALUES ('i1', 'RE-1', 'unpaid', '2026-10-01', '2026-10-15', ?1)",
            params![pdf.to_string_lossy()],
        )
        .unwrap();
        conn.execute("INSERT INTO expenses (id, date, name, amount_cents) VALUES ('e1', '2026-10-02', 'Figma', 1440)", []).unwrap();
        conn.execute(
            "INSERT INTO receipts (id, expense_id, name, path) VALUES ('r1', 'e1', 'beleg.txt', ?1)",
            params![receipt.to_string_lossy()],
        )
        .unwrap();
        backfill(&conn, &dir).unwrap();
        let docs = search_documents(&conn, None, None, 10).unwrap();
        assert_eq!(docs.len(), 2);
        let inv = docs.iter().find(|d| d.invoice_id.is_some()).unwrap();
        assert_eq!(inv.invoice_number.as_deref(), Some("RE-1"));
        assert_eq!(inv.date.as_deref(), Some("2026-10-01"));
        let rec = docs.iter().find(|d| d.expense_id.is_some()).unwrap();
        assert_eq!(rec.expense_name.as_deref(), Some("Figma"));
        assert_eq!(search_documents(&conn, Some("Figma"), None, 10).unwrap().len(), 1, "receipt text is indexed");
        assert!(ensure_unlinked(&conn, &rec.id).is_err());

        conn.execute("UPDATE invoices SET status = 'draft' WHERE id = 'i1'", []).unwrap();
        sync_invoice(&conn, "i1").unwrap();
        assert!(!has_invoice(&conn, "i1").unwrap(), "drafts are not in the vault");
        conn.execute("UPDATE invoices SET status = 'paid' WHERE id = 'i1'", []).unwrap();
        sync_invoice(&conn, "i1").unwrap();
        assert!(has_invoice(&conn, "i1").unwrap());

        conn.execute("DELETE FROM invoices WHERE id = 'i1'", []).unwrap();
        conn.execute("DELETE FROM expenses WHERE id = 'e1'", []).unwrap();
        assert!(search_documents(&conn, None, None, 10).unwrap().is_empty(), "deleting the source removes the vault entry");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
