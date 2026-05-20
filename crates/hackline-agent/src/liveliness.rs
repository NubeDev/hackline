//! Declares the `hackline/<org>/<zid>/health` Zenoh liveliness
//! token. Holding the returned token keeps the agent visible to any
//! gateway watching `hackline/*/*/health`; dropping it (process
//! exit, lost session) propagates a `Delete` sample so the gateway
//! marks the device offline without waiting on a heartbeat timeout.

use hackline_proto::keyexpr;
use hackline_proto::Zid;
use zenoh::liveliness::LivelinessToken;
use zenoh::Session;

use crate::error::AgentError;

/// Declare the agent's liveliness token. The caller must hold the
/// returned `LivelinessToken` for the lifetime of the agent — Zenoh
/// retracts the token (and notifies subscribers with a `Delete`)
/// when the value is dropped.
pub async fn declare(
    session: &Session,
    org: &str,
    zid: &Zid,
) -> Result<LivelinessToken, AgentError> {
    let ke = keyexpr::health(org, zid);
    let token = session.liveliness().declare_token(ke).await?;
    Ok(token)
}
