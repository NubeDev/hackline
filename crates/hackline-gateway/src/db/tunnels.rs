//! `tunnels` table repository. The `kind`/hostname/port `CHECK`
//! constraint is in the migration; this layer just maps rows.

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::GatewayError;

#[derive(Debug, Clone, Serialize)]
pub struct Tunnel {
    pub id: i64,
    pub device_id: i64,
    pub kind: String,
    pub local_port: i64,
    pub public_hostname: Option<String>,
    pub public_port: Option<i64>,
    pub enabled: bool,
    pub created_at: i64,
}

/// Row used by the tunnel manager to spin up listeners. Joins
/// the device ZID so the bridge knows where to route.
#[derive(Debug, Clone)]
pub struct TunnelWithZid {
    pub id: i64,
    pub device_id: i64,
    pub zid: String,
    /// Owning org's slug; the tunnel listener needs it to build the
    /// `hackline/<org_slug>/<zid>/...` keyexpr the agent listens on.
    pub org_slug: String,
    pub kind: String,
    pub local_port: u16,
    pub public_port: u16,
    pub enabled: bool,
}

pub fn insert(
    conn: &Connection,
    device_id: i64,
    kind: &str,
    local_port: i64,
    public_hostname: Option<&str>,
    public_port: Option<i64>,
) -> Result<Tunnel, GatewayError> {
    conn.execute(
        "INSERT INTO tunnels (device_id, kind, local_port, public_hostname, public_port, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, unixepoch())",
        params![device_id, kind, local_port, public_hostname, public_port],
    )?;
    let id = conn.last_insert_rowid();
    get(conn, id)
}

pub fn list(conn: &Connection) -> Result<Vec<Tunnel>, GatewayError> {
    let mut stmt = conn.prepare(
        "SELECT id, device_id, kind, local_port, public_hostname, public_port, enabled, created_at
         FROM tunnels ORDER BY id",
    )?;
    let rows = stmt.query_map([], row_to_tunnel)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Tunnels visible to `org_id` — the join filters by the owning
/// device's org (SCOPE.md §13 Phase 4).
pub fn list_in_org(conn: &Connection, org_id: i64) -> Result<Vec<Tunnel>, GatewayError> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.device_id, t.kind, t.local_port, t.public_hostname,
                t.public_port, t.enabled, t.created_at
         FROM tunnels t
         JOIN devices d ON d.id = t.device_id
         WHERE d.org_id = ?1
         ORDER BY t.id",
    )?;
    let rows = stmt.query_map(params![org_id], row_to_tunnel)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get(conn: &Connection, id: i64) -> Result<Tunnel, GatewayError> {
    conn.query_row(
        "SELECT id, device_id, kind, local_port, public_hostname, public_port, enabled, created_at
         FROM tunnels WHERE id = ?1",
        params![id],
        row_to_tunnel,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => GatewayError::NotFound,
        other => GatewayError::Db(other),
    })
}

/// Delete a tunnel iff its owning device belongs to `org_id`. Foreign
/// org returns `false` (which handlers translate to 404 — no way to
/// distinguish from "no such tunnel").
pub fn delete_in_org(conn: &Connection, org_id: i64, id: i64) -> Result<bool, GatewayError> {
    let n = conn.execute(
        "DELETE FROM tunnels
         WHERE id = ?1
           AND device_id IN (SELECT id FROM devices WHERE org_id = ?2)",
        params![id, org_id],
    )?;
    Ok(n > 0)
}

/// Load all enabled TCP tunnels with their device ZID, for the
/// tunnel manager to spin up listeners.
pub fn list_active_tcp(conn: &Connection) -> Result<Vec<TunnelWithZid>, GatewayError> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.device_id, d.zid, o.slug, t.kind, t.local_port, t.public_port, t.enabled
         FROM tunnels t
         JOIN devices d ON d.id = t.device_id
         JOIN orgs    o ON o.id = d.org_id
         WHERE t.enabled = 1 AND t.kind = 'tcp' AND t.public_port IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(TunnelWithZid {
            id: row.get(0)?,
            device_id: row.get(1)?,
            zid: row.get(2)?,
            org_slug: row.get(3)?,
            kind: row.get(4)?,
            local_port: row.get::<_, i64>(5)? as u16,
            public_port: row.get::<_, i64>(6)? as u16,
            enabled: row.get(7)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn row_to_tunnel(row: &rusqlite::Row) -> rusqlite::Result<Tunnel> {
    Ok(Tunnel {
        id: row.get(0)?,
        device_id: row.get(1)?,
        kind: row.get(2)?,
        local_port: row.get(3)?,
        public_hostname: row.get(4)?,
        public_port: row.get(5)?,
        enabled: row.get(6)?,
        created_at: row.get(7)?,
    })
}
