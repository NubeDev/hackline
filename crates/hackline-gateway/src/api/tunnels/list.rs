//! `GET /v1/tunnels`.

use axum::extract::State;
use axum::Json;

use crate::auth::middleware::AuthedUser;
use crate::db::tunnels;
use crate::error::GatewayError;
use crate::state::AppState;

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
) -> Result<Json<Vec<tunnels::Tunnel>>, GatewayError> {
    let conn = state.db.get()?;
    let org_id = caller.org_id;
    let list = tokio::task::spawn_blocking(move || tunnels::list_in_org(&conn, org_id))
        .await
        .unwrap()?;
    Ok(Json(list))
}
