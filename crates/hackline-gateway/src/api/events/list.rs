//! `GET /v1/events` — cursor-paginated query over the persisted
//! event ring. Read-only, returns newest-first.

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::auth::middleware::AuthedUser;
use crate::db::events::{self, EventRow};
use crate::error::GatewayError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    pub device: Option<i64>,
    pub topic: Option<String>,
    /// Unix milliseconds. Returns rows with `ts >= since`.
    pub since: Option<i64>,
    /// Opaque cursor — the `id` of the last row from the previous page.
    pub cursor: Option<i64>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    100
}

#[derive(Debug, Serialize)]
pub struct EventsPage {
    pub items: Vec<EventRow>,
    pub next_cursor: Option<i64>,
}

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
    Query(q): Query<EventsQuery>,
) -> Result<Json<EventsPage>, GatewayError> {
    let conn = state.db.get()?;
    let limit = q.limit;
    let org_id = caller.org_id;
    let items = tokio::task::spawn_blocking(move || {
        events::list(
            &conn,
            org_id,
            q.device,
            q.topic.as_deref(),
            q.since,
            q.cursor,
            limit,
        )
    })
    .await
    .unwrap()?;

    // Saturated page → emit the last id as the next cursor. If we
    // got fewer rows than requested, there is no more history.
    let next_cursor = if items.len() as i64 >= limit {
        items.last().map(|r| r.id)
    } else {
        None
    };
    Ok(Json(EventsPage { items, next_cursor }))
}
