//! SSE event variants emitted by the gateway. One enum variant per
//! event type listed in `DOCS/REST-API.md`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Zero-length payload published on a stream channel to signal close.
pub const CLOSE_SENTINEL: &[u8] = b"";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    DeviceOnline { device_id: i64 },
    DeviceOffline { device_id: i64 },
    TunnelOpened { tunnel_id: i64 },
    TunnelClosed { tunnel_id: i64 },
    TunnelConnection { tunnel_id: i64, request_id: Uuid },
}
