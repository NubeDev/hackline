//! Handles `hackline/<zid>/tcp/<port>/connect` queries. Validates the
//! requested port against the agent's whitelist before opening a
//! loopback TCP connection and handing off to `hackline-core::bridge`.

use std::sync::Arc;

use hackline_core::bridge;
use hackline_proto::keyexpr;
use hackline_proto::Zid;
use tracing::{info, warn};
use zenoh::Session;

use crate::diag::DiagState;
use crate::error::AgentError;

/// Run one queryable per allowed port. Blocks until all queryables close.
pub async fn serve_connect(
    session: Arc<Session>,
    org: &str,
    zid: &Zid,
    allowed_ports: &[u16],
    diag: Arc<DiagState>,
) -> Result<(), AgentError> {
    let mut handles = Vec::with_capacity(allowed_ports.len());

    for &port in allowed_ports {
        let ke = keyexpr::connect(org, zid, port);
        let q = session.declare_queryable(&ke).await?;
        info!(ke = %ke, "queryable ready");

        let s = session.clone();
        let z = zid.clone();
        let org = org.to_owned();
        let diag = diag.clone();
        handles.push(tokio::spawn(async move {
            loop {
                match q.recv_async().await {
                    Ok(query) => {
                        let s2 = s.clone();
                        let z2 = z.clone();
                        let org2 = org.clone();
                        let diag2 = diag.clone();
                        tokio::spawn(async move {
                            // `accept_bridge` parses the request_id
                            // and peer out of the connect query
                            // payload; we attribute the diag entry
                            // off those so each accepted stream
                            // shows up as its own row (matching the
                            // gateway-side `request_id` audit).
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
                        warn!(port, "queryable closed: {e}");
                        break;
                    }
                }
            }
        }));
    }

    for h in handles {
        let _ = h.await;
    }
    Ok(())
}
