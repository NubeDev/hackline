//! `POST /v1/devices/:id/api/:topic` — one synchronous Zenoh `get`,
//! returning the device's first reply or a timeout/unreachable
//! error.

use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use hackline_proto::keyexpr;
use hackline_proto::msg::{ApiReply, ApiRequest};
use hackline_proto::Zid;
use serde::{Deserialize, Serialize};
use zenoh::bytes::ZBytes;

use crate::auth::middleware::AuthedUser;
use crate::auth::scope;
use crate::db::audit;
use crate::db::devices;
use crate::error::GatewayError;
use crate::metrics::Outcome;
use crate::state::AppState;

const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const MAX_TIMEOUT_MS: u64 = 60_000;

#[derive(Deserialize)]
pub struct ApiCallBody {
    pub payload: serde_json::Value,
    pub timeout_ms: Option<u64>,
    pub content_type: Option<String>,
}

#[derive(Serialize)]
pub struct ApiCallResponse {
    pub content_type: String,
    pub reply: serde_json::Value,
}

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Path((device_id, topic)): Path<(i64, String)>,
    Json(body): Json<ApiCallBody>,
) -> Result<Response, GatewayError> {
    scope::check_device(&user, device_id)?;
    if topic.is_empty() {
        return Err(GatewayError::BadRequest("topic must not be empty".into()));
    }

    let db = state.db.clone();
    let org_id = user.org_id;
    let (device, org_slug) = tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        // Re-uses the org-scoped get for the 404 path, then resolves
        // the slug. Two lookups are one round-trip on a hot
        // connection and keep the org-isolation check explicit.
        devices::get_in_org(&conn, org_id, device_id)?;
        devices::get_with_org_slug(&conn, device_id)
    })
    .await
    .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))??;

    let zid = Zid::new(&device.zid).map_err(|e| GatewayError::BadRequest(e.to_string()))?;
    let ke = keyexpr::msg_api(&org_slug, &zid, &topic);

    let req = ApiRequest {
        content_type: body
            .content_type
            .unwrap_or_else(|| "application/json".into()),
        payload: body.payload,
    };
    let bytes = serde_json::to_vec(&req)
        .map_err(|e| GatewayError::BadRequest(format!("request encode: {e}")))?;

    let timeout = body
        .timeout_ms
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .min(MAX_TIMEOUT_MS);

    let replies = state
        .zenoh
        .get(&ke)
        .payload(ZBytes::from(bytes))
        .timeout(Duration::from_millis(timeout))
        .await
        .map_err(GatewayError::Zenoh)?;

    let reply = match tokio::time::timeout(
        Duration::from_millis(timeout + 500),
        replies.recv_async(),
    )
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(_)) => {
            audit_api_call(&state, user.id, device_id, &topic, "device_unreachable").await;
            state.metrics.inc_api_call(&topic, Outcome::Error);
            return Ok((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "device_unreachable" })),
            )
                .into_response());
        }
        Err(_) => {
            audit_api_call(&state, user.id, device_id, &topic, "device_timeout").await;
            state.metrics.inc_api_call(&topic, Outcome::Error);
            return Ok((
                StatusCode::GATEWAY_TIMEOUT,
                Json(serde_json::json!({ "error": "device_timeout" })),
            )
                .into_response());
        }
    };

    let sample_bytes = reply
        .result()
        .map_err(|e| GatewayError::BadRequest(format!("device error: {e:?}")))?
        .payload()
        .to_bytes()
        .to_vec();
    let api_reply: ApiReply = serde_json::from_slice(&sample_bytes)
        .map_err(|e| GatewayError::BadRequest(format!("reply decode: {e}")))?;

    audit_api_call(&state, user.id, device_id, &topic, "ok").await;
    state.metrics.inc_api_call(&topic, Outcome::Ok);

    Ok(Json(ApiCallResponse {
        content_type: api_reply.content_type,
        reply: api_reply.payload,
    })
    .into_response())
}

/// Emit one `api.call` audit row with the SCOPE.md §7.2 detail keys
/// (`topic`, `outcome`). Best-effort — a failed audit insert must not
/// hide the underlying RPC result from the operator.
async fn audit_api_call(
    state: &AppState,
    user_id: i64,
    device_id: i64,
    topic: &str,
    outcome: &str,
) {
    let db = state.db.clone();
    let detail = serde_json::json!({ "topic": topic, "outcome": outcome }).to_string();
    let _ = tokio::task::spawn_blocking(move || -> Result<(), GatewayError> {
        let conn = db.get()?;
        audit::insert(
            &conn,
            Some(user_id),
            Some(device_id),
            None,
            "api.call",
            Some(&detail),
        )
    })
    .await;
}
