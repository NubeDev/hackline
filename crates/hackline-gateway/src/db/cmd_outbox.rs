//! `cmd_outbox` table repository. Cloud writes via REST, the cmd
//! delivery loop reads `pending`, the cmd-ack fan-in updates ack
//! columns. Write-time enforcement of the per-device row cap so a
//! reader never sees the table over-cap (SCOPE.md §7.3).

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::GatewayError;

/// Per-device row cap. SCOPE.md §7.3 default; once the gateway grows
/// a runtime retention config block this becomes a parameter.
pub const CMD_MAX_PER_DEVICE: i64 = 1000;

/// Default TTL for an enqueued command if the REST caller omits one.
/// SCOPE.md §7.3 `cmd.default_ttl` = 7 days.
pub const CMD_DEFAULT_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize)]
pub struct CmdRow {
    pub id: i64,
    pub cmd_id: String,
    pub device_id: i64,
    pub topic: String,
    pub content_type: String,
    pub payload: serde_json::Value,
    pub enqueued_at: i64,
    pub expires_at: i64,
    pub delivered_at: Option<i64>,
    pub ack_at: Option<i64>,
    pub ack_result: Option<String>,
    pub ack_detail: Option<String>,
    pub attempts: i64,
    pub last_error: Option<String>,
    /// Derived status string: `pending` | `delivered` | `acked` |
    /// `expired`. Computed so the REST consumer doesn't have to
    /// reconstruct the state machine.
    pub status: String,
}

/// Enqueue a new command. Refuses if the device already has
/// `CMD_MAX_PER_DEVICE` non-acked rows whose oldest pending row is
/// not yet past `expires_at` — we are not a general broker
/// (SCOPE.md §2).
pub fn enqueue(
    conn: &mut Connection,
    cmd_id: &str,
    device_id: i64,
    topic: &str,
    content_type: &str,
    payload: &serde_json::Value,
    enqueued_at: i64,
    expires_at: i64,
) -> Result<i64, GatewayError> {
    let bytes = serde_json::to_vec(payload)
        .map_err(|e| GatewayError::BadRequest(format!("cmd payload not serialisable: {e}")))?;
    if bytes.len() > 65536 {
        return Err(GatewayError::BadRequest(format!(
            "cmd payload {} bytes exceeds 64 KiB cap",
            bytes.len()
        )));
    }
    let tx = conn.transaction()?;

    let count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM cmd_outbox WHERE device_id = ?1 AND ack_at IS NULL",
        params![device_id],
        |row| row.get(0),
    )?;
    if count >= CMD_MAX_PER_DEVICE {
        return Err(GatewayError::BadRequest(format!(
            "device {device_id} cmd_outbox full ({count} pending)"
        )));
    }

    tx.execute(
        "INSERT INTO cmd_outbox
           (cmd_id, device_id, topic, content_type, payload, enqueued_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            cmd_id,
            device_id,
            topic,
            content_type,
            bytes,
            enqueued_at,
            expires_at
        ],
    )?;
    let id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(id)
}

pub fn get_by_cmd_id(conn: &Connection, cmd_id: &str) -> Result<Option<CmdRow>, GatewayError> {
    let mut stmt = conn.prepare(
        "SELECT id, cmd_id, device_id, topic, content_type, payload,
                enqueued_at, expires_at, delivered_at, ack_at,
                ack_result, ack_detail, attempts, last_error
         FROM cmd_outbox WHERE cmd_id = ?1",
    )?;
    let row = stmt
        .query_row(params![cmd_id], row_to_cmd)
        .optional()
        .map_err(GatewayError::Db)?;
    Ok(row)
}

