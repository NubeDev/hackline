//! Gateway-side cmd outbox delivery loop. Two cooperating tasks:
//!
//! 1. **Ack fan-in.** Subscribes `hackline/*/msg/cmd-ack/**`, parses
//!    each sample into `(zid, cmd_id)`, decodes the `CmdAck`
//!    envelope, and writes the result to the outbox row.
//! 2. **Pusher.** Wakes on a `tokio::sync::Notify` whenever the REST
//!    handler enqueues a row, plus periodically as a fallback for
//!    devices that came online while no enqueue trigger was firing.
//!    Loads every `pending` row and publishes the `CmdEnvelope` on
//!    `hackline/<zid>/msg/cmd/<topic>`. The reliable Zenoh transport
//!    handles in-link retransmits; the durable outbox handles
//!    device-offline windows.
//!
//! Push-on-enqueue + push-on-online is one of the two strawmen in
//! SCOPE.md §14 Q2. The chosen shape keeps a single broadcast
//! keyexpr per cmd; pull-from-device would require per-device offset
//! state which we don't otherwise need.

use std::sync::Arc;
use std::time::Duration;

use hackline_proto::keyexpr;
use hackline_proto::msg::{CmdAck, CmdEnvelope, MsgEnvelope};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use uuid::Uuid;
use zenoh::bytes::ZBytes;
use zenoh::Session;

use crate::db::cmd_outbox;
use crate::db::devices;
use crate::db::pool::DbPool;
use crate::error::GatewayError;
use crate::metrics::Metrics;

/// Wakes the pusher task whenever a REST handler enqueues a cmd.
/// Lives in `AppState` so handlers can call `notify_one()`.
#[derive(Clone, Default)]
pub struct CmdNotifier {
    inner: Arc<Notify>,
}

impl CmdNotifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn notify(&self) {
        self.inner.notify_one();
    }

    async fn notified(&self) {
        self.inner.notified().await;
    }
}

pub async fn spawn(
    session: Arc<Session>,
    db: DbPool,
    notifier: CmdNotifier,
    metrics: Metrics,
) -> Result<Vec<JoinHandle<()>>, GatewayError> {
    let mut handles = Vec::with_capacity(2);

    let ack_sub = session
        .declare_subscriber(keyexpr::MSG_CMD_ACK_FANIN.to_owned())
        .await
        .map_err(GatewayError::Zenoh)?;
    tracing::info!(ke = keyexpr::MSG_CMD_ACK_FANIN, "cmd-ack fan-in ready");

    let db_ack = db.clone();
    let metrics_ack = metrics.clone();
    handles.push(tokio::spawn(async move {
        loop {
            match ack_sub.recv_async().await {
                Ok(sample) => {
                    let ke = sample.key_expr().as_str().to_owned();
                    let bytes = sample.payload().to_bytes().to_vec();
                    if let Err(e) = handle_ack(&db_ack, &metrics_ack, &ke, &bytes).await {
                        warn!(ke = %ke, "cmd-ack drop: {e}");
                    }
                }
                Err(e) => {
                    warn!("cmd-ack subscriber closed: {e}");
                    break;
                }
            }
        }
    }));

    let db_push = db.clone();
    let sess_push = session.clone();
    let notif_push = notifier.clone();
    let metrics_push = metrics.clone();
    handles.push(tokio::spawn(async move {
        loop {
            if let Err(e) = drain_pending(&db_push, &sess_push, &metrics_push).await {
                warn!("cmd pusher: {e}");
            }
            tokio::select! {
                _ = notif_push.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(30)) => {}
            }
        }
    }));

    Ok(handles)
}

