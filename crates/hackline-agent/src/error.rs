//! Agent-level error type. Wraps proto, core, and io errors with
//! enough context for the journal to be useful in production.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("config: {0}")]
    Config(String),

    #[error("bridge: {0}")]
    Bridge(#[from] hackline_core::error::BridgeError),

    #[error("zenoh: {0}")]
    Zenoh(#[from] zenoh::Error),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
