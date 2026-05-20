//! Gateway-side fan-in: one Zenoh subscriber per message-plane
//! keyexpr family (`hackline/*/msg/event/**`, `hackline/*/msg/log/**`).
//! Each delivery is parsed back into `(zid, kind, dotted-topic)`,
//! the device row is looked up, the envelope is persisted into
//! `events` / `logs`, and the resulting row is published on the
//! in-process `MsgBus` so SSE subscribers see it live.
//!
//! Messages from unknown ZIDs (no row in `devices`) and malformed
//! envelopes are logged and dropped — the gateway is a fan-in, not a
//! validator of every device's wire conformance.

use std::sync::Arc;

use hackline_proto::keyexpr::{self, MsgKind};
use hackline_proto::msg::MsgEnvelope;
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use zenoh::Session;

use crate::db::events;
use crate::db::logs;
use crate::db::pool::DbPool;
use crate::error::GatewayError;
use crate::events_bus::{MsgBus, MsgEvent};
use crate::metrics::Metrics;

/// Declare the two wildcard subscribers and spawn one task per
/// subscriber. Returns the join handles so `serve.rs` can include
/// them in its `select!`; tasks only return on subscriber close.
pub async fn spawn(
    session: Arc<Session>,
    db: DbPool,
    bus: MsgBus,
    metrics: Metrics,
) -> Result<Vec<JoinHandle<()>>, GatewayError> {
    let mut handles = Vec::with_capacity(2);

    for (ke, kind) in [
        (keyexpr::MSG_EVENT_FANIN, MsgKind::Event),
        (keyexpr::MSG_LOG_FANIN, MsgKind::Log),
    ] {
        let sub = session
            .declare_subscriber(ke.to_owned())
            .await
            .map_err(GatewayError::Zenoh)?;
        tracing::info!(ke = ke, "message-plane fan-in subscriber ready");

        let db = db.clone();
        let bus = bus.clone();
        let metrics = metrics.clone();
        let handle = tokio::spawn(async move {
            loop {
                match sub.recv_async().await {
                    Ok(sample) => {
                        let received_ke = sample.key_expr().as_str().to_owned();
                        let payload = sample.payload().to_bytes().to_vec();
                        if let Err(e) =
                            handle_sample(&db, &bus, &metrics, kind, &received_ke, &payload).await
                        {
                            warn!(ke = %received_ke, "fan-in drop: {e}");
                        }
                    }
                    Err(e) => {
                        warn!(ke = ke, "fan-in subscriber closed: {e}");
                        break;
                    }
                }
            }
        });
        handles.push(handle);
    }
    Ok(handles)
}

async fn handle_sample(
    db: &DbPool,
    bus: &MsgBus,
    metrics: &Metrics,
    expected_kind: MsgKind,
    ke: &str,
    payload: &[u8],
) -> Result<(), GatewayError> {
    let (org_slug, zid, kind, topic) = keyexpr::parse_msg_keyexpr(ke)
        .ok_or_else(|| GatewayError::BadRequest(format!("unparsable keyexpr: {ke}")))?;
    if kind != expected_kind {
        return Err(GatewayError::BadRequest(format!(
            "keyexpr {ke} routed to {expected_kind:?} subscriber"
        )));
    }

    let env: MsgEnvelope = serde_json::from_slice(payload)
        .map_err(|e| GatewayError::BadRequest(format!("envelope: {e}")))?;

    let zid_str = zid.as_str().to_owned();
    let topic_owned = topic.clone();
    let db = db.clone();

    let result = tokio::task::spawn_blocking(move || -> Result<MsgEvent, GatewayError> {
        let mut conn = db.get()?;
        // Cross-org isolation (SCOPE.md §13 Phase 4): the published
        // keyexpr's org slug must match the device row's owning org
        // before we persist or fan out. A device publishing under
        // someone else's prefix is an ACL violation at the Zenoh
        // edge in production deployments; the gateway drops it here
        // as a belt-and-braces second check.
        let (device_id, device_org_id) = crate::db::devices::get_by_zid(&conn, &zid_str)?
            .ok_or_else(|| GatewayError::BadRequest(format!("unknown device zid {zid_str}")))?;
        let org_row = crate::db::orgs::get(&conn, device_org_id)?;
        if org_row.slug != org_slug {
            return Err(GatewayError::BadRequest(format!(
                "device {zid_str} publishing under org `{org_slug}` but owns org `{}`",
                org_row.slug
            )));
        }
        let device_org_id_captured = device_org_id;
        match kind {
            MsgKind::Event => {
                let id = events::insert(
                    &mut conn,
                    device_id,
                    &topic_owned,
                    env.ts,
                    &env.content_type,
                    &env.payload,
                )?;
                Ok(MsgEvent::Event {
                    org_id: device_org_id_captured,
                    row: events::EventRow {
                        id,
                        device_id,
                        topic: topic_owned,
                        ts: env.ts,
                        content_type: env.content_type,
                        payload: env.payload,
                    },
                })
            }
            MsgKind::Log => {
                let level = env.log_level().as_str().to_owned();
                let id = logs::insert(
                    &mut conn,
                    device_id,
                    &topic_owned,
                    env.ts,
                    &level,
                    &env.content_type,
                    &env.payload,
                )?;
                Ok(MsgEvent::Log {
                    org_id: device_org_id_captured,
                    row: logs::LogRow {
                        id,
                        device_id,
                        topic: topic_owned,
                        ts: env.ts,
                        level,
                        content_type: env.content_type,
                        payload: env.payload,
                    },
                })
            }
        }
    })
    .await
    .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))??;

    match &result {
        MsgEvent::Event { row, .. } => metrics.inc_event(&row.topic),
        MsgEvent::Log { row, .. } => metrics.inc_log(&row.level),
    }
    debug!(ke = %ke, "fan-in persisted + broadcast");
    bus.publish(result);
    Ok(())
}
