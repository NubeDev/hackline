//! `POST /v1/users/:id/tokens` — issue a new token for an existing
//! user. Returns the raw token once; only the hash is persisted.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::auth::middleware::AuthedUser;
use crate::auth::token;
use crate::db::users;
use crate::error::GatewayError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct MintTokenResponse {
    pub token: String,
}

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
    Path(user_id): Path<i64>,
) -> Result<(StatusCode, Json<MintTokenResponse>), GatewayError> {
    if caller.role != "owner" && caller.id != user_id {
        return Err(GatewayError::Unauthorized("not permitted".into()));
    }
    let conn = state.db.get()?;
    let org_id = caller.org_id;
    // Cross-org isolation (SCOPE.md §13 Phase 4): minting tokens for
    // a user in another org is `NotFound`, not `Unauthorized` — the
    // caller can't distinguish "no such user" from "user in another
    // org" by status code or timing.
    let raw = tokio::task::spawn_blocking(move || {
        let target = users::get(&conn, user_id)?;
        if target.org_id != org_id {
            return Err(GatewayError::NotFound);
        }
        let pair = token::generate();
        users::update_token_hash(&conn, user_id, &pair.hash)?;
        Ok::<_, GatewayError>(pair.raw)
    })
    .await
    .unwrap()?;
    Ok((StatusCode::CREATED, Json(MintTokenResponse { token: raw })))
}
