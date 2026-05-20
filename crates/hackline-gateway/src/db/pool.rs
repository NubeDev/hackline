//! r2d2 pool setup. Opens SQLite with WAL mode and the foreign-key
//! pragma. Sized conservatively against tokio's blocking-thread pool.

use std::path::Path;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

pub type DbPool = Pool<SqliteConnectionManager>;

/// Open (or create) a SQLite database and return a connection pool.
///
/// WAL mode is set once via a dedicated single connection before the pool
/// opens. Setting it inside `with_init` causes all 16 pool connections to
/// race for the exclusive lock WAL activation requires, producing spurious
/// "database is locked" errors at startup.
pub fn open(path: &Path) -> Result<DbPool, crate::error::GatewayError> {
    // WAL mode is a database-level setting that persists across connections;
    // it must be set before the pool opens its connections to avoid the race.
    {
        let conn = rusqlite::Connection::open(path).map_err(crate::error::GatewayError::Db)?;
        conn.execute_batch("PRAGMA journal_mode = WAL;")
            .map_err(crate::error::GatewayError::Db)?;
    }

    let manager = SqliteConnectionManager::file(path).with_init(|conn| {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
                 PRAGMA busy_timeout = 5000;",
        )?;
        Ok(())
    });
    let pool = Pool::builder()
        .max_size(16)
        .build(manager)
        .map_err(|e| crate::error::GatewayError::Config(format!("db pool: {e}")))?;
    Ok(pool)
}
