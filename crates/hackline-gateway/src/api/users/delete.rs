//! `DELETE /v1/users/:id`.

use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::auth::middleware::AuthedUser;
use crate::db::users;
use crate::error::GatewayError;
use crate::state::AppState;

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, GatewayError> {
    if caller.role != "owner" {
        return Err(GatewayError::Unauthorized(
            "only owner can delete users".into(),
        ));
    }
    if caller.id == id {
        return Err(GatewayError::BadRequest("cannot delete yourself".into()));
    }
    let conn = state.db.get()?;
    let org_id = caller.org_id;
    let deleted = tokio::task::spawn_blocking(move || users::delete_in_org(&conn, org_id, id))
        .await
        .unwrap()?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(GatewayError::NotFound)
    }
}
