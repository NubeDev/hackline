//! `GET /v1/devices/:id/health` — liveliness summary derived from
//! the device's `last_seen_at` column plus a synchronous Zenoh
//! `liveliness::Get` probe for `rtt_ms`.
//!
//! Shape matches `DOCS/openapi.yaml` `DeviceHealth`:
//! `{ online: bool, last_seen_at: int|null, rtt_ms: int|null }`.
//! `online` flips false when no liveliness sample has arrived
//! within the window in `health_probe::ONLINE_STALE_SECS` (the
//! background fan-in in `liveliness.rs` is the writer). `rtt_ms`
//! is the measured round-trip of a single liveliness query
//! against the device's own `hackline/<org>/<zid>/health` token;
//! null if the probe times out, errors, or returns no replies.
//!
//! The probe result is cached in `state.rtt_cache` for a short
//! TTL so a burst of polls (admin UI rendering many cards at
//! once, or React strict-mode double-effects in dev) collapses
//! into one Zenoh query per device per window. The cache and
//! the freshness/probe helpers are shared with the collection-
//! level `GET /v1/devices/health` handler in `list_health.rs`.

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

use crate::auth::middleware::AuthedUser;
use crate::db::{devices, orgs};
use crate::error::GatewayError;
use crate::state::AppState;

use super::health_probe::{cached_rtt_ms, now_unix_secs, online_from_last_seen};

#[derive(Debug, Serialize)]
pub struct DeviceHealth {
    pub online: bool,
    pub last_seen_at: Option<i64>,
    pub rtt_ms: Option<i64>,
}

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
    Path(id): Path<i64>,
) -> Result<Json<DeviceHealth>, GatewayError> {
    let conn = state.db.get()?;
    let org_id = caller.org_id;
    let (device, org) = tokio::task::spawn_blocking(move || -> Result<_, GatewayError> {
        let d = devices::get_in_org(&conn, org_id, id)?;
        let o = orgs::get(&conn, org_id)?;
        Ok((d, o))
    })
    .await
    .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))??;

    let online = online_from_last_seen(device.last_seen_at, now_unix_secs());
    let rtt_ms = cached_rtt_ms(&state, org_id, device.id, &org.slug, &device.zid).await;

    Ok(Json(DeviceHealth {
        online,
        last_seen_at: device.last_seen_at,
        rtt_ms,
    }))
}
