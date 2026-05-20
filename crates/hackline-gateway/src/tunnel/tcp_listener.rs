//! Per-tunnel TCP listener. One task per `kind = 'tcp'` row; accepts
//! connections and bridges each one into the device. Every accepted
//! connection emits exactly one `tunnel.session` audit row — inserted
//! at bridge open and finalised at close with byte counters and
//! `ts_close` (SCOPE.md §7.2). The matching Prometheus counters and
//! the `tunnel_active{kind}` gauge are updated on the same boundary.

use std::sync::Arc;

use hackline_proto::Zid;
use tokio::net::TcpListener;
use tracing::{info, warn};
use zenoh::Session;

use crate::db::audit;
use crate::db::pool::DbPool;
use crate::error::GatewayError;
use crate::metrics::{Metrics, Outcome};

/// Optional TLS acceptor for tunnel TCP sockets. `None` (or any build
/// without the `tls` feature) means the listener bridges raw TCP, the
/// way it always has. With `Some(_)`, every accepted socket is
/// `accept`-handshaked through rustls before bytes are pumped — same
/// cert chain as the REST API, since `tls::TlsState` builds both from
/// the same PEM material.
///
/// The acceptor is wrapped in `Arc<ArcSwap<_>>` so that ACME
/// renewal can swap in a fresh cert without restarting any listener:
/// `bridge_socket` calls `load_full()` immediately before each
/// handshake, so the very next accepted connection picks up the new
/// cert. Already-handshaken sockets keep running on their previous
/// session keys until they close, which is correct — TLS does not
/// require renegotiation when the server cert rotates.
#[cfg(feature = "tls")]
pub type TunnelTls = Option<std::sync::Arc<arc_swap::ArcSwap<tokio_rustls::TlsAcceptor>>>;
#[cfg(not(feature = "tls"))]
pub type TunnelTls = Option<std::convert::Infallible>;

/// Listen on `listen_port` and bridge every accepted connection to
/// `device_port` on the device identified by `zid`. `tunnel_id` and
/// `device_id` are stamped onto the `tunnel.session` audit row so the
/// admin UI can join back to the originating tunnel without re-parsing
/// a JSON detail blob.
pub async fn run_tcp_listener(
    session: Arc<Session>,
    db: DbPool,
    metrics: Metrics,
    tunnel_id: i64,
    device_id: i64,
    org_slug: String,
    zid: Zid,
    device_port: u16,
    listen_port: u16,
    tls: TunnelTls,
) -> Result<(), GatewayError> {
    let listener = TcpListener::bind(format!("0.0.0.0:{listen_port}")).await?;
    info!(
        listen_port,
        %zid,
        org = %org_slug,
        device_port,
        tls = tls.is_some(),
        "tcp tunnel listener ready",
    );

    loop {
        let (tcp, addr) = listener.accept().await?;
        let s = session.clone();
        let z = zid.clone();
        let db = db.clone();
        let metrics = metrics.clone();
        let org = org_slug.clone();
        // See `manager.rs`: `.clone()` is the right Arc bump under
        // `feature = "tls"` and a no-op clippy warning under the
        // default no-feature build where `TunnelTls` is Copy.
        #[allow(clippy::clone_on_copy)]
        let tls = tls.clone();
        tokio::spawn(async move {
            let peer = addr.to_string();
            run_bridged_connection(
                s,
                db,
                metrics,
                tunnel_id,
                device_id,
                "tcp",
                org,
                z,
                device_port,
                tcp,
                Some(peer),
                tls,
            )
            .await;
        });
    }
}

