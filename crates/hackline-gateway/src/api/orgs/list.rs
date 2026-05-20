//! `GET /v1/orgs` — owner-only: every org on the gateway. Non-owner
//! callers see exactly one row (their own); we surface that via
//! `GET /v1/orgs/me` instead of mutating the response shape here.

use axum::extract::State;
use axum::Json;

use crate::auth::middleware::AuthedUser;
use crate::db::orgs::{self, Org};
use crate::error::GatewayError;
use crate::state::AppState;

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
) -> Result<Json<Vec<Org>>, GatewayError> {
    if caller.role != "owner" {
        return Err(GatewayError::Unauthorized(
            "only owner can list every org".into(),
        ));
    }
    let conn = state.db.get()?;
    let list = tokio::task::spawn_blocking(move || orgs::list(&conn))
        .await
        .unwrap()?;
    Ok(Json(list))
}
