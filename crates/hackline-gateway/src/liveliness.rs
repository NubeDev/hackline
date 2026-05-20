//! Gateway-side liveliness fan-in. Subscribes to
//! `hackline/*/*/health` so every agent that declares its
//! `keyexpr::health` token is observed. `Put` samples upsert the
//! device row (creating it if absent) and bump `last_seen_at`;
//! `Delete` samples (token retracted, agent process gone) clear
//! `last_seen_at` so the admin UI flips the device to offline.
//!
//! Samples for orgs the gateway has never seen are dropped — the
//! org row is created via the claim flow, not on the wire, so an
//! arbitrary device cannot manifest a tenant.

use std::sync::Arc;

use hackline_proto::keyexpr;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use zenoh::sample::SampleKind;
use zenoh::Session;

use crate::db::{devices, orgs, pool::DbPool};
use crate::error::GatewayError;

/// Spawn the liveliness watcher. Returns the join handle so
/// `serve.rs` can drop it on shutdown; the task itself loops until
/// the subscriber closes.
pub async fn spawn(session: Arc<Session>, db: DbPool) -> Result<JoinHandle<()>, GatewayError> {
    let sub = session
        .liveliness()
        .declare_subscriber(keyexpr::HEALTH_FANIN)
        .await
        .map_err(GatewayError::Zenoh)?;
    info!(
        ke = keyexpr::HEALTH_FANIN,
        "liveliness fan-in subscriber ready"
    );

    let handle = tokio::spawn(async move {
        loop {
            match sub.recv_async().await {
                Ok(sample) => {
                    let ke = sample.key_expr().as_str().to_owned();
                    let kind = sample.kind();
                    if let Err(e) = handle_sample(&db, &ke, kind).await {
                        warn!(ke = %ke, "liveliness drop: {e}");
                    }
                }
                Err(e) => {
                    warn!("liveliness subscriber closed: {e}");
                    break;
                }
            }
        }
    });
    Ok(handle)
}

async fn handle_sample(db: &DbPool, ke: &str, kind: SampleKind) -> Result<(), GatewayError> {
    let (org_slug, zid) = keyexpr::parse_health_keyexpr(ke)
        .ok_or_else(|| GatewayError::BadRequest(format!("unparsable liveliness ke: {ke}")))?;
    let zid_str = zid.as_str().to_owned();
    let db = db.clone();

    tokio::task::spawn_blocking(move || -> Result<(), GatewayError> {
        let conn = db.get()?;
        let org = orgs::get_by_slug(&conn, &org_slug)?
            .ok_or_else(|| GatewayError::BadRequest(format!("unknown org: {org_slug}")))?;
        match kind {
            SampleKind::Put => {
                let (id, created) = devices::upsert_seen(&conn, org.id, &zid_str)?;
                if created {
                    info!(zid = %zid_str, org = %org_slug, device_id = id, "device registered via liveliness");
                } else {
                    debug!(zid = %zid_str, org = %org_slug, device_id = id, "device liveness");
                }
            }
            SampleKind::Delete => {
                devices::mark_offline(&conn, org.id, &zid_str)?;
                info!(zid = %zid_str, org = %org_slug, "device offline (liveliness retracted)");
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| GatewayError::BadRequest(format!("join: {e}")))??;
    Ok(())
}
