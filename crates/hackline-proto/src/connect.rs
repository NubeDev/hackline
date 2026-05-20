//! `ConnectRequest` and `ConnectAck` — the per-tunnel-open exchange
//! between gateway and agent.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Sent by the gateway as the payload of a Zenoh `get` on
/// `hackline/<zid>/tcp/<port>/connect`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ConnectRequest {
    /// Ties gateway and agent log lines together.
    pub request_id: Uuid,
    /// Peer address for the agent's audit log.
    pub peer: Option<String>,
}

/// Reply from the agent. If `ok` is true, the paired pub/sub channels
/// are ready for bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ConnectAck {
    pub request_id: Uuid,
    pub ok: bool,
    pub message: Option<String>,
}
