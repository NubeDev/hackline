//! Loopback-only diagnostic HTTP UI for `hackline-agent`.
//!
//! Device-side counterpart to the gateway admin UI: a tiny status
//! page operators can reach when SSH'd into the box (or via a
//! hackline TCP tunnel pointed at this bind port). Read-only — see
//! SCOPE.md §3.6 ("thin Zenoh-to-loopback bridge with no IPC
//! server"). State changes go through the gateway's cmd outbox, not
//! here.
//!
//! Modeled on `rubixd/ui/`: hand-written Bootstrap + a small vanilla
//! ES-module front end. No build step. Assets are baked into the
//! binary with `include_str!` so the agent ships standalone.

mod state;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use tracing::{info, warn};

use crate::error::AgentError;

pub use state::{ConnectionEvent, DiagState};

const INDEX_HTML: &str = include_str!("../../static/index.html");
const APP_JS: &str = include_str!("../../static/app.js");
const STYLE_CSS: &str = include_str!("../../static/style.css");
const BOOTSTRAP_CSS: &str = include_str!("../../static/bootstrap.min.css");

/// Validate the diag bind string and refuse non-loopback addresses.
/// Called from `main` so a bad config aborts startup before any
/// network listeners come up.
pub fn parse_bind(s: &str) -> Result<SocketAddr, AgentError> {
    let addr: SocketAddr = s
        .parse()
        .map_err(|e| AgentError::Config(format!("diag.bind: {e}")))?;
    if !addr.ip().is_loopback() {
        return Err(AgentError::Config(format!(
            "diag.bind must be a loopback address (got {addr}); the diag UI \
             has no auth and is not safe to expose"
        )));
    }
    Ok(addr)
}

/// Bind the diag server on the (already-validated) loopback address.
pub async fn serve(addr: SocketAddr, state: Arc<DiagState>) -> Result<(), AgentError> {
    let app = Router::new()
        .route("/", get(index))
        .route("/static/{name}", get(asset))
        .route("/api/v1/info", get(api_info))
        .route("/api/v1/zenoh", get(api_zenoh))
        .route("/api/v1/connections", get(api_connections))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| AgentError::Config(format!("diag bind {addr}: {e}")))?;
    info!(diag_addr = %addr, "diag UI ready (loopback only)");
    if let Err(e) = axum::serve(listener, app).await {
        warn!("diag server stopped: {e}");
    }
    Ok(())
}

async fn index() -> impl IntoResponse {
    serve_static(INDEX_HTML.as_bytes(), "text/html; charset=utf-8", false)
}

async fn asset(Path(name): Path<String>) -> Response {
    let (body, ct) = match name.as_str() {
        "app.js" => (APP_JS.as_bytes(), "application/javascript; charset=utf-8"),
        "style.css" => (STYLE_CSS.as_bytes(), "text/css; charset=utf-8"),
        "bootstrap.min.css" => (BOOTSTRAP_CSS.as_bytes(), "text/css; charset=utf-8"),
        _ => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    serve_static(body, ct, true).into_response()
}

#[derive(Serialize)]
struct InfoResp {
    zid: String,
    label: Option<String>,
    org: String,
    allowed_ports: Vec<u16>,
    version: &'static str,
    uptime_s: u64,
}

async fn api_info(State(s): State<Arc<DiagState>>) -> Json<InfoResp> {
    Json(InfoResp {
        zid: s.zid.clone(),
        label: s.label.clone(),
        org: s.org.clone(),
        allowed_ports: s.allowed_ports.clone(),
        version: env!("CARGO_PKG_VERSION"),
        uptime_s: s.started_at.elapsed().as_secs(),
    })
}

#[derive(Serialize)]
struct ZenohResp {
    session_zid: String,
    mode: String,
    listen: Vec<String>,
    connect: Vec<String>,
}

async fn api_zenoh(State(s): State<Arc<DiagState>>) -> Json<ZenohResp> {
    Json(ZenohResp {
        session_zid: s.session_zid.clone(),
        mode: s.zenoh_mode.clone(),
        listen: s.zenoh_listen.clone(),
        connect: s.zenoh_connect.clone(),
    })
}

#[derive(Serialize)]
struct ConnectionsResp {
    entries: Vec<ConnectionEvent>,
}

async fn api_connections(State(s): State<Arc<DiagState>>) -> Json<ConnectionsResp> {
    Json(ConnectionsResp {
        entries: s.recent_connections(),
    })
}

fn serve_static(body: &'static [u8], ct: &'static str, cacheable: bool) -> Response {
    let cache = if cacheable {
        "public, max-age=300"
    } else {
        "no-cache"
    };
    (
        StatusCode::OK,
        [(CONTENT_TYPE, ct), (CACHE_CONTROL, cache)],
        body,
    )
        .into_response()
}
