//! `events` table repository. Bounded ring buffer per device — the
//! oldest row(s) above `EVENTS_MAX_PER_DEVICE` are deleted in the
//! same transaction as the insert (SCOPE.md §7.2, §7.3). The ring
//! cap is hard-coded for v0.1; once the gateway grows a runtime
//! config block for retention this becomes a parameter.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;

use crate::error::GatewayError;

/// Default ring-buffer cap per device. Mirrors SCOPE.md §7.3.
pub const EVENTS_MAX_PER_DEVICE: i64 = 10_000;

#[derive(Debug, Clone, Serialize)]
pub struct EventRow {
    pub id: i64,
    pub device_id: i64,
    pub topic: String,
    /// Unix milliseconds since epoch (matches `MsgEnvelope.ts`).
    pub ts: i64,
    pub content_type: String,
    pub payload: serde_json::Value,
}

/// Insert one event and prune anything past the per-device cap. Both
/// steps run in a single transaction so a reader never sees the
/// table over-cap.
pub fn insert(
    conn: &mut Connection,
    device_id: i64,
    topic: &str,
    ts: i64,
    content_type: &str,
    payload: &serde_json::Value,
) -> Result<i64, GatewayError> {
    let bytes = serde_json::to_vec(payload)
        .map_err(|e| GatewayError::BadRequest(format!("event payload not serialisable: {e}")))?;
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO events (device_id, topic, ts, content_type, payload)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![device_id, topic, ts, content_type, bytes],
    )?;
    let id = tx.last_insert_rowid();
    prune(&tx, device_id, EVENTS_MAX_PER_DEVICE)?;
    tx.commit()?;
    Ok(id)
}

fn prune(tx: &Transaction<'_>, device_id: i64, cap: i64) -> Result<(), GatewayError> {
    tx.execute(
        "DELETE FROM events
         WHERE device_id = ?1
           AND id IN (
             SELECT id FROM events
             WHERE device_id = ?1
             ORDER BY id DESC
             LIMIT -1 OFFSET ?2
           )",
        params![device_id, cap],
    )?;
    Ok(())
}

/// Cursor-paginated listing. `cursor` is the `id` returned by the
/// previous page; rows are sorted DESC so newest comes first.
/// `topic_glob` filters with SQLite `GLOB` semantics so callers can
/// pass shell-style wildcards (`graph.slot.*`).
pub fn list(
    conn: &Connection,
    org_id: i64,
    device_id: Option<i64>,
    topic_glob: Option<&str>,
    since_ms: Option<i64>,
    cursor: Option<i64>,
    limit: i64,
) -> Result<Vec<EventRow>, GatewayError> {
    let limit = limit.clamp(1, 1000);
    // Cross-org isolation (SCOPE.md §13 Phase 4): every event row
    // belongs to a device, and a device belongs to exactly one org.
    // The join filters foreign-org rows out before we even consider
    // any caller-supplied filters.
    let mut sql = String::from(
        "SELECT e.id, e.device_id, e.topic, e.ts, e.content_type, e.payload
         FROM events e
         JOIN devices d ON d.id = e.device_id
         WHERE d.org_id = ?",
    );
    let mut args: Vec<rusqlite::types::Value> = Vec::new();
    args.push(org_id.into());
    if let Some(d) = device_id {
        sql.push_str(" AND e.device_id = ?");
        args.push(d.into());
    }
    if let Some(t) = topic_glob {
        sql.push_str(" AND e.topic GLOB ?");
        args.push(t.to_string().into());
    }
    if let Some(s) = since_ms {
        sql.push_str(" AND e.ts >= ?");
        args.push(s.into());
    }
    if let Some(c) = cursor {
        sql.push_str(" AND e.id < ?");
        args.push(c.into());
    }
    sql.push_str(" ORDER BY e.id DESC LIMIT ?");
    args.push(limit.into());

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), row_to_event)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn row_to_event(row: &rusqlite::Row) -> rusqlite::Result<EventRow> {
    let payload_bytes: Vec<u8> = row.get(5)?;
    let payload: serde_json::Value =
        serde_json::from_slice(&payload_bytes).unwrap_or(serde_json::Value::Null);
    Ok(EventRow {
        id: row.get(0)?,
        device_id: row.get(1)?,
        topic: row.get(2)?,
        ts: row.get(3)?,
        content_type: row.get(4)?,
        payload,
    })
}

/// Convenience used by the SSE bus — resolve a ZID to a device id
/// without bouncing through `db::devices` since callers here only
/// need the id.
pub fn device_id_for_zid(conn: &Connection, zid: &str) -> Result<Option<i64>, GatewayError> {
    conn.query_row(
        "SELECT id FROM devices WHERE zid = ?1",
        params![zid],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .map_err(Into::into)
}
