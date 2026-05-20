//! `AgentInfo` — payload returned by `hackline/<org>/<zid>/info`.
//!
//! Identity (`zid`) and runtime facts (`version`, `uptime_s`) come
//! from the agent itself; `allowed_ports` is the policy the agent
//! loaded from its config. The device's operator-assigned label
//! deliberately is *not* on this wire — that's a row on the gateway
//! and reporting it from the agent would create two sources of truth
//! that can disagree.

use serde::{Deserialize, Serialize};

/// Describes a running agent instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AgentInfo {
    pub zid: String,
    pub version: String,
    pub allowed_ports: Vec<u16>,
    pub uptime_s: u64,
}
