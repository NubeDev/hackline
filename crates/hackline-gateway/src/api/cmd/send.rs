//! `POST /v1/devices/:id/cmd/:topic` — enqueue a durable command.
//! Always succeeds (subject to the per-device cap and the 64 KiB
//! payload cap); the gateway's cmd-delivery loop fires the actual
//! Zenoh publish asynchronously. SCOPE.md §5.3.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::middleware::AuthedUser;
use crate::auth::scope;
use crate::db::audit;
use crate::db::cmd_outbox::{self, CMD_DEFAULT_TTL_MS};
use crate::db::devices;
use crate::error::GatewayError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SendCmd {
    pub payload: serde_json::Value,
    /// TTL in milliseconds. Defaults to 7 days (SCOPE.md §7.3).
    pub expires_in_ms: Option<i64>,
    pub content_type: Option<String>,
}

#[derive(Serialize)]
pub struct SendCmdResponse {
    pub cmd_id: String,
}

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Path((device_id, topic)): Path<(i64, String)>,
    Json(body): Json<SendCmd>,
) -> Result<(StatusCode, Json<SendCmdResponse>), GatewayError> {
    scope::check_device(&user, device_id)?;
    // Cross-org isolation (SCOPE.md §13 Phase 4) — done before any
    // write so we don't allocate a cmd row pointing at someone
    // else's device.
    {
        let db = state.db.clone();
        let org_id = user.org_id;
        tokio::task::spawn_blocking(move || {
            let conn = db.get()?;
            devices::get_in_org(&conn, org_id, device_id)
        })
        .await
        .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))??;
    }

    if topic.is_empty() {
        return Err(GatewayError::BadRequest("topic must not be empty".into()));
    }

    let cmd_id = Uuid::new_v4();
    let cmd_id_str = cmd_id.to_string();
    let now = now_ms();
    let ttl = body.expires_in_ms.unwrap_or(CMD_DEFAULT_TTL_MS).max(1);
    let expires_at = now.saturating_add(ttl);
    let content_type = body
        .content_type
        .unwrap_or_else(|| "application/json".into());

    let db = state.db.clone();
    let cmd_id_for_blocking = cmd_id_str.clone();
    let topic_clone = topic.clone();
    let payload = body.payload;
    tokio::task::spawn_blocking(move || {
        let mut conn = db.get()?;
        cmd_outbox::enqueue(
            &mut conn,
            &cmd_id_for_blocking,
            device_id,
            &topic_clone,
            &content_type,
            &payload,
            now,
            expires_at,
        )
    })
    .await
    .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))??;

    // `cmd.send` (SCOPE.md §7.2): record `cmd_id` + `topic` on the
    // operator's behalf so the audit log can answer "who issued
    // which command, when, against which device".
    {
        let db = state.db.clone();
        let user_id = user.id;
        let detail = serde_json::json!({ "cmd_id": cmd_id_str, "topic": topic }).to_string();
        let _ = tokio::task::spawn_blocking(move || -> Result<(), GatewayError> {
            let conn = db.get()?;
            audit::insert(
                &conn,
                Some(user_id),
                Some(device_id),
                None,
                "cmd.send",
                Some(&detail),
            )
        })
        .await;
    }
    state.metrics.inc_cmd("accepted");

    state.cmd_notifier.notify();

    Ok((
        StatusCode::ACCEPTED,
        Json(SendCmdResponse { cmd_id: cmd_id_str }),
    ))
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
