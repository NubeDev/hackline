//! `users` table repository: insert, lookup-by-token-hash, list,
//! delete, scope checks.

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::GatewayError;

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: i64,
    pub org_id: i64,
    pub name: String,
    pub role: String,
    pub device_scope: String,
    pub tunnel_scope: String,
    pub expires_at: Option<i64>,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

/// Lookup a user by token hash. Used by the auth middleware on every
/// authenticated request.
pub fn get_by_token_hash(
    conn: &Connection,
    token_hash: &str,
) -> Result<Option<User>, GatewayError> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, name, role, device_scope, tunnel_scope, expires_at, created_at, last_used_at
         FROM users WHERE token_hash = ?1",
    )?;
    let mut rows = stmt.query_map(params![token_hash], row_to_user)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn insert(
    conn: &Connection,
    org_id: i64,
    name: &str,
    role: &str,
    token_hash: &str,
) -> Result<User, GatewayError> {
    conn.execute(
        "INSERT INTO users (org_id, name, role, token_hash, created_at)
         VALUES (?1, ?2, ?3, ?4, unixepoch())",
        params![org_id, name, role, token_hash],
    )?;
    let id = conn.last_insert_rowid();
    get(conn, id)
}

pub fn list(conn: &Connection) -> Result<Vec<User>, GatewayError> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, name, role, device_scope, tunnel_scope, expires_at, created_at, last_used_at
         FROM users ORDER BY id",
    )?;
    let rows = stmt.query_map([], row_to_user)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_in_org(conn: &Connection, org_id: i64) -> Result<Vec<User>, GatewayError> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, name, role, device_scope, tunnel_scope, expires_at, created_at, last_used_at
         FROM users WHERE org_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map(params![org_id], row_to_user)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get(conn: &Connection, id: i64) -> Result<User, GatewayError> {
    conn.query_row(
        "SELECT id, org_id, name, role, device_scope, tunnel_scope, expires_at, created_at, last_used_at
         FROM users WHERE id = ?1",
        params![id],
        row_to_user,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => GatewayError::NotFound,
        other => GatewayError::Db(other),
    })
}

pub fn delete(conn: &Connection, id: i64) -> Result<bool, GatewayError> {
    let n = conn.execute("DELETE FROM users WHERE id = ?1", params![id])?;
    Ok(n > 0)
}

pub fn delete_in_org(conn: &Connection, org_id: i64, id: i64) -> Result<bool, GatewayError> {
    let n = conn.execute(
        "DELETE FROM users WHERE id = ?1 AND org_id = ?2",
        params![id, org_id],
    )?;
    Ok(n > 0)
}

/// Update `last_used_at` for the user. Fire-and-forget; called from
/// the auth middleware after successful validation.
pub fn touch(conn: &Connection, id: i64) -> Result<(), GatewayError> {
    conn.execute(
        "UPDATE users SET last_used_at = unixepoch() WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

/// Update the token hash for an existing user (mint new token).
pub fn update_token_hash(conn: &Connection, id: i64, token_hash: &str) -> Result<(), GatewayError> {
    let n = conn.execute(
        "UPDATE users SET token_hash = ?1 WHERE id = ?2",
        params![token_hash, id],
    )?;
    if n == 0 {
        return Err(GatewayError::NotFound);
    }
    Ok(())
}

fn row_to_user(row: &rusqlite::Row) -> rusqlite::Result<User> {
    Ok(User {
        id: row.get(0)?,
        org_id: row.get(1)?,
        name: row.get(2)?,
        role: row.get(3)?,
        device_scope: row.get(4)?,
        tunnel_scope: row.get(5)?,
        expires_at: row.get(6)?,
        created_at: row.get(7)?,
        last_used_at: row.get(8)?,
    })
}
