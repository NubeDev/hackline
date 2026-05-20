//! Bridging-layer errors. Distinct from `hackline-proto::error` so
//! that callers can match on transport vs. protocol failures.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("zenoh: {0}")]
    Zenoh(#[from] zenoh::Error),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("proto: {0}")]
    Proto(#[from] hackline_proto::error::ProtoError),

    #[error("connect rejected: {0}")]
    Rejected(String),

    #[error("timeout waiting for connect ack")]
    AckTimeout,
}
