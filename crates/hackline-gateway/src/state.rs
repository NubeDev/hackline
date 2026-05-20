//! Process-wide application state passed to every axum handler.
//! Holds the db pool, the Zenoh session, the in-process events bus,
//! and the tunnel manager. Concrete (no `dyn`) — tests build a real
//! one against a loopback Zenoh router rather than mocking.

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::cmd_delivery::CmdNotifier;
use crate::db::pool::DbPool;
use crate::events_bus::MsgBus;
use crate::metrics::Metrics;
use crate::rtt_cache::RttCache;
use crate::tunnel::manager::TunnelEvent;

#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
    pub zenoh: Arc<zenoh::Session>,
    pub tunnel_tx: mpsc::Sender<TunnelEvent>,
    pub msg_bus: MsgBus,
    pub cmd_notifier: CmdNotifier,
    pub metrics: Metrics,
    pub rtt_cache: RttCache,
}
