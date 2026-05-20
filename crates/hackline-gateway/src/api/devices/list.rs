//! `GET /v1/devices` — list devices visible to the caller's scope.

use axum::extract::State;
use axum::Json;

use crate::auth::middleware::AuthedUser;
use crate::db::devices;
use crate::error::GatewayError;
use crate::state::AppState;

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
) -> Result<Json<Vec<devices::Device>>, GatewayError> {
    let conn = state.db.get()?;
    let org_id = caller.org_id;
    let list = tokio::task::spawn_blocking(move || devices::list_in_org(&conn, org_id))
        .await
        .unwrap()?;
    Ok(Json(list))
}
