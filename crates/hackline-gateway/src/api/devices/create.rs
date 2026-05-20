//! `POST /v1/devices` — register a device by ZID.

use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use crate::auth::middleware::AuthedUser;
use crate::db::{audit, devices};
use crate::error::GatewayError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CreateDevice {
    pub zid: String,
    pub label: String,
}

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
    Json(body): Json<CreateDevice>,
) -> Result<(axum::http::StatusCode, Json<devices::Device>), GatewayError> {
    let conn = state.db.get()?;
    let org_id = caller.org_id;
    let user_id = caller.id;
    let device = tokio::task::spawn_blocking(move || {
        let device = devices::insert(&conn, org_id, &body.zid, &body.label)?;
        // SCOPE.md §7.2: `device.create` carries `zid` + `label`.
        // The new device id goes in `detail` rather than the FK
        // column — audit's FK to `devices(id)` is plain (no `ON
        // DELETE SET NULL`), so a row pointing at this id would
        // block the operator from ever deleting the device. The
        // join back is still possible via `detail.device_id`.
        let detail = serde_json::json!({
            "device_id": device.id,
            "zid": device.zid,
            "label": device.label,
        })
        .to_string();
        audit::insert(
            &conn,
            Some(user_id),
            None,
            None,
            "device.create",
            Some(&detail),
        )?;
        Ok::<_, GatewayError>(device)
    })
    .await
    .unwrap()?;
    Ok((axum::http::StatusCode::CREATED, Json(device)))
}
