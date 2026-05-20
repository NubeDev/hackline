//! `logs` table repository. Same ring-buffer shape as `events` but
//! with a `level` column extracted from the envelope's headers.

use rusqlite::{params, Connection, Transaction};
use serde::Serialize;

use crate::error::GatewayError;

pub const LOGS_MAX_PER_DEVICE: i64 = 10_000;

#[derive(Debug, Clone, Serialize)]
pub struct LogRow {
    pub id: i64,
    pub device_id: i64,
    pub topic: String,
    pub ts: i64,
    pub level: String,
    pub content_type: String,
    pub payload: serde_json::Value,
}

pub fn insert(
    conn: &mut Connection,
    device_id: i64,
    topic: &str,
    ts: i64,
    level: &str,
    content_type: &str,
    payload: &serde_json::Value,
) -> Result<i64, GatewayError> {
    let bytes = serde_json::to_vec(payload)
        .map_err(|e| GatewayError::BadRequest(format!("log payload not serialisable: {e}")))?;
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO logs (device_id, topic, ts, level, content_type, payload)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![device_id, topic, ts, level, content_type, bytes],
    )?;
    let id = tx.last_insert_rowid();
    prune(&tx, device_id, LOGS_MAX_PER_DEVICE)?;
    tx.commit()?;
    Ok(id)
}

fn prune(tx: &Transaction<'_>, device_id: i64, cap: i64) -> Result<(), GatewayError> {
    tx.execute(
        "DELETE FROM logs
         WHERE device_id = ?1
           AND id IN (
             SELECT id FROM logs
             WHERE device_id = ?1
             ORDER BY id DESC
             LIMIT -1 OFFSET ?2
           )",
        params![device_id, cap],
    )?;
    Ok(())
}

pub fn list(
    conn: &Connection,
    org_id: i64,
    device_id: Option<i64>,
    topic_glob: Option<&str>,
    level: Option<&str>,
    since_ms: Option<i64>,
    cursor: Option<i64>,
    limit: i64,
) -> Result<Vec<LogRow>, GatewayError> {
    let limit = limit.clamp(1, 1000);
    // Cross-org isolation (SCOPE.md §13 Phase 4) — see `events::list`
    // for the equivalent join shape.
    let mut sql = String::from(
        "SELECT l.id, l.device_id, l.topic, l.ts, l.level, l.content_type, l.payload
         FROM logs l
         JOIN devices d ON d.id = l.device_id
         WHERE d.org_id = ?",
    );
    let mut args: Vec<rusqlite::types::Value> = Vec::new();
    args.push(org_id.into());
    if let Some(d) = device_id {
        sql.push_str(" AND l.device_id = ?");
        args.push(d.into());
    }
    if let Some(t) = topic_glob {
        sql.push_str(" AND l.topic GLOB ?");
        args.push(t.to_string().into());
    }
    if let Some(lv) = level {
        sql.push_str(" AND l.level = ?");
        args.push(lv.to_string().into());
    }
    if let Some(s) = since_ms {
        sql.push_str(" AND l.ts >= ?");
        args.push(s.into());
    }
    if let Some(c) = cursor {
        sql.push_str(" AND l.id < ?");
        args.push(c.into());
    }
    sql.push_str(" ORDER BY l.id DESC LIMIT ?");
    args.push(limit.into());

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), row_to_log)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn row_to_log(row: &rusqlite::Row) -> rusqlite::Result<LogRow> {
    let payload_bytes: Vec<u8> = row.get(6)?;
    let payload: serde_json::Value =
        serde_json::from_slice(&payload_bytes).unwrap_or(serde_json::Value::Null);
    Ok(LogRow {
        id: row.get(0)?,
        device_id: row.get(1)?,
        topic: row.get(2)?,
        ts: row.get(3)?,
        level: row.get(4)?,
        content_type: row.get(5)?,
        payload,
    })
}
