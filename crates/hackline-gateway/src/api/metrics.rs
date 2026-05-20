//! `GET /metrics` — Prometheus text format per SCOPE.md §10.2. The
//! handler refreshes the two derived values (`hackline_audit_rows`,
//! `hackline_cmd_outbox_depth`) from SQLite immediately before
//! formatting so the snapshot is internally consistent.
//!
//! Admin-token gated in v0.1 via the existing `AuthedUser` extractor
//! (any valid bearer token is admitted; finer-grained role gating
//! lands when the `customer`/`viewer` enforcement matrix grows).

use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::auth::middleware::AuthedUser;
use crate::db::{audit, cmd_outbox};
use crate::error::GatewayError;
use crate::state::AppState;

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(_user): AuthedUser,
) -> Result<impl IntoResponse, GatewayError> {
    let db = state.db.clone();
    let (audit_rows, depths) = tokio::task::spawn_blocking(
        move || -> Result<(i64, std::collections::BTreeMap<String, i64>), GatewayError> {
            let conn = db.get()?;
            let rows = audit::count(&conn)?;
            let depths = cmd_outbox::pending_depth_by_device(&conn)?;
            Ok((rows, depths))
        },
    )
    .await
    .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))??;

    state.metrics.set_audit_rows(audit_rows);
    let body = state.metrics.render(&depths);

    Ok((
        StatusCode::OK,
        [(CONTENT_TYPE, "text/plain; version=0.0.4")],
        body,
    ))
}
