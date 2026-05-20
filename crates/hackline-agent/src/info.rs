//! Serves `hackline/<org>/<zid>/info` — replies to each query with a
//! serialised `AgentInfo` describing this agent's identity, version,
//! port whitelist, and uptime.

use std::sync::Arc;
use std::time::Instant;

use hackline_proto::keyexpr;
use hackline_proto::AgentInfo;
use hackline_proto::Zid;
use tracing::{debug, warn};
use zenoh::Session;

use crate::error::AgentError;

/// Spawn a queryable on `keyexpr::info(org, zid)` and run it for the
/// lifetime of the returned task. Each query gets a fresh
/// `AgentInfo` snapshot — `uptime_s` is recomputed per reply from
/// `started_at.elapsed()`.
///
/// `started_at` is captured by `main` at process start (before
/// anything that could block) so the value is monotonic and does not
/// jitter with system clock drift.
pub fn spawn(
    session: Arc<Session>,
    org: String,
    zid: Zid,
    allowed_ports: Vec<u16>,
    started_at: Instant,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = serve(session, &org, &zid, &allowed_ports, started_at).await {
            warn!("info queryable failed: {e}");
        }
    })
}

async fn serve(
    session: Arc<Session>,
    org: &str,
    zid: &Zid,
    allowed_ports: &[u16],
    started_at: Instant,
) -> Result<(), AgentError> {
    let ke = keyexpr::info(org, zid);
    let q = session.declare_queryable(&ke).await?;
    debug!(ke = %ke, "info queryable ready");

    loop {
        match q.recv_async().await {
            Ok(query) => {
                let info = AgentInfo {
                    zid: zid.to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    allowed_ports: allowed_ports.to_vec(),
                    uptime_s: started_at.elapsed().as_secs(),
                };
                // Encode failures here would mean the proto type
                // shape regressed — surface as a warn, not a panic,
                // so the agent stays up and other queryables keep
                // working.
                match serde_json::to_vec(&info) {
                    Ok(bytes) => {
                        if let Err(e) = query.reply(ke.clone(), bytes).await {
                            warn!("info reply failed: {e}");
                        }
                    }
                    Err(e) => warn!("info encode failed: {e}"),
                }
            }
            Err(e) => {
                warn!("info queryable closed: {e}");
                break;
            }
        }
    }
    Ok(())
}
