//! `GET /v1/events/stream` — live SSE feed of every device's
//! message-plane events. One `data:` frame per row, payload is the
//! same JSON shape as the cursor API returns. Reverse proxies must
//! disable response buffering (`flush_interval -1` in Caddy) or the
//! browser will not see frames until the connection closes —
//! SCOPE.md §5.4.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::auth::middleware::AuthedUser;
use crate::events_bus::MsgEvent;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    /// Restrict to one device id. Without it the stream carries
    /// every device's events.
    pub device: Option<i64>,
    /// Topic glob (SQLite GLOB semantics). `graph.*` matches one
    /// segment, `graph.**` is not supported here — use repeated
    /// `*` segments instead.
    pub topic: Option<String>,
}

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
    Query(q): Query<StreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.msg_bus.subscribe();
    let caller_org = caller.org_id;
    let stream = BroadcastStream::new(rx).filter_map(move |item| {
        let (org_id, row) = match item {
            Ok(MsgEvent::Event { org_id, row }) => (org_id, row),
            // Lagged subscribers: log and drop; client should
            // reconnect and replay via the cursor API.
            _ => return None,
        };
        // Cross-org isolation (SCOPE.md §13 Phase 4): each frame
        // carries its owning org; drop anything from another tenant
        // without leaking even its existence to the subscriber.
        if org_id != caller_org {
            return None;
        }
        if let Some(d) = q.device {
            if row.device_id != d {
                return None;
            }
        }
        if let Some(ref glob) = q.topic {
            if !glob_match(glob, &row.topic) {
                return None;
            }
        }
        match serde_json::to_string(&row) {
            Ok(json) => Some(Ok(Event::default().event("event").data(json))),
            Err(_) => None,
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

/// Re-export under a stable name so the logs stream can reuse it
/// without growing a third file just for the matcher.
pub fn glob_match_pub(pattern: &str, value: &str) -> bool {
    glob_match(pattern, value)
}

/// Trivial `*` / `?` matcher for topic globs. Mirrors the GLOB
/// semantics the cursor API uses, scoped to topic strings — keeping
/// the SSE filter in-process avoids round-tripping every broadcast
/// through SQLite.
fn glob_match(pattern: &str, value: &str) -> bool {
    fn inner(p: &[u8], v: &[u8]) -> bool {
        if p.is_empty() {
            return v.is_empty();
        }
        match p[0] {
            b'*' => {
                if inner(&p[1..], v) {
                    return true;
                }
                if v.is_empty() {
                    return false;
                }
                inner(p, &v[1..])
            }
            b'?' => !v.is_empty() && inner(&p[1..], &v[1..]),
            c => !v.is_empty() && v[0] == c && inner(&p[1..], &v[1..]),
        }
    }
    inner(pattern.as_bytes(), value.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::glob_match;
    #[test]
    fn globs() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("graph.*", "graph.x"));
        assert!(!glob_match("graph.*", "audit.x"));
        assert!(glob_match("a.b.c", "a.b.c"));
    }
}
