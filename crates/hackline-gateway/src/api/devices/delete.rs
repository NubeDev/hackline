//! `DELETE /v1/devices/:id` — cascades to `tunnels` via the FK.

use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::auth::middleware::AuthedUser;
use crate::db::{audit, devices};
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
        // Look the device up first so the audit row carries the
        // `zid` SCOPE.md §7.2 requires — once the row is gone we
        // have no way to recover it. Also doubles as the cross-org
        // isolation check (`get_in_org` returns NotFound for both
        // "missing" and "belongs to another org").
        let device = devices::get_in_org(&conn, org_id, id)?;
        let deleted = devices::delete_in_org(&conn, org_id, id)?;
        if deleted {
            // After-delete the row is gone, so the audit FK to
            // `devices(id)` would be dangling — sqlite has
            // `PRAGMA foreign_keys = ON` (db/pool.rs) and would
            // reject the insert. Carry the id in `detail` instead.
            let detail = serde_json::json!({
                "device_id": id,
                "zid": device.zid,
            })
            .to_string();
            audit::insert(
                &conn,
                Some(user_id),
                None,
                None,
                "device.delete",
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
