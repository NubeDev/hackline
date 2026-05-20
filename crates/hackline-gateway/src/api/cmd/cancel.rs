//! `DELETE /v1/cmd/:cmd_id` — cancel a queued (not yet delivered)
//! command. Returns 404 if the row was already delivered or never
//! existed; cancel is best-effort, mirroring SCOPE.md §5.3.

use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::auth::middleware::AuthedUser;
use crate::auth::scope;
use crate::db::audit;
use crate::db::cmd_outbox;
use crate::db::devices;
use crate::error::GatewayError;
use crate::state::AppState;

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(user): AuthedUser,
    Path(cmd_id): Path<String>,
) -> Result<StatusCode, GatewayError> {
    let db = state.db.clone();
    let cmd_id_lookup = cmd_id.clone();
    let row = tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        cmd_outbox::get_by_cmd_id(&conn, &cmd_id_lookup)
    })
    .await
    .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))??;
    let row = row.ok_or(GatewayError::NotFound)?;
    scope::check_device(&user, row.device_id)?;
    // Cross-org isolation (SCOPE.md §13 Phase 4): the cmd id is a
    // bare UUID with no org in its path. The cmd_outbox row points
    // at a device, and devices belong to exactly one org; if that
    // org isn't the caller's, treat the row as if it doesn't exist.
    {
        let db = state.db.clone();
        let org_id = user.org_id;
        let device_id = row.device_id;
        tokio::task::spawn_blocking(move || {
            let conn = db.get()?;
            devices::get_in_org(&conn, org_id, device_id)
        })
        .await
        .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))??;
    }

    let db = state.db.clone();
    let cmd_id_for_cancel = cmd_id.clone();
    let deleted = tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        cmd_outbox::cancel(&conn, &cmd_id_for_cancel)
    })
    .await
    .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))??;

    if deleted {
        let db = state.db.clone();
        let user_id = user.id;
        let device_id = row.device_id;
        let detail = serde_json::json!({ "cmd_id": cmd_id }).to_string();
        let _ = tokio::task::spawn_blocking(move || -> Result<(), GatewayError> {
            let conn = db.get()?;
            audit::insert(
                &conn,
                Some(user_id),
                Some(device_id),
                None,
                "cmd.cancel",
                Some(&detail),
            )
        })
        .await;
        state.metrics.inc_cmd("cancelled");
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(GatewayError::NotFound)
    }
}
