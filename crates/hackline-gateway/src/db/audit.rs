//! `audit` table repository. Two row shapes share the table:
//!
//! - **Point-in-time actions** (`cmd.send`, `api.call`, `device.create`,
//!   ...): `insert(...)` writes one row with `ts` + `action` + `detail`.
//! - **Tunnel sessions** (`tunnel.session`): `insert_tunnel_session_open`
//!   writes one row at bridge open and `finalize_tunnel_session`
//!   updates the same row with byte counters and `ts_close` when the
//!   bridge closes. Per-event logging would be hundreds of millions of
//!   rows/year at fleet scale, so a session is one row, not two.
//!
//! Retention strategy is documented in `DOCS/DATABASE.md`.

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::GatewayError;

#[derive(Debug, Serialize)]
pub struct AuditEntry {
    pub id: i64,
    pub ts: i64,
    pub ts_close: Option<i64>,
    pub user_id: Option<i64>,
    pub device_id: Option<i64>,
    pub tunnel_id: Option<i64>,
    pub request_id: Option<String>,
    pub action: String,
    pub peer: Option<String>,
    pub bytes_up: Option<i64>,
    pub bytes_down: Option<i64>,
    pub detail: Option<String>,
}

/// Append a point-in-time audit action. Used by every handler that
/// emits a SCOPE.md §7.2 well-known action other than `tunnel.session`.
pub fn insert(
    conn: &Connection,
    user_id: Option<i64>,
    device_id: Option<i64>,
    tunnel_id: Option<i64>,
    action: &str,
    detail: Option<&str>,
) -> Result<(), GatewayError> {
    conn.execute(
        "INSERT INTO audit (ts, user_id, device_id, tunnel_id, action, detail)
         VALUES (unixepoch(), ?1, ?2, ?3, ?4, ?5)",
        params![user_id, device_id, tunnel_id, action, detail],
    )?;
    Ok(())
}

