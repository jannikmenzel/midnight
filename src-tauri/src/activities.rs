use rusqlite::{params, Connection};
use serde::Serialize;

use crate::db::AppState;
use crate::error::AppResult;
use crate::util::new_id;

#[derive(Debug, Serialize)]
pub struct Activity {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
    pub metadata: Option<String>,
    pub read: bool,
    pub created_at: String,
}

pub fn log(conn: &Connection, kind: &str, message: &str, metadata: Option<serde_json::Value>) -> AppResult<()> {
    conn.execute(
        "INSERT INTO activities (id, type, message, metadata) VALUES (?1, ?2, ?3, ?4)",
        params![new_id(), kind, message, metadata.map(|m| m.to_string())],
    )?;
    Ok(())
}

pub async fn list_activities(state: &AppState) -> AppResult<Vec<Activity>> {
    let conn = state.conn()?;
    let mut stmt = conn.prepare(
        "SELECT id, type, message, metadata, read, created_at FROM activities ORDER BY created_at DESC LIMIT 100",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Activity {
            id: r.get(0)?,
            kind: r.get(1)?,
            message: r.get(2)?,
            metadata: r.get(3)?,
            read: r.get(4)?,
            created_at: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub async fn mark_activities_read(state: &AppState) -> AppResult<()> {
    state.conn()?.execute("UPDATE activities SET read = 1 WHERE read = 0", [])?;
    Ok(())
}

pub async fn clear_activities(state: &AppState) -> AppResult<()> {
    state.conn()?.execute("DELETE FROM activities", [])?;
    Ok(())
}
