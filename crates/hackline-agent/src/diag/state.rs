//! In-memory state shared between the agent's connect handler and
//! the diag HTTP server. Read-mostly: most fields are written once at
//! startup; the `connections` ring is the only mutable surface and
//! is bounded so the agent's RSS stays predictable.

use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;

const CONNECTION_RING_CAP: usize = 64;

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionEvent {
    pub at_unix: u64,
    pub port: u16,
    pub request_id: String,
    pub peer: Option<String>,
    pub outcome: String,
}

pub struct DiagState {
    pub zid: String,
    pub label: Option<String>,
    pub org: String,
    pub allowed_ports: Vec<u16>,
    pub session_zid: String,
    pub zenoh_mode: String,
    pub zenoh_listen: Vec<String>,
    pub zenoh_connect: Vec<String>,
    pub started_at: Instant,
    connections: Mutex<Vec<ConnectionEvent>>,
}

impl DiagState {
    pub fn new(
        zid: String,
        label: Option<String>,
        org: String,
        allowed_ports: Vec<u16>,
        session_zid: String,
        zenoh_mode: String,
        zenoh_listen: Vec<String>,
        zenoh_connect: Vec<String>,
    ) -> Self {
        Self {
            zid,
            label,
            org,
            allowed_ports,
            session_zid,
            zenoh_mode,
            zenoh_listen,
            zenoh_connect,
            started_at: Instant::now(),
            connections: Mutex::new(Vec::with_capacity(CONNECTION_RING_CAP)),
        }
    }

    /// Record a connection event. The ring is bounded; the oldest
    /// entry is dropped when full. Cheap enough that the connect
    /// handler can call it on every accept without measurable impact.
    pub fn push_connection(&self, ev: ConnectionEvent) {
        let mut g = match self.connections.lock() {
            Ok(g) => g,
            // A poisoned mutex here is not worth crashing the agent
            // for — diag is best-effort. Recover the inner vec and
            // keep going.
            Err(p) => p.into_inner(),
        };
        if g.len() >= CONNECTION_RING_CAP {
            g.remove(0);
        }
        g.push(ev);
    }

    /// Snapshot the ring for serialisation. Returns newest-first so
    /// the UI doesn't have to reverse.
    pub fn recent_connections(&self) -> Vec<ConnectionEvent> {
        let g = match self.connections.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let mut out: Vec<ConnectionEvent> = g.iter().rev().cloned().collect();
        out.shrink_to_fit();
        out
    }
}
