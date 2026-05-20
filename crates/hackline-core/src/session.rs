//! Zenoh-session open / close helpers. Centralised so that the
//! gateway and the agent use the same shutdown semantics.

use zenoh::Session;

/// Open a Zenoh session with the given config. Both the agent and the
/// gateway call this so connection/reconnect behaviour is uniform.
pub async fn open(config: zenoh::Config) -> Result<Session, crate::error::BridgeError> {
    let session = zenoh::open(config)
        .await
        .map_err(crate::error::BridgeError::Zenoh)?;
    Ok(session)
}
