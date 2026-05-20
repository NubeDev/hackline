//! Loopback-only diagnostic HTTP UI for `hackline-agent`.
//!
//! Device-side counterpart to the gateway admin UI. Read-mostly,
//! with two write surfaces: add/remove port queryables at runtime.
//! Those mutations are safe to expose without auth because the bind
//! is loopback-only (validated at startup) — anything that can hit
//! this port can also `kill` the agent process, so it gets the same
//! trust as local shell access.
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
use axum::routing::{delete, get};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::error::AgentError;

pub use state::{ActivePort, ConnectionEvent, DiagState};

const INDEX_HTML: &str = include_str!("../../static/index.html");
const APP_JS: &str = include_str!("../../static/app.js");
const STYLE_CSS: &str = include_str!("../../static/style.css");
const BOOTSTRAP_CSS: &str = include_str!("../../static/bootstrap.min.css");

// lib/ modules
const LIB_DOM_JS: &str = include_str!("../../static/lib/dom.js");
const LIB_API_JS: &str = include_str!("../../static/lib/api.js");
const LIB_FMT_JS: &str = include_str!("../../static/lib/fmt.js");
const LIB_UI_JS: &str = include_str!("../../static/lib/ui.js");

// views/ modules
const VIEWS_OVERVIEW_JS: &str = include_str!("../../static/views/overview.js");
const VIEWS_PORTS_JS: &str = include_str!("../../static/views/ports.js");
const VIEWS_ZENOH_JS: &str = include_str!("../../static/views/zenoh.js");
const VIEWS_CONNECTIONS_JS: &str = include_str!("../../static/views/connections.js");
const VIEWS_SETUP_JS: &str = include_str!("../../static/views/setup.js");

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

pub async fn serve(addr: SocketAddr, state: Arc<DiagState>) -> Result<(), AgentError> {
    let app = Router::new()
        .route("/", get(index))
        .route("/static/{*path}", get(asset))
        .route("/api/v1/info", get(api_info))
        .route("/api/v1/zenoh", get(api_zenoh))
        .route("/api/v1/connections", get(api_connections))
        .route("/api/v1/ports", get(api_ports_list).post(api_ports_add))
        .route("/api/v1/ports/{port}", delete(api_ports_remove))
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
    serve_static(INDEX_HTML.as_bytes(), "text/html; charset=utf-8")
}

