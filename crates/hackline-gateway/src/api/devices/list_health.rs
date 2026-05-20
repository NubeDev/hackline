//! `GET /v1/devices/health` — collection-level health for every
//! device in the caller's org.
//!
//! Returns `{ items: DeviceHealthEntry[] }` where each entry is
//! the per-device `health` shape plus the device id used to
//! join against `GET /v1/devices`. The fan-out runs the same
//! cached probe used by `GET /v1/devices/:id/health`, so a poll
//! loop pays the per-device 250 ms probe cost once per cache
//! TTL window and reads from memory thereafter.
//!
//! Wall-clock budget: each probe is independently capped at
//! `health_probe::PROBE_TIMEOUT_MS`, and `futures::join_all`
//! runs them in parallel, so total time is ≈ one probe budget
//! regardless of org size. A failed probe yields
//! `rtt_ms: null` for that entry; it does not fail the request
//! or cancel the others.

use axum::extract::State;
use axum::Json;
use futures::future::join_all;
use serde::Serialize;

use crate::auth::middleware::AuthedUser;
use crate::db::{devices, orgs};
use crate::error::GatewayError;
use crate::state::AppState;

use super::health_probe::{cached_rtt_ms, now_unix_secs, online_from_last_seen};

#[derive(Debug, Serialize)]
pub struct DeviceHealthEntry {
    pub device_id: i64,
    pub online: bool,
    pub last_seen_at: Option<i64>,
    pub rtt_ms: Option<i64>,
}

/// Wrapped in `{ items, ... }` to match the page-envelope shape
/// other collection endpoints use, even though this one is not
/// paginated. The wrapper also lets us keep `device_id` as a
/// typed `i64` field on each entry rather than smuggling it
/// into a stringly-typed object key.
#[derive(Debug, Serialize)]
pub struct DeviceHealthList {
    pub items: Vec<DeviceHealthEntry>,
}

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
) -> Result<Json<DeviceHealthList>, GatewayError> {
    let conn = state.db.get()?;
    let org_id = caller.org_id;
    let (device_list, org) = tokio::task::spawn_blocking(move || -> Result<_, GatewayError> {
        let ds = devices::list_in_org(&conn, org_id)?;
        let o = orgs::get(&conn, org_id)?;
        Ok((ds, o))
    })
    .await
    .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))??;

    let now = now_unix_secs();
    let org_slug = org.slug;

    let probes = device_list.into_iter().map(|d| {
        let slug = org_slug.as_str();
        let state_ref = &state;
        async move {
            let online = online_from_last_seen(d.last_seen_at, now);
            let rtt_ms = cached_rtt_ms(state_ref, org_id, d.id, slug, &d.zid).await;
            DeviceHealthEntry {
                device_id: d.id,
                online,
                last_seen_at: d.last_seen_at,
                rtt_ms,
            }
        }
    });
    let items = join_all(probes).await;

    Ok(Json(DeviceHealthList { items }))
}
