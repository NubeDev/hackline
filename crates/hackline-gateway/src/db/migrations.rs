//! Embedded SQL migrations. Run on every boot; idempotent. Each
//! version's status is tracked in `_migrations`; landed migrations
//! are never edited — new tables get a fresh `Vnnn__*.sql` file.

use rusqlite::Connection;

const MIGRATIONS: &[(i32, &str, &str)] = &[
    (
        1,
        "V001__init",
        include_str!("../../migrations/V001__init.sql"),
    ),
    (
        2,
        "V002__message_plane",
        include_str!("../../migrations/V002__message_plane.sql"),
    ),
    (
        3,
        "V003__cmd_outbox",
        include_str!("../../migrations/V003__cmd_outbox.sql"),
    ),
    (
        4,
        "V004__audit_session_columns",
        include_str!("../../migrations/V004__audit_session_columns.sql"),
    ),
    (
        5,
        "V005__orgs",
        include_str!("../../migrations/V005__orgs.sql"),
    ),
    (
        6,
        "V006__audit_tunnel_session_ts_seconds",
        include_str!("../../migrations/V006__audit_tunnel_session_ts_seconds.sql"),
    ),
    (
        7,
        "V007__audit_fks_on_delete_set_null",
        include_str!("../../migrations/V007__audit_fks_on_delete_set_null.sql"),
    ),
];

pub fn run(conn: &Connection) -> Result<(), crate::error::GatewayError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name    TEXT NOT NULL,
            applied_at INTEGER NOT NULL
        );",
    )
    .map_err(crate::error::GatewayError::Db)?;

    for (version, name, sql) in MIGRATIONS {
        let applied: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE version = ?1",
                [version],
                |row| row.get(0),
            )
            .map_err(crate::error::GatewayError::Db)?;
        if applied {
            continue;
        }
        conn.execute_batch(sql)
            .map_err(crate::error::GatewayError::Db)?;
        conn.execute(
            "INSERT INTO _migrations (version, name, applied_at) VALUES (?1, ?2, unixepoch())",
            rusqlite::params![version, name],
        )
        .map_err(crate::error::GatewayError::Db)?;
        tracing::info!(version, name, "applied migration");
    }
    Ok(())
}
