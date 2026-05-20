//! `GET /v1/orgs/me` — the caller's own org. SCOPE.md §13 Phase 4:
//! this is the only org-shaped lookup non-owner callers can perform.

use axum::extract::State;
use axum::Json;

use crate::auth::middleware::AuthedUser;
use crate::db::orgs::{self, Org};
use crate::error::GatewayError;
use crate::state::AppState;

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
) -> Result<Json<Org>, GatewayError> {
    let conn = state.db.get()?;
    let org_id = caller.org_id;
    let org = tokio::task::spawn_blocking(move || orgs::get(&conn, org_id))
        .await
        .unwrap()?;
    Ok(Json(org))
}
