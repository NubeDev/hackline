//! `orgs` table repository. Multi-tenant boundary: every user and
//! every device carries an `org_id`; REST handlers filter by the
//! authenticated user's org so a row from another tenant is never
//! returned, mutated, or referenced. Cross-org isolation is also
//! enforced on the Zenoh fabric via the `hackline/<org_slug>/...`
//! keyexpr prefix (SCOPE.md §5.1, §6, §13 Phase 4).

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::GatewayError;

/// `default` org id seeded by `V005__orgs.sql`. Used by the claim flow
/// when the operator does not supply an explicit slug, and by tests.
pub const DEFAULT_ORG_ID: i64 = 1;
pub const DEFAULT_ORG_SLUG: &str = "default";

#[derive(Debug, Clone, Serialize)]
pub struct Org {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub created_at: i64,
}

pub fn insert(conn: &Connection, slug: &str, name: &str) -> Result<Org, GatewayError> {
    validate_slug(slug)?;
    conn.execute(
        "INSERT INTO orgs (slug, name, created_at) VALUES (?1, ?2, unixepoch())",
        params![slug, name],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(_, Some(ref msg))
            if msg.contains("UNIQUE") || msg.contains("CHECK") =>
        {
            GatewayError::BadRequest(format!("org slug `{slug}` invalid or taken"))
        }
        other => GatewayError::Db(other),
    })?;
    let id = conn.last_insert_rowid();
    get(conn, id)
}

pub fn list(conn: &Connection) -> Result<Vec<Org>, GatewayError> {
    let mut stmt = conn.prepare("SELECT id, slug, name, created_at FROM orgs ORDER BY id")?;
    let rows = stmt.query_map([], row_to_org)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get(conn: &Connection, id: i64) -> Result<Org, GatewayError> {
    conn.query_row(
        "SELECT id, slug, name, created_at FROM orgs WHERE id = ?1",
        params![id],
        row_to_org,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => GatewayError::NotFound,
        other => GatewayError::Db(other),
    })
}

pub fn get_by_slug(conn: &Connection, slug: &str) -> Result<Option<Org>, GatewayError> {
    conn.query_row(
        "SELECT id, slug, name, created_at FROM orgs WHERE slug = ?1",
        params![slug],
        row_to_org,
    )
    .optional()
    .map_err(GatewayError::Db)
}

fn row_to_org(row: &rusqlite::Row) -> rusqlite::Result<Org> {
    Ok(Org {
        id: row.get(0)?,
        slug: row.get(1)?,
        name: row.get(2)?,
        created_at: row.get(3)?,
    })
}

fn validate_slug(slug: &str) -> Result<(), GatewayError> {
    if slug.is_empty() || slug.len() > 63 {
        return Err(GatewayError::BadRequest(
            "org slug must be 1..=63 chars".into(),
        ));
    }
    let mut chars = slug.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return Err(GatewayError::BadRequest(
            "org slug must start with [a-z0-9]".into(),
        ));
    }
    for c in chars {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err(GatewayError::BadRequest(
                "org slug must match [a-z0-9][a-z0-9-]*".into(),
            ));
        }
    }
    Ok(())
}
