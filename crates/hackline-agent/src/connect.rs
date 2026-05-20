//! Handles `hackline/<org>/<zid>/tcp/<port>/connect` queries.
//! Validates the requested port against the agent's active set
//! before opening a loopback TCP connection and handing off to
//! `hackline-core::bridge`. The "active set" is mutable at runtime —
//! `DiagState` owns the map and the diag UI can add/remove ports
//! without restarting the agent.

use std::sync::Arc;

use hackline_core::bridge;
use hackline_proto::keyexpr;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::diag::{ActivePort, DiagState};
use crate::error::AgentError;

/// Declare the connect queryable for one port and spawn the receive
/// loop. The returned `JoinHandle` owns the queryable; aborting the
/// handle drops it and undeclares the keyexpr.
///
/// `from_config` is forwarded into the `ActivePort` so the UI can
/// distinguish startup ports (lost-on-restart-is-fine because they
/// will be re-declared from `agent.toml`) from runtime-added ports
/// (lost on restart for real).
pub async fn spawn_port_queryable(
    state: Arc<DiagState>,
    port: u16,
    from_config: bool,
) -> Result<(), AgentError> {
    if state.has_port(port) {
        return Err(AgentError::Config(format!(
            "port {port} is already being served"
        )));
    }

    let session = state.session.clone();
    let zid = state.zid_typed.clone();
    let org = state.org.clone();

    let ke = keyexpr::connect(&org, &zid, port);
    let queryable = session.declare_queryable(&ke).await?;
    info!(ke = %ke, runtime = !from_config, "queryable ready");

    let session_loop = session.clone();
    let state_loop = state.clone();
    let zid_loop = zid.clone();
    let org_loop = org.clone();
    let handle: JoinHandle<()> = tokio::spawn(async move {
        loop {
            match queryable.recv_async().await {
                Ok(query) => {
                    let s2 = session_loop.clone();
                    let z2 = zid_loop.clone();
                    let org2 = org_loop.clone();
                    let diag2 = state_loop.clone();
                    tokio::spawn(async move {
                        let selector = query.selector().to_string();
                        let res = bridge::accept_bridge(&s2, &org2, &z2, port, query).await;
                        let (rid, peer, outcome) = match &res {
                            Ok((id, peer)) => (id.to_string(), peer.clone(), "ok"),
                            Err(e) => {
                                warn!(port, "bridge error: {e}");
                                (selector, None, "error")
                            }
                        };
                        diag2.push_connection(crate::conn_event(port, rid, peer, outcome));
                    });
                }
                Err(e) => {
                    // recv_async errors when the queryable is
                    // dropped (which is how `remove_port` shuts us
                    // down); log at debug so a routine remove isn't
                    // alarming.
                    warn!(port, "queryable closed: {e}");
                    break;
                }
            }
        }
    });

    let ok = state.insert_port(port, ActivePort { handle, from_config });
    if !ok {
        // Race: another caller inserted between has_port() and
        // insert_port(). `insert_port` already aborted the handle we
        // just spawned.
        return Err(AgentError::Config(format!(
            "port {port} was added concurrently"
        )));
    }
    Ok(())
}

/// Declare the startup set of port queryables. Called once from
/// `main` after the diag state is constructed.
pub async fn start_initial_ports(
    state: Arc<DiagState>,
    ports: &[u16],
) -> Result<(), AgentError> {
    for &port in ports {
        spawn_port_queryable(state.clone(), port, true).await?;
    }
    Ok(())
}
