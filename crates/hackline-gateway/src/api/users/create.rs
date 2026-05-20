//! `POST /v1/users` — owner mints a scoped user.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::auth::middleware::AuthedUser;
use crate::auth::token;
use crate::db::users;
use crate::error::GatewayError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CreateUser {
    pub name: String,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "operator".into()
}

#[derive(Serialize)]
pub struct CreateUserResponse {
    pub user: users::User,
    pub token: String,
}

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
    Json(body): Json<CreateUser>,
) -> Result<(StatusCode, Json<CreateUserResponse>), GatewayError> {
    if caller.role != "owner" {
        return Err(GatewayError::Unauthorized(
            "only owner can create users".into(),
        ));
    }
    let conn = state.db.get()?;
    let org_id = caller.org_id;
    let (user, raw_token) = tokio::task::spawn_blocking(move || {
        let pair = token::generate();
        let u = users::insert(&conn, org_id, &body.name, &body.role, &pair.hash)?;
        Ok::<_, GatewayError>((u, pair.raw))
    })
    .await
    .unwrap()?;
    Ok((
        StatusCode::CREATED,
        Json(CreateUserResponse {
            user,
            token: raw_token,
        }),
    ))
}
