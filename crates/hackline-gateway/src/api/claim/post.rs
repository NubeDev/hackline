//! `POST /v1/claim` — atomic consume-pending + insert-owner.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::db::{claim, orgs};
use crate::error::GatewayError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ClaimRequest {
    pub token: String,
    #[serde(default = "default_owner_name")]
    pub name: String,
    /// Optional org slug. SCOPE.md §13 Phase 4: the claim flow
    /// allocates this org if absent; if omitted the owner lands in
    /// the seeded `default` org so single-tenant deployments work
    /// without ceremony.
    #[serde(default)]
    pub org: Option<String>,
}

fn default_owner_name() -> String {
    "owner".into()
}

#[derive(Serialize)]
pub struct ClaimResponse {
    pub user_id: i64,
    pub token: String,
    /// Slug of the org the owner was stamped into. Echoed so the CLI
    /// can cache it locally for display; the server still enforces
    /// isolation off the bearer token.
    pub org: String,
}

pub async fn handler(
    State(state): State<AppState>,
    Json(body): Json<ClaimRequest>,
) -> Result<(StatusCode, Json<ClaimResponse>), GatewayError> {
    let conn = state.db.get()?;
    let (bearer_raw, user_id, org_slug) = tokio::task::spawn_blocking(move || {
        let bearer = claim::consume(&conn, &body.token, &body.name, body.org.as_deref())?;
        let user = crate::db::users::list(&conn)?
            .into_iter()
            .find(|u| u.role == "owner")
            .ok_or_else(|| GatewayError::BadRequest("claim failed".into()))?;
        let org = orgs::get(&conn, user.org_id)?;
        Ok::<_, GatewayError>((bearer.raw, user.id, org.slug))
    })
    .await
    .unwrap()?;

    Ok((
        StatusCode::CREATED,
        Json(ClaimResponse {
            user_id,
            token: bearer_raw,
            org: org_slug,
        }),
    ))
}
