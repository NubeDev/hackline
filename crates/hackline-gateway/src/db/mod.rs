//! SQLite repositories. One file per table; pool + transaction
//! helpers in `pool.rs`. All sync; called from async handlers via
//! `tokio::task::spawn_blocking`. Pool max-size must stay <= the
//! tokio blocking-thread budget — see `DOCS/DATABASE.md`.

pub mod audit;
pub mod claim;
pub mod cmd_outbox;
pub mod devices;
pub mod events;
pub mod logs;
pub mod migrations;
pub mod orgs;
pub mod pool;
pub mod tunnels;
pub mod users;
