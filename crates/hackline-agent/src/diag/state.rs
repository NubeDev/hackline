//! Shared agent state read (and now also mutated) by the diag HTTP
//! server. Most fields are written once at startup; the connection
//! ring and the active-port map are mutable surfaces, both bounded
//! so the agent's RSS stays predictable.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use hackline_proto::Zid;
use serde::Serialize;
use tokio::task::JoinHandle;
use zenoh::Session;

const CONNECTION_RING_CAP: usize = 64;

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionEvent {
    pub at_unix: u64,
    pub port: u16,
    pub request_id: String,
    pub peer: Option<String>,
    pub outcome: String,
}

/// Active TCP-bridge queryable. The `JoinHandle` owns the queryable
/// internally; aborting the task drops it, which triggers a clean
/// undeclare on the Zenoh side. `from_config` lets the UI surface
/// which ports came from `agent.toml` and which were added at
/// runtime through the diag UI (the latter are lost on restart).
pub struct ActivePort {
    pub handle: JoinHandle<()>,
    pub from_config: bool,
}

pub struct DiagState {
    pub zid: String,
    pub label: Option<String>,
    pub org: String,
    pub session_zid: String,
    pub zenoh_mode: String,
    pub zenoh_listen: Vec<String>,
    pub zenoh_connect: Vec<String>,
    pub started_at: Instant,
    /// Held so the diag handlers can query peer state and declare
    /// new queryables when ports are added at runtime.
    pub session: Arc<Session>,
    /// Device identity reused by `connect::spawn_port_queryable`
    /// when the operator adds a port from the UI.
    pub zid_typed: Zid,
    ports: RwLock<HashMap<u16, ActivePort>>,
    connections: Mutex<Vec<ConnectionEvent>>,
}

impl DiagState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        zid: String,
        label: Option<String>,
        org: String,
        session_zid: String,
        zenoh_mode: String,
        zenoh_listen: Vec<String>,
        zenoh_connect: Vec<String>,
        session: Arc<Session>,
        zid_typed: Zid,
    ) -> Self {
        Self {
            zid,
            label,
            org,
            session_zid,
            zenoh_mode,
            zenoh_listen,
            zenoh_connect,
            started_at: Instant::now(),
            session,
            zid_typed,
            ports: RwLock::new(HashMap::new()),
            connections: Mutex::new(Vec::with_capacity(CONNECTION_RING_CAP)),
        }
    }

    /// Snapshot of active ports, sorted for stable UI ordering.
    pub fn active_ports(&self) -> Vec<(u16, bool)> {
        let g = self.ports.read().expect("ports lock poisoned");
        let mut v: Vec<(u16, bool)> = g.iter().map(|(&p, a)| (p, a.from_config)).collect();
        v.sort_by_key(|(p, _)| *p);
        v
    }

    pub fn has_port(&self, port: u16) -> bool {
        self.ports
            .read()
            .expect("ports lock poisoned")
            .contains_key(&port)
    }

    /// Insert an `ActivePort`. Returns false (and aborts the passed
    /// handle) if the port is already tracked, so a duplicate-add
    /// race can't leak a queryable.
    pub fn insert_port(&self, port: u16, info: ActivePort) -> bool {
        let mut g = self.ports.write().expect("ports lock poisoned");
        if g.contains_key(&port) {
            info.handle.abort();
            return false;
        }
        g.insert(port, info);
        true
    }

    /// Abort the queryable task for `port` and remove it from the
    /// map. Returns true if it was present.
    pub fn remove_port(&self, port: u16) -> bool {
        let mut g = self.ports.write().expect("ports lock poisoned");
        match g.remove(&port) {
            Some(ap) => {
                ap.handle.abort();
                true
            }
            None => false,
        }
    }

    pub fn push_connection(&self, ev: ConnectionEvent) {
        let mut g = match self.connections.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if g.len() >= CONNECTION_RING_CAP {
            g.remove(0);
        }
        g.push(ev);
    }

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