/// Cursor-paginated listing for `GET /v1/devices/:id/cmd`.
/// `status_filter`: `None` returns every row; `Some("pending")` →
/// `delivered_at IS NULL`; `"delivered"` → `delivered_at IS NOT NULL
/// AND ack_at IS NULL`; `"acked"` → `ack_at IS NOT NULL`.
pub fn list_by_device(
    conn: &Connection,
    device_id: i64,
    status_filter: Option<&str>,
    cursor: Option<i64>,
    limit: i64,
) -> Result<Vec<CmdRow>, GatewayError> {
    let limit = limit.clamp(1, 1000);
    let mut sql = String::from(
        "SELECT id, cmd_id, device_id, topic, content_type, payload,
                enqueued_at, expires_at, delivered_at, ack_at,
                ack_result, ack_detail, attempts, last_error
         FROM cmd_outbox WHERE device_id = ?",
    );
    let mut args: Vec<rusqlite::types::Value> = vec![device_id.into()];
    match status_filter {
        Some("pending") => sql.push_str(" AND delivered_at IS NULL"),
        Some("delivered") => sql.push_str(" AND delivered_at IS NOT NULL AND ack_at IS NULL"),
        Some("acked") => sql.push_str(" AND ack_at IS NOT NULL"),
        Some(other) => {
            return Err(GatewayError::BadRequest(format!(
                "unknown status `{other}`"
            )))
        }
        None => {}
    }
    if let Some(c) = cursor {
        sql.push_str(" AND id < ?");
        args.push(c.into());
    }
    sql.push_str(" ORDER BY id DESC LIMIT ?");
    args.push(limit.into());

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), row_to_cmd)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Rows that need to be (re)published — undelivered and not expired.
pub fn list_pending(conn: &Connection, now_ms: i64) -> Result<Vec<CmdRow>, GatewayError> {
    let mut stmt = conn.prepare(
        "SELECT id, cmd_id, device_id, topic, content_type, payload,
                enqueued_at, expires_at, delivered_at, ack_at,
                ack_result, ack_detail, attempts, last_error
         FROM cmd_outbox
         WHERE delivered_at IS NULL AND expires_at > ?1
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![now_ms], row_to_cmd)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn mark_delivered(conn: &Connection, cmd_id: &str, now_ms: i64) -> Result<(), GatewayError> {
    conn.execute(
        "UPDATE cmd_outbox
         SET delivered_at = ?2, attempts = attempts + 1
         WHERE cmd_id = ?1 AND delivered_at IS NULL",
        params![cmd_id, now_ms],
    )?;
    Ok(())
}

pub fn record_ack(
    conn: &Connection,
    cmd_id: &str,
    result: &str,
    detail: Option<&str>,
    now_ms: i64,
) -> Result<bool, GatewayError> {
    let n = conn.execute(
        "UPDATE cmd_outbox
         SET ack_at = ?2, ack_result = ?3, ack_detail = ?4,
             delivered_at = COALESCE(delivered_at, ?2)
         WHERE cmd_id = ?1",
        params![cmd_id, now_ms, result, detail],
    )?;
    Ok(n > 0)
}

/// Per-device pending depth, for the `hackline_cmd_outbox_depth`
/// metric. Only devices with at least one un-acked row appear; the
/// Prometheus formatter renders one labelled sample per entry.
pub fn pending_depth_by_device(
    conn: &Connection,
) -> Result<std::collections::BTreeMap<String, i64>, GatewayError> {
    let mut stmt = conn.prepare(
        "SELECT device_id, COUNT(*) FROM cmd_outbox
         WHERE ack_at IS NULL
         GROUP BY device_id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
    let mut out = std::collections::BTreeMap::new();
    for r in rows {
        let (dev, n) = r?;
        out.insert(dev.to_string(), n);
    }
    Ok(out)
}

/// Cancel a queued (still-pending) row. Returns false if the row
/// already moved to `delivered` or `acked` — cancel is best-effort.
pub fn cancel(conn: &Connection, cmd_id: &str) -> Result<bool, GatewayError> {
    let n = conn.execute(
        "DELETE FROM cmd_outbox WHERE cmd_id = ?1 AND delivered_at IS NULL",
        params![cmd_id],
    )?;
    Ok(n > 0)
}

fn row_to_cmd(row: &rusqlite::Row) -> rusqlite::Result<CmdRow> {
    let payload_bytes: Vec<u8> = row.get(5)?;
    let payload: serde_json::Value =
        serde_json::from_slice(&payload_bytes).unwrap_or(serde_json::Value::Null);
    let delivered_at: Option<i64> = row.get(8)?;
    let ack_at: Option<i64> = row.get(9)?;
    let expires_at: i64 = row.get(7)?;
    let status = derive_status(delivered_at, ack_at, expires_at);
    Ok(CmdRow {
        id: row.get(0)?,
        cmd_id: row.get(1)?,
        device_id: row.get(2)?,
        topic: row.get(3)?,
        content_type: row.get(4)?,
        payload,
        enqueued_at: row.get(6)?,
        expires_at,
        delivered_at,
        ack_at,
        ack_result: row.get(10)?,
        ack_detail: row.get(11)?,
        attempts: row.get(12)?,
        last_error: row.get(13)?,
        status,
    })
}

fn derive_status(delivered_at: Option<i64>, ack_at: Option<i64>, expires_at: i64) -> String {
    let now = now_ms();
    if ack_at.is_some() {
        "acked".into()
    } else if delivered_at.is_some() {
        "delivered".into()
    } else if expires_at <= now {
        "expired".into()
    } else {
        "pending".into()
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