/// Insert the `tunnel.session` row at bridge open. Returns the row id
/// so the caller can finalise it with byte counters when the bridge
/// closes.
///
/// `ts_open_ms` is converted to unix seconds before storage so the
/// `audit.ts` column carries one unit (seconds) for every row,
/// matching `DOCS/openapi.yaml` §AuditEntry `at` and the goal-20
/// REST projection. Callers retain ms because the bridge code uses
/// the same value to compute `duration_ms` in metrics.
pub fn insert_tunnel_session_open(
    conn: &Connection,
    device_id: i64,
    tunnel_id: Option<i64>,
    request_id: &str,
    peer: Option<&str>,
    ts_open_ms: i64,
) -> Result<i64, GatewayError> {
    let ts_open_s = ts_open_ms / 1000;
    conn.execute(
        "INSERT INTO audit (ts, user_id, device_id, tunnel_id, action, request_id, peer)
         VALUES (?1, NULL, ?2, ?3, 'tunnel.session', ?4, ?5)",
        params![ts_open_s, device_id, tunnel_id, request_id, peer],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Stamp the close-time fields on a `tunnel.session` row. `ts_close_ms`
/// is converted to seconds for the same reason as
/// `insert_tunnel_session_open`.
pub fn finalize_tunnel_session(
    conn: &Connection,
    id: i64,
    bytes_up: u64,
    bytes_down: u64,
    ts_close_ms: i64,
) -> Result<(), GatewayError> {
    let ts_close_s = ts_close_ms / 1000;
    conn.execute(
        "UPDATE audit
         SET ts_close = ?2, bytes_up = ?3, bytes_down = ?4
         WHERE id = ?1",
        params![id, ts_close_s, bytes_up as i64, bytes_down as i64],
    )?;
    Ok(())
}

/// Recent audit rows visible to `org_id`. SCOPE.md §13 Phase 4:
/// a row is visible iff its `device_id` lives in the caller's org,
/// or it has no device id (claim, auth.login) and its `user_id`
/// belongs to the caller's org. Rows referencing a device the caller
/// doesn't own (or a user in another org) are filtered out at the
/// SQL level so a cross-org caller cannot even count them.
pub fn list_recent(
    conn: &Connection,
    org_id: i64,
    limit: i64,
) -> Result<Vec<AuditEntry>, GatewayError> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.ts, a.ts_close, a.user_id, a.device_id, a.tunnel_id,
                a.request_id, a.action, a.peer, a.bytes_up, a.bytes_down, a.detail
         FROM audit a
         LEFT JOIN devices d ON d.id = a.device_id
         LEFT JOIN users   u ON u.id = a.user_id
         WHERE (a.device_id IS NULL OR d.org_id = ?1)
           AND (a.user_id   IS NULL OR u.org_id = ?1)
         ORDER BY a.id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![org_id, limit], |row| {
        Ok(AuditEntry {
            id: row.get(0)?,
            ts: row.get(1)?,
            ts_close: row.get(2)?,
            user_id: row.get(3)?,
            device_id: row.get(4)?,
            tunnel_id: row.get(5)?,
            request_id: row.get(6)?,
            action: row.get(7)?,
            peer: row.get(8)?,
            bytes_up: row.get(9)?,
            bytes_down: row.get(10)?,
            detail: row.get(11)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn count(conn: &Connection) -> Result<i64, GatewayError> {
    conn.query_row("SELECT COUNT(*) FROM audit", [], |row| row.get::<_, i64>(0))
        .map_err(GatewayError::Db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        // Use whichever org id the migrations seeded; create a
        // device under it. The org / device columns are not
        // observed by this test, only the `audit.ts` value is.
        let org_id: i64 = conn
            .query_row("SELECT id FROM orgs ORDER BY id LIMIT 1", [], |r| r.get(0))
            .unwrap();
        conn.execute(
            "INSERT INTO devices (org_id, zid, label, created_at)
             VALUES (?1, 'zid1', 'd', unixepoch())",
            [org_id],
        )
        .unwrap();
        conn
    }

    /// Lock-in for goal 22: every `audit.ts` value lands in seconds,
    /// even though the bridge code passes ms. A regression that
    /// stores ms here would put `tunnel.session` rows ~1970 in any
    /// `relTime`-rendered audit log (UI assumes seconds per the
    /// goal-20 projection contract).
    #[test]
    fn tunnel_session_ts_is_stored_in_seconds() {
        let conn = fresh_db();
        let device_id: i64 = conn
            .query_row("SELECT id FROM devices ORDER BY id DESC LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        let now_ms = 1_700_000_000_000_i64;
        let id = insert_tunnel_session_open(&conn, device_id, None, "req-1", None, now_ms).unwrap();
        finalize_tunnel_session(&conn, id, 10, 20, now_ms + 5_000).unwrap();

        let (ts, ts_close): (i64, i64) = conn
            .query_row(
                "SELECT ts, ts_close FROM audit WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(ts, now_ms / 1000);
        assert_eq!(ts_close, (now_ms + 5_000) / 1000);
    }

    /// Lock-in for goal 23 (V006 backfill). Pre-fix bridge code
    /// stamped `tunnel.session` rows in ms; V006 divides any
    /// `tunnel.session` row whose `ts` (or `ts_close`) is large
    /// enough that it cannot already be seconds. The threshold
    /// `10_000_000_000` corresponds to year 2286 in seconds, so
    /// any value above it today is necessarily a ms stamp.
    ///
    /// This test bypasses the migration runner (V006 has already
    /// been applied by `fresh_db`) and re-applies the SQL after
    /// inserting a synthetic legacy row, plus a control row that
    /// is already in seconds and a non-tunnel row that happens
    /// to be large. The second must round-trip unchanged; the
    /// third must not be touched (the migration is scoped to
    /// `action = 'tunnel.session'`).
    #[test]
    fn v006_backfill_converts_ms_rows_only() {
        let conn = fresh_db();
        let device_id: i64 = conn
            .query_row("SELECT id FROM devices ORDER BY id DESC LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        let legacy_ms: i64 = 1_700_000_000_000;
        let legacy_close_ms: i64 = 1_700_000_005_000;
        let modern_s: i64 = 1_700_000_010;
        let large_non_tunnel_s: i64 = 99_999_999_999; // year 5138 in seconds; leave alone.

        conn.execute(
            "INSERT INTO audit (ts, ts_close, device_id, action, detail)
             VALUES (?1, ?2, ?3, 'tunnel.session', NULL)",
            rusqlite::params![legacy_ms, legacy_close_ms, device_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO audit (ts, device_id, action, detail)
             VALUES (?1, ?2, 'tunnel.session', NULL)",
            rusqlite::params![modern_s, device_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO audit (ts, device_id, action, detail)
             VALUES (?1, ?2, 'cmd.send', NULL)",
            rusqlite::params![large_non_tunnel_s, device_id],
        )
        .unwrap();

        conn.execute_batch(include_str!(
            "../../migrations/V006__audit_tunnel_session_ts_seconds.sql"
        ))
        .unwrap();

        let (ts, ts_close): (i64, i64) = conn
            .query_row(
                "SELECT ts, ts_close FROM audit WHERE ts_close IS NOT NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(ts, legacy_ms / 1000);
        assert_eq!(ts_close, legacy_close_ms / 1000);

        let modern_after: i64 = conn
            .query_row(
                "SELECT ts FROM audit WHERE action = 'tunnel.session' AND ts_close IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(modern_after, modern_s);

        let non_tunnel_after: i64 = conn
            .query_row("SELECT ts FROM audit WHERE action = 'cmd.send'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(non_tunnel_after, large_non_tunnel_s);
    }

    /// Lock-in for goal 24 (V007). Before V007, deleting a tunnel
    /// or device with any audit history failed under
    /// `PRAGMA foreign_keys = ON` because `audit.{tunnel,device}_id`
    /// had no `ON DELETE` clause. After V007 the FKs are
    /// `ON DELETE SET NULL`, so the parent delete succeeds and the
    /// audit row survives with the FK column nulled. The audit
    /// projection in `api/audit/list.rs` already tolerates a NULL
    /// FK by emitting an empty `subject`, so the wire shape does
    /// not regress.
    ///
    /// FKs default to OFF on a fresh `Connection::open_in_memory`,
    /// so the test toggles them on after migrations have run (the
    /// V007 SQL flips them off-then-on around its recreate dance,
    /// which leaves them on but only because the SQL ends with
    /// `PRAGMA foreign_keys = ON` — explicit toggle here makes the
    /// test independent of migration internals).
    #[test]
    fn v007_audit_fks_set_null_on_parent_delete() {
        let conn = fresh_db();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

        let device_id: i64 = conn
            .query_row("SELECT id FROM devices ORDER BY id DESC LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        conn.execute(
            "INSERT INTO tunnels
               (id, device_id, kind, local_port, public_port, enabled, created_at)
             VALUES (1, ?1, 'tcp', 8080, 19000, 1, unixepoch())",
            [device_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO audit (ts, device_id, tunnel_id, action, detail)
             VALUES (unixepoch(), ?1, 1, 'tunnel.session', NULL)",
            [device_id],
        )
        .unwrap();
        let audit_id: i64 = conn
            .query_row("SELECT last_insert_rowid()", [], |r| r.get(0))
            .unwrap();

        conn.execute("DELETE FROM tunnels WHERE id = 1", [])
            .unwrap();

        let tunnel_id_after: Option<i64> = conn
            .query_row(
                "SELECT tunnel_id FROM audit WHERE id = ?1",
                [audit_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tunnel_id_after, None, "tunnel_id should be SET NULL");
        let device_id_after: Option<i64> = conn
            .query_row(
                "SELECT device_id FROM audit WHERE id = ?1",
                [audit_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            device_id_after,
            Some(device_id),
            "device_id is unrelated to the tunnel delete"
        );

        conn.execute("DELETE FROM devices WHERE id = ?1", [device_id])
            .unwrap();
        let device_id_after_dev_delete: Option<i64> = conn
            .query_row(
                "SELECT device_id FROM audit WHERE id = ?1",
                [audit_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            device_id_after_dev_delete, None,
            "device_id should be SET NULL after device delete"
        );
    }
}