async fn handle_ack(
    db: &DbPool,
    metrics: &Metrics,
    ke: &str,
    payload: &[u8],
) -> Result<(), GatewayError> {
    let (_org, _zid, cmd_id) = keyexpr::parse_msg_cmd_ack_keyexpr(ke)
        .ok_or_else(|| GatewayError::BadRequest(format!("unparsable cmd-ack keyexpr: {ke}")))?;
    let ack: CmdAck = serde_json::from_slice(payload)
        .map_err(|e| GatewayError::BadRequest(format!("cmd-ack envelope: {e}")))?;
    if ack.cmd_id != cmd_id {
        return Err(GatewayError::BadRequest(format!(
            "cmd-ack body cmd_id {} != keyexpr {}",
            ack.cmd_id, cmd_id
        )));
    }
    let cmd_id_str = ack.cmd_id.to_string();
    let result = ack.result.as_str().to_owned();
    let detail = ack.detail.clone();
    let db = db.clone();
    tokio::task::spawn_blocking(move || -> Result<(), GatewayError> {
        let conn = db.get()?;
        let _ = cmd_outbox::record_ack(&conn, &cmd_id_str, &result, detail.as_deref(), now_ms())?;
        Ok(())
    })
    .await
    .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))??;
    metrics.inc_cmd(ack.result.as_str());
    debug!(cmd_id = %ack.cmd_id, "cmd ack recorded");
    Ok(())
}

async fn drain_pending(
    db: &DbPool,
    session: &Arc<Session>,
    metrics: &Metrics,
) -> Result<(), GatewayError> {
    let now = now_ms();
    let db2 = db.clone();
    let rows: Vec<cmd_outbox::CmdRow> = tokio::task::spawn_blocking(move || {
        let conn = db2.get()?;
        cmd_outbox::list_pending(&conn, now)
    })
    .await
    .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))??;

    if rows.is_empty() {
        return Ok(());
    }

    for row in rows {
        let db3 = db.clone();
        let device_id = row.device_id;
        let lookup = tokio::task::spawn_blocking(move || {
            let conn = db3.get()?;
            devices::get_with_org_slug(&conn, device_id)
        })
        .await
        .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))?;
        let (device, org_slug) = match lookup {
            Ok(pair) => pair,
            Err(e) => {
                warn!(device_id, "cmd pusher: device lookup failed: {e}");
                continue;
            }
        };

        let zid = match hackline_proto::Zid::new(&device.zid) {
            Ok(z) => z,
            Err(e) => {
                warn!(device_id, "cmd pusher: bad zid {}: {e}", device.zid);
                continue;
            }
        };

        let cmd_id = match Uuid::parse_str(&row.cmd_id) {
            Ok(u) => u,
            Err(e) => {
                warn!(cmd_id = %row.cmd_id, "cmd pusher: bad cmd_id: {e}");
                continue;
            }
        };

        let env = CmdEnvelope {
            cmd_id,
            topic: row.topic.clone(),
            enqueued_at: row.enqueued_at,
            expires_at: row.expires_at,
            envelope: MsgEnvelope {
                id: cmd_id,
                ts: row.enqueued_at,
                content_type: row.content_type.clone(),
                headers: Default::default(),
                payload: row.payload.clone(),
            },
        };
        let bytes = match serde_json::to_vec(&env) {
            Ok(b) => b,
            Err(e) => {
                warn!(cmd_id = %row.cmd_id, "cmd envelope encode failed: {e}");
                continue;
            }
        };
        let ke = keyexpr::msg_cmd(&org_slug, &zid, &row.topic);
        match session.put(ke.clone(), ZBytes::from(bytes)).await {
            Ok(()) => {
                debug!(cmd_id = %row.cmd_id, ke = %ke, "cmd published");
                metrics.inc_cmd("delivered");
                let db4 = db.clone();
                let cmd_id_str = row.cmd_id.clone();
                let _ = tokio::task::spawn_blocking(move || -> Result<(), GatewayError> {
                    let conn = db4.get()?;
                    cmd_outbox::mark_delivered(&conn, &cmd_id_str, now_ms())?;
                    Ok(())
                })
                .await
                .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))?;
            }
            Err(e) => {
                warn!(cmd_id = %row.cmd_id, "cmd publish failed: {e}");
            }
        }
    }
    Ok(())
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