async fn asset(Path(path): Path<String>) -> Response {
    let (body, ct) = match path.as_str() {
        "app.js" => (APP_JS.as_bytes(), "application/javascript; charset=utf-8"),
        "style.css" => (STYLE_CSS.as_bytes(), "text/css; charset=utf-8"),
        "bootstrap.min.css" => (BOOTSTRAP_CSS.as_bytes(), "text/css; charset=utf-8"),
        "lib/dom.js" => (LIB_DOM_JS.as_bytes(), "application/javascript; charset=utf-8"),
        "lib/api.js" => (LIB_API_JS.as_bytes(), "application/javascript; charset=utf-8"),
        "lib/fmt.js" => (LIB_FMT_JS.as_bytes(), "application/javascript; charset=utf-8"),
        "lib/ui.js" => (LIB_UI_JS.as_bytes(), "application/javascript; charset=utf-8"),
        "views/overview.js" => (VIEWS_OVERVIEW_JS.as_bytes(), "application/javascript; charset=utf-8"),
        "views/ports.js" => (VIEWS_PORTS_JS.as_bytes(), "application/javascript; charset=utf-8"),
        "views/zenoh.js" => (VIEWS_ZENOH_JS.as_bytes(), "application/javascript; charset=utf-8"),
        "views/connections.js" => (VIEWS_CONNECTIONS_JS.as_bytes(), "application/javascript; charset=utf-8"),
        "views/setup.js" => (VIEWS_SETUP_JS.as_bytes(), "application/javascript; charset=utf-8"),
        _ => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    // Assets are baked into the binary with include_str! and only
    // change when the binary is rebuilt. A long max-age caches stale
    // markup across upgrades (operator restarts the agent, browser
    // still shows the old UI for 5 minutes); revalidate every load.
    serve_static(body, ct).into_response()
}

#[derive(Serialize)]
struct PortEntry {
    port: u16,
    from_config: bool,
}

#[derive(Serialize)]
struct GatewayInfo {
    /// `cfg.zenoh.connect` verbatim, so the UI can show what the
    /// operator configured even when zero peers are connected.
    configured: Vec<String>,
    /// Count of currently-open Zenoh peer transports. >0 means we
    /// have at least one neighbour; in a typical agent deployment
    /// that neighbour is the configured gateway.
    peer_count: usize,
    connected: bool,
}

#[derive(Serialize)]
struct InfoResp {
    zid: String,
    label: Option<String>,
    org: String,
    ports: Vec<PortEntry>,
    version: &'static str,
    uptime_s: u64,
    gateway: GatewayInfo,
}

async fn api_info(State(s): State<Arc<DiagState>>) -> Json<InfoResp> {
    let ports = s
        .active_ports()
        .into_iter()
        .map(|(port, from_config)| PortEntry { port, from_config })
        .collect();

    let peer_count = peer_count(&s).await;
    let gateway = GatewayInfo {
        configured: s.zenoh_connect.clone(),
        peer_count,
        connected: peer_count > 0,
    };

    Json(InfoResp {
        zid: s.zid.clone(),
        label: s.label.clone(),
        org: s.org.clone(),
        ports,
        version: env!("CARGO_PKG_VERSION"),
        uptime_s: s.started_at.elapsed().as_secs(),
        gateway,
    })
}

async fn peer_count(state: &DiagState) -> usize {
    // `peers_zid()` is part of the stable Zenoh surface and yields
    // one entry per connected peer. We count rather than collect to
    // keep the diag handler allocation-free in the hot read path.
    state.session.info().peers_zid().await.count()
}

#[derive(Serialize)]
struct ZenohResp {
    session_zid: String,
    mode: String,
    listen: Vec<String>,
    connect: Vec<String>,
    peers: Vec<String>,
}

async fn api_zenoh(State(s): State<Arc<DiagState>>) -> Json<ZenohResp> {
    let peers: Vec<String> = s
        .session
        .info()
        .peers_zid()
        .await
        .map(|zid| zid.to_string())
        .collect();
    Json(ZenohResp {
        session_zid: s.session_zid.clone(),
        mode: s.zenoh_mode.clone(),
        listen: s.zenoh_listen.clone(),
        connect: s.zenoh_connect.clone(),
        peers,
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

#[derive(Serialize)]
struct PortsListResp {
    ports: Vec<PortEntry>,
}

async fn api_ports_list(State(s): State<Arc<DiagState>>) -> Json<PortsListResp> {
    let ports = s
        .active_ports()
        .into_iter()
        .map(|(port, from_config)| PortEntry { port, from_config })
        .collect();
    Json(PortsListResp { ports })
}

#[derive(Deserialize)]
struct AddPortReq {
    port: u16,
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

async fn api_ports_add(
    State(s): State<Arc<DiagState>>,
    Json(req): Json<AddPortReq>,
) -> Response {
    if req.port == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "port must be > 0".into(),
            }),
        )
            .into_response();
    }
    match crate::connect::spawn_port_queryable(s.clone(), req.port, false).await {
        Ok(()) => {
            info!(port = req.port, "diag: added port at runtime");
            (StatusCode::CREATED, Json(PortEntry {
                port: req.port,
                from_config: false,
            }))
                .into_response()
        }
        Err(e) => (
            StatusCode::CONFLICT,
            Json(ApiError {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_ports_remove(
    State(s): State<Arc<DiagState>>,
    Path(port): Path<u16>,
) -> Response {
    if s.remove_port(port) {
        info!(port, "diag: removed port at runtime");
        StatusCode::NO_CONTENT.into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: format!("port {port} is not active"),
            }),
        )
            .into_response()
    }
}

fn serve_static(body: &'static [u8], ct: &'static str) -> Response {
    (
        StatusCode::OK,
        [(CONTENT_TYPE, ct), (CACHE_CONTROL, "no-cache")],
        body,
    )
        .into_response()
}
