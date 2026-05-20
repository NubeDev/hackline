//! `DELETE /v1/tunnels/:id` — closes the listener and deletes the row.

use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::auth::middleware::AuthedUser;
use crate::db::{audit, tunnels};
use crate::error::GatewayError;
use crate::state::AppState;

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, GatewayError> {
    let conn = state.db.get()?;
    let org_id = caller.org_id;
    let user_id = caller.id;
    let deleted = tokio::task::spawn_blocking(move || {
        let deleted = tunnels::delete_in_org(&conn, org_id, id)?;
        if deleted {
            // After-delete the row is gone; FK on audit.tunnel_id
            // would dangle (PRAGMA foreign_keys = ON). Carry the
            // id in `detail` instead so the join back is still
            // possible without the FK.
            let detail = serde_json::json!({ "tunnel_id": id }).to_string();
            audit::insert(
                &conn,
                Some(user_id),
                None,
                None,
                "tunnel.delete",
                Some(&detail),
            )?;
        }
        Ok::<_, GatewayError>(deleted)
    })
    .await
    .unwrap()?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(GatewayError::NotFound)
    }
}
