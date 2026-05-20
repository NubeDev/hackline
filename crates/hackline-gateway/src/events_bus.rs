//! In-process broadcast bus. Carries device→cloud message-plane
//! deliveries (`events` + `logs`) after the gateway has persisted
//! them, so SSE subscribers see the same row that the cursor API
//! will return. Backed by `tokio::sync::broadcast` — a lagging
//! subscriber gets a `Lagged` error and is expected to reconnect
//! and replay history via the cursor API.

use tokio::sync::broadcast;

use crate::db::events::EventRow;
use crate::db::logs::LogRow;

/// Enough capacity to absorb a short burst from one device while a
/// slow SSE client catches up. Beyond this, the bus drops the oldest
/// message for that subscriber.
const CHANNEL_CAPACITY: usize = 1024;

/// Bus envelope. `org_id` is captured at publish time so SSE
/// subscribers can filter cross-org rows without a per-frame DB
/// lookup (SCOPE.md §13 Phase 4).
#[derive(Debug, Clone)]
pub enum MsgEvent {
    Event { org_id: i64, row: EventRow },
    Log { org_id: i64, row: LogRow },
}

#[derive(Clone)]
pub struct MsgBus {
    tx: broadcast::Sender<MsgEvent>,
}

impl MsgBus {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<MsgEvent> {
        self.tx.subscribe()
    }

    pub fn publish(&self, msg: MsgEvent) {
        // `send` only errors when there are zero subscribers — the
        // row is already persisted, so dropping it on the bus is a
        // no-op.
        let _ = self.tx.send(msg);
    }
}

impl Default for MsgBus {
    fn default() -> Self {
        Self::new()
    }
}
