//! `GET /v1/claim/status` — `{ claimed, can_claim }`.

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::db::claim;
use crate::error::GatewayError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct ClaimStatus {
    pub claimed: bool,
    pub can_claim: bool,
}

pub async fn handler(State(state): State<AppState>) -> Result<Json<ClaimStatus>, GatewayError> {
    let conn = state.db.get()?;
    let status = tokio::task::spawn_blocking(move || {
        let claimed = claim::is_claimed(&conn)?;
        let can_claim = claim::is_pending(&conn)?;
        Ok::<_, GatewayError>(ClaimStatus { claimed, can_claim })
    })
    .await
    .unwrap()?;
    Ok(Json(status))
}
