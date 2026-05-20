//! `POST /v1/orgs` — owner-only: allocate a fresh org. The owner
//! stays in their original org; cross-org user provisioning is a
//! follow-on step (mint a token for a user pinned to the new org).

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::auth::middleware::AuthedUser;
use crate::db::orgs::{self, Org};
use crate::error::GatewayError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CreateOrg {
    pub slug: String,
    #[serde(default)]
    pub name: Option<String>,
}

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
    Json(body): Json<CreateOrg>,
) -> Result<(StatusCode, Json<Org>), GatewayError> {
    if caller.role != "owner" {
        return Err(GatewayError::Unauthorized(
            "only owner can create orgs".into(),
        ));
    }
    let conn = state.db.get()?;
    let slug = body.slug;
    let name = body.name.clone().unwrap_or_else(|| slug.clone());
    let org = tokio::task::spawn_blocking(move || orgs::insert(&conn, &slug, &name))
        .await
        .unwrap()?;
    Ok((StatusCode::CREATED, Json(org)))
}
