//! `GET /v1/log/stream` — live SSE feed of every device's logs.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::api::events::stream::glob_match_pub;
use crate::auth::middleware::AuthedUser;
use crate::events_bus::MsgEvent;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    pub device: Option<i64>,
    pub topic: Option<String>,
    pub level: Option<String>,
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
            Ok(MsgEvent::Log { org_id, row }) => (org_id, row),
            _ => return None,
        };
        if org_id != caller_org {
            return None;
        }
        if let Some(d) = q.device {
            if row.device_id != d {
                return None;
            }
        }
        if let Some(ref glob) = q.topic {
            if !glob_match_pub(glob, &row.topic) {
                return None;
            }
        }
        if let Some(ref l) = q.level {
            if row.level != *l {
                return None;
            }
        }
        match serde_json::to_string(&row) {
            Ok(json) => Some(Ok(Event::default().event("log").data(json))),
            Err(_) => None,
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
