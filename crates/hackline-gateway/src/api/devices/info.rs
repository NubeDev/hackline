//! `GET /v1/devices/:id/info` — issues a Zenoh query against
//! `hackline/<org>/<zid>/info` and returns the agent's `AgentInfo`
//! (zid, version, allowed_ports, uptime_s).
//!
//! Failure mapping mirrors `api_call::call.rs`:
//! - `504 Gateway Timeout` if the agent doesn't reply within
//!   `INFO_QUERY_TIMEOUT_MS`.
//! - `503 Service Unavailable` if the query returns no replies
//!   (no agent listens on the keyexpr).
//! - `502 Bad Gateway` if the reply doesn't deserialise into
//!   `AgentInfo` (wire-shape drift on the device).

use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use hackline_proto::{keyexpr, AgentInfo, Zid};

use crate::auth::middleware::AuthedUser;
use crate::db::{devices, orgs};
use crate::error::GatewayError;
use crate::state::AppState;

/// Hard cap on the query. The agent generates the reply
/// synchronously from in-memory state (no I/O), so a healthy mesh
/// resolves in single-digit ms; 1 s is enough slack for a slow path
/// while keeping the HTTP request snappy. Larger than the
/// `health_probe::PROBE_TIMEOUT_MS` (250 ms) because operators may
/// want the version string from a sluggish agent even when the
/// liveliness probe would have given up.
const INFO_QUERY_TIMEOUT_MS: u64 = 1_000;

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
    Path(id): Path<i64>,
) -> Result<Response, GatewayError> {
    let conn = state.db.get()?;
    let org_id = caller.org_id;
    let (device, org) = tokio::task::spawn_blocking(move || -> Result<_, GatewayError> {
        let d = devices::get_in_org(&conn, org_id, id)?;
        let o = orgs::get(&conn, org_id)?;
        Ok((d, o))
    })
    .await
    .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))??;

    let zid = Zid::new(&device.zid).map_err(|e| GatewayError::BadRequest(e.to_string()))?;
    let ke = keyexpr::info(&org.slug, &zid);

    let replies = state
        .zenoh
        .get(&ke)
        .timeout(Duration::from_millis(INFO_QUERY_TIMEOUT_MS))
        .await
        .map_err(GatewayError::Zenoh)?;

    // Belt-and-braces wrap mirroring health_probe.rs: guards against
    // a future zenoh release that holds the channel open past its
    // declared timeout.
    let reply = match tokio::time::timeout(
        Duration::from_millis(INFO_QUERY_TIMEOUT_MS + 250),
        replies.recv_async(),
    )
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(_)) => {
            return Ok((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "agent_unreachable" })),
            )
                .into_response());
        }
        Err(_) => {
            return Ok((
                StatusCode::GATEWAY_TIMEOUT,
                Json(serde_json::json!({ "error": "agent_timeout" })),
            )
                .into_response());
        }
    };

    let bytes = reply
        .result()
        .map_err(|e| GatewayError::BadRequest(format!("agent error: {e:?}")))?
        .payload()
        .to_bytes()
        .to_vec();

    let info: AgentInfo = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            return Ok((
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("agent reply decode: {e}") })),
            )
                .into_response());
        }
    };

    Ok(Json(info).into_response())
}