/// Bridge one accepted socket and book-end it with a `tunnel.session`
/// audit row. `kind` is `"tcp"` for this listener and `"http"` when
/// the host-router reuses this helper.
pub async fn run_bridged_connection(
    session: Arc<Session>,
    db: DbPool,
    metrics: Metrics,
    tunnel_id: i64,
    device_id: i64,
    kind: &'static str,
    org_slug: String,
    zid: Zid,
    device_port: u16,
    tcp: tokio::net::TcpStream,
    peer: Option<String>,
    tls: TunnelTls,
) {
    metrics.inc_tunnel_active(kind);
    let now_ms_open = now_ms();
    let request_id = uuid::Uuid::new_v4();

    let audit_id = {
        let db = db.clone();
        let peer_for_blocking = peer.clone();
        let rid = request_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<i64, GatewayError> {
            let conn = db.get()?;
            audit::insert_tunnel_session_open(
                &conn,
                device_id,
                Some(tunnel_id),
                &rid,
                peer_for_blocking.as_deref(),
                now_ms_open,
            )
        })
        .await
        .ok()
        .and_then(|r| r.ok())
    };

    let result = bridge_socket(
        &session,
        &org_slug,
        &zid,
        device_port,
        tcp,
        peer.clone(),
        tls,
    )
    .await;

    metrics.dec_tunnel_active(kind);

    let (bytes_up, bytes_down, outcome) = match result {
        Ok(b) => {
            // Successful bridge proves the device responded over
            // Zenoh; treat it as a liveliness ping so `last_seen_at`
            // reflects bytes-flowed, not just the standalone token.
            let db = db.clone();
            let _ = tokio::task::spawn_blocking(move || -> Result<(), GatewayError> {
                let conn = db.get()?;
                crate::db::devices::touch_last_seen(&conn, device_id)
            })
            .await;
            (b.up, b.down, Outcome::Ok)
        }
        Err(e) => {
            warn!(?peer, "bridge error: {e}");
            (0u64, 0u64, Outcome::Error)
        }
    };

    metrics.inc_tunnel_session(kind, outcome);
    metrics.add_tunnel_bytes(bytes_up, bytes_down);

    if let Some(id) = audit_id {
        let db = db.clone();
        let ts_close = now_ms();
        let _ = tokio::task::spawn_blocking(move || -> Result<(), GatewayError> {
            let conn = db.get()?;
            audit::finalize_tunnel_session(&conn, id, bytes_up, bytes_down, ts_close)
        })
        .await;
    }

    info!(?peer, bytes_up, bytes_down, "connection closed");
}

/// Wrap the accepted TCP socket in TLS (if configured) and call into
/// the appropriate bridge entry point. Split out so the audit
/// book-ending stays one cohesive function regardless of TLS mode,
/// and so the `#[cfg]` is contained to the one branch that actually
/// touches `tokio_rustls`.
#[cfg(feature = "tls")]
async fn bridge_socket(
    session: &Session,
    org_slug: &str,
    zid: &Zid,
    device_port: u16,
    tcp: tokio::net::TcpStream,
    peer: Option<String>,
    tls: TunnelTls,
) -> Result<hackline_core::bridge::BridgeBytes, hackline_core::error::BridgeError> {
    match tls {
        Some(acceptor_swap) => {
            // Load the current acceptor immediately before the
            // handshake so a renewer that swapped a fresh cert in is
            // honoured by the very next accepted connection.
            let acceptor = acceptor_swap.load_full();
            let tls_stream = acceptor
                .accept(tcp)
                .await
                .map_err(hackline_core::error::BridgeError::Io)?;
            hackline_core::bridge::initiate_bridge_io_with_id(
                session,
                org_slug,
                zid,
                device_port,
                tls_stream,
                peer,
            )
            .await
            .map(|(_id, b)| b)
        }
        None => {
            hackline_core::bridge::initiate_bridge(session, org_slug, zid, device_port, tcp, peer)
                .await
        }
    }
}

#[cfg(not(feature = "tls"))]
async fn bridge_socket(
    session: &Session,
    org_slug: &str,
    zid: &Zid,
    device_port: u16,
    tcp: tokio::net::TcpStream,
    peer: Option<String>,
    _tls: TunnelTls,
) -> Result<hackline_core::bridge::BridgeBytes, hackline_core::error::BridgeError> {
    hackline_core::bridge::initiate_bridge(session, org_slug, zid, device_port, tcp, peer).await
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
