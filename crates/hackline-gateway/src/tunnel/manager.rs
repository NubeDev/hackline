//! Watches the `tunnels` table and opens / closes listeners to match.
//! The single source of truth for "which listeners are live right now".

use std::sync::Arc;

use hackline_proto::Zid;
use tokio::sync::mpsc;
use tracing::{error, info};
use zenoh::Session;

use crate::db::pool::DbPool;
use crate::db::tunnels;
use crate::metrics::Metrics;
use crate::tunnel::tcp_listener::{self, TunnelTls};

/// Sent by API handlers when a tunnel is created or deleted.
#[derive(Debug)]
pub enum TunnelEvent {
    Added(tunnels::TunnelWithZid),
    Removed(i64),
}

/// Load active TCP tunnels from the DB, spawn listeners, then watch
/// for hot-reload events on `rx`.
pub async fn run(
    db: DbPool,
    session: Arc<Session>,
    metrics: Metrics,
    mut rx: mpsc::Receiver<TunnelEvent>,
    tls: TunnelTls,
) -> Result<(), crate::error::GatewayError> {
    let conn = db.get()?;
    let active = tunnels::list_active_tcp(&conn)?;
    drop(conn);

    if active.is_empty() {
        info!("no active TCP tunnels in DB");
    } else {
        info!(count = active.len(), "starting tunnel listeners from DB");
    }

    for t in active {
        spawn_listener(&session, &db, &metrics, &t, &tls);
    }

    while let Some(event) = rx.recv().await {
        match event {
            TunnelEvent::Added(t) => {
                info!(
                    id = t.id,
                    port = t.public_port,
                    "hot-starting tunnel listener"
                );
                spawn_listener(&session, &db, &metrics, &t, &tls);
            }
            TunnelEvent::Removed(id) => {
                info!(
                    id,
                    "tunnel removed (listener will close on next connection attempt)"
                );
            }
        }
    }

    Ok(())
}

fn spawn_listener(
    session: &Arc<Session>,
    db: &DbPool,
    metrics: &Metrics,
    t: &tunnels::TunnelWithZid,
    tls: &TunnelTls,
) {
    let Ok(zid) = Zid::new(&t.zid) else {
        error!(zid = %t.zid, "invalid ZID in tunnels table, skipping");
        return;
    };
    let s = session.clone();
    let db = db.clone();
    let metrics = metrics.clone();
    let tunnel_id = t.id;
    let device_id = t.device_id;
    let org_slug = t.org_slug.clone();
    let local_port = t.local_port;
    let public_port = t.public_port;
    // With `feature = "tls"`, `TunnelTls` is `Option<Arc<ArcSwap<_>>>` and
    // `.clone()` is the cheap Arc bump we want; without the feature it
    // collapses to `Option<Infallible>` (Copy) and clippy flags the call.
    // Keep the clone so both builds compile through the same call site.
    #[allow(clippy::clone_on_copy)]
    let tls = tls.clone();
    tokio::spawn(async move {
        if let Err(e) = tcp_listener::run_tcp_listener(
            s,
            db,
            metrics,
            tunnel_id,
            device_id,
            org_slug,
            zid,
            local_port,
            public_port,
            tls,
        )
        .await
        {
            error!(listen_port = public_port, "tunnel listener failed: {e}");
        }
    });
}
