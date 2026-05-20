//! Cloud-side hackline gateway. axum REST + SSE, TCP listeners,
//! SQLite, Zenoh client. The library half — binaries live in `bin/`.

pub mod api;
pub mod auth;
pub mod cmd_delivery;
pub mod config;
pub mod db;
pub mod error;
pub mod events_bus;
pub mod liveliness;
pub mod metrics;
pub mod msg_fanin;
pub mod rtt_cache;
pub mod state;
#[cfg(feature = "tls")]
pub mod tls;
pub mod tunnel;
pub mod zenoh_client;
