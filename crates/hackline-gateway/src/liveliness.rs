//! Gateway-side liveliness fan-in. Subscribes to
//! `hackline/*/*/health` so every agent that declares its
//! `keyexpr::health` token is observed. `Put` samples upsert the
//! device row (creating it if absent) and bump `last_seen_at`;
//! `Delete` samples (token retracted, agent process gone) clear
//! `last_seen_at` so the admin UI flips the device to offline.
//!
//! A second task periodically issues `liveliness().get(...)` against
//! the same fan-in keyexpr and stamps `last_seen_at` for every
//! responder. Zenoh liveliness is presence-based — a token only
//! generates one `Put` when it's declared, so without this poll a
//! long-held token would drift past `ONLINE_STALE_SECS` even though
//! the agent is alive. The poll period sits below that window with
//! one beat of slack.
//!
//! Samples for orgs the gateway has never seen are dropped — the
//! org row is created via the claim flow, not on the wire, so an
//! arbitrary device cannot manifest a tenant.

use std::sync::Arc;
use std::time::Duration;

use hackline_proto::keyexpr;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use zenoh::sample::SampleKind;
use zenoh::Session;

use crate::db::{devices, orgs, pool::DbPool};
use crate::error::GatewayError;

/// How often the keepalive poll fires. Must stay strictly below
/// `api::devices::health_probe::ONLINE_STALE_SECS` (60 s) so a
/// healthy device never flips offline between polls. 20 s gives
/// two refreshes per window — one missed beat still keeps the row
/// fresh.
const KEEPALIVE_POLL_SECS: u64 = 20;

/// Hard cap on one keepalive sweep. The poll fans replies in over
/// the wire; bounding the wait keeps the task tick predictable even
/// if a peer is slow to answer.
const KEEPALIVE_POLL_TIMEOUT_MS: u64 = 2_000;

/// Spawn the liveliness watcher and the keepalive poller. Returns
/// the join handle of the watcher so `serve.rs` can drop it on
/// shutdown; the poller is detached (idempotent stamps, nothing to
/// wait on at shutdown).
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

    let db_poll = db.clone();
    let session_poll = session.clone();
    tokio::spawn(async move {
        keepalive_loop(session_poll, db_poll).await;
    });

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

async fn keepalive_loop(session: Arc<Session>, db: DbPool) {
    let mut tick = tokio::time::interval(Duration::from_secs(KEEPALIVE_POLL_SECS));
    // Skip the immediate first tick: the subscriber already stamps
    // on declare, so the first poll is useful one period in.
    tick.tick().await;
    loop {
        tick.tick().await;
        if let Err(e) = keepalive_sweep(&session, &db).await {
            warn!("liveliness keepalive sweep failed: {e}");
        }
    }
}

async fn keepalive_sweep(session: &Session, db: &DbPool) -> Result<(), GatewayError> {
    let replies = session
        .liveliness()
        .get(keyexpr::HEALTH_FANIN)
        .timeout(Duration::from_millis(KEEPALIVE_POLL_TIMEOUT_MS))
        .await
        .map_err(GatewayError::Zenoh)?;
    while let Ok(reply) = replies.recv_async().await {
        let Ok(sample) = reply.result() else { continue };
        let ke = sample.key_expr().as_str().to_owned();
        // Reuse the same Put path as the subscriber so a device the
        // gateway has never seen still gets created on first poll
        // (e.g. gateway started after the agent).
        if let Err(e) = handle_sample(db, &ke, SampleKind::Put).await {
            debug!(ke = %ke, "liveliness keepalive drop: {e}");
        }
    }
    Ok(())
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
