//! `GET /v1/devices/:id`.

use axum::extract::{Path, State};
use axum::Json;

use crate::auth::middleware::AuthedUser;
use crate::db::devices;
use crate::error::GatewayError;
use crate::state::AppState;

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
    Path(id): Path<i64>,
) -> Result<Json<devices::Device>, GatewayError> {
    let conn = state.db.get()?;
    let org_id = caller.org_id;
    let device = tokio::task::spawn_blocking(move || devices::get_in_org(&conn, org_id, id))
        .await
        .unwrap()?;
    Ok(Json(device))
}
