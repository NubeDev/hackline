//! Single shared HTTP listener that routes by `Host:` to the right
//! `kind = 'http'` tunnel and bridges the connection through the
//! same Zenoh byte tunnel used for raw TCP. WebSocket upgrades pass
//! through unchanged — we are not parsing HTTP framing, just peeking
//! the first request's `Host:` header off the wire to pick a route.
//!
//! Routing rule: the listener accepts a TCP connection, reads
//! request bytes until it has seen the first `Host:` header, then
//! matches the host against the `tunnels` table (`kind = 'http'`,
//! `public_hostname = <host>`). Bytes already read are forwarded
//! into the bridge before the socket halves are pumped freely.
//!
//! Keep-alive across different hostnames on a single TCP connection
//! is not supported — the matching `tunnels` row is selected once
//! per connection. HTTP/2 host-routing belongs to Phase 3.

use std::sync::Arc;
use std::time::Duration;

use hackline_proto::Zid;
use rusqlite::OptionalExtension;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{info, warn};
use zenoh::bytes::ZBytes;
use zenoh::Session;

use crate::db::audit;
use crate::db::pool::DbPool;
use crate::error::GatewayError;
use crate::metrics::{Metrics, Outcome};

const HEADER_LIMIT: usize = 8 * 1024;
const READ_BUF: usize = 8 * 1024;

/// Run a single HTTP host-routing listener on `listen_addr`. Each
/// accepted connection is host-routed to the matching `http` tunnel.
pub async fn run(
    db: DbPool,
    session: Arc<Session>,
    metrics: Metrics,
    listen_addr: &str,
) -> Result<(), GatewayError> {
    let listener = TcpListener::bind(listen_addr).await?;
    info!(addr = listen_addr, "HTTP host-router listening");

    loop {
        let (tcp, peer) = listener.accept().await?;
        let db = db.clone();
        let session = session.clone();
        let metrics = metrics.clone();
        let peer_s = peer.to_string();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(db, session, metrics, tcp, peer_s).await {
                warn!(%peer, "http host-router: {e}");
            }
        });
    }
}

async fn handle_connection(
    db: DbPool,
    session: Arc<Session>,
    metrics: Metrics,
    mut tcp: tokio::net::TcpStream,
    peer: String,
) -> Result<(), GatewayError> {
    let mut prefix = Vec::with_capacity(2048);
    let mut buf = [0u8; READ_BUF];
    let host = loop {
        if prefix.len() > HEADER_LIMIT {
            return Err(GatewayError::BadRequest(
                "HTTP header section exceeded 8 KiB".into(),
            ));
        }
        let n = tcp.read(&mut buf).await?;
        if n == 0 {
            return Err(GatewayError::BadRequest(
                "HTTP preamble closed early".into(),
            ));
        }
        prefix.extend_from_slice(&buf[..n]);
        if let Some(h) = parse_host_header(&prefix) {
            break h;
        }
        if find_double_crlf(&prefix).is_some() {
            return Err(GatewayError::BadRequest(
                "HTTP request missing Host header".into(),
            ));
        }
    };

    let host_lookup = host.clone();
    let db_for_lookup = db.clone();
    // One-shot tuple consumed by the `match` below; naming it would
    // require a public type alias for a private join projection.
    #[allow(clippy::type_complexity)]
    let row = tokio::task::spawn_blocking(
        move || -> Result<Option<(String, i64, i64, i64, String)>, GatewayError> {
            let conn = db_for_lookup.get()?;
            let r = conn
                .query_row(
                    "SELECT d.zid, t.local_port, t.id, t.device_id, o.slug
                       FROM tunnels t
                       JOIN devices d ON d.id = t.device_id
                       JOIN orgs    o ON o.id = d.org_id
                      WHERE t.kind = 'http'
                        AND t.public_hostname = ?1
                        AND t.enabled = 1",
                    rusqlite::params![host_lookup],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, i64>(2)?,
                            r.get::<_, i64>(3)?,
                            r.get::<_, String>(4)?,
                        ))
                    },
                )
                .optional()
                .map_err(GatewayError::Db)?;
            Ok(r)
        },
    )
    .await
    .map_err(|e| GatewayError::Config(format!("blocking task join: {e}")))??;

    let (zid_str, local_port, tunnel_id, device_id, org_slug) =
        row.ok_or_else(|| GatewayError::BadRequest(format!("no http tunnel for host `{host}`")))?;
    let zid = Zid::new(&zid_str).map_err(|e| GatewayError::BadRequest(e.to_string()))?;

    metrics.inc_tunnel_active("http");
    let request_id = uuid::Uuid::new_v4();
    let ts_open = now_ms();
    let audit_id = {
        let db = db.clone();
        let rid = request_id.to_string();
        let peer_owned = peer.clone();
        tokio::task::spawn_blocking(move || -> Result<i64, GatewayError> {
            let conn = db.get()?;
            audit::insert_tunnel_session_open(
                &conn,
                device_id,
                Some(tunnel_id),
                &rid,
                Some(&peer_owned),
                ts_open,
            )
        })
        .await
        .ok()
        .and_then(|r| r.ok())
    };

    let bridge_result = bridge_with_prefix(
        &session,
        &org_slug,
        &zid,
        local_port as u16,
        tcp,
        prefix,
        request_id,
    )
    .await;

    metrics.dec_tunnel_active("http");

    let (bytes_up, bytes_down, outcome, ret) = match bridge_result {
        Ok((up, down)) => (up, down, Outcome::Ok, Ok(())),
        Err(e) => (0u64, 0u64, Outcome::Error, Err(e)),
    };

    metrics.inc_tunnel_session("http", outcome);
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

    ret
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Open a bridge to the device's local HTTP port and stream bytes
/// in both directions. The captured `prefix` is forwarded first so
/// the device's local HTTP server sees a complete request.
async fn bridge_with_prefix(
    session: &Session,
    org: &str,
    zid: &Zid,
    port: u16,
    tcp: tokio::net::TcpStream,
    prefix: Vec<u8>,
    request_id: uuid::Uuid,
) -> Result<(u64, u64), GatewayError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    let (mut tcp_read, mut tcp_write) = tcp.into_split();

    let req = hackline_proto::connect::ConnectRequest {
        request_id,
        peer: None,
    };
    let ke_connect = hackline_proto::keyexpr::connect(org, zid, port);
    let replies = session
        .get(&ke_connect)
        .payload(ZBytes::from(serde_json::to_vec(&req).unwrap()))
        .timeout(Duration::from_secs(2))
        .await
        .map_err(GatewayError::Zenoh)?;
    let reply = tokio::time::timeout(Duration::from_secs(10), replies.recv_async())
        .await
        .map_err(|_| GatewayError::BadRequest("device ack timeout".into()))?
        .map_err(GatewayError::Zenoh)?;
    drop(replies);

    let ack_bytes = reply
        .result()
        .map_err(|e| GatewayError::BadRequest(format!("device ack: {e:?}")))?
        .payload()
        .to_bytes()
        .to_vec();
    let ack: hackline_proto::connect::ConnectAck = serde_json::from_slice(&ack_bytes)
        .map_err(|e| GatewayError::BadRequest(format!("ack decode: {e}")))?;
    if !ack.ok {
        return Err(GatewayError::BadRequest(
            ack.message.unwrap_or_else(|| "device rejected".into()),
        ));
    }

    let ke_to_dev = hackline_proto::keyexpr::stream_gw(org, zid, &request_id);
    let ke_from_dev = hackline_proto::keyexpr::stream_dev(org, zid, &request_id);

    let publisher = session
        .declare_publisher(ke_to_dev)
        .await
        .map_err(GatewayError::Zenoh)?;
    let subscriber = session
        .declare_subscriber(ke_from_dev)
        .await
        .map_err(GatewayError::Zenoh)?;

    let up = Arc::new(AtomicU64::new(0));
    let down = Arc::new(AtomicU64::new(0));

    if !prefix.is_empty() {
        up.fetch_add(prefix.len() as u64, Ordering::Relaxed);
        publisher
            .put(ZBytes::from(prefix))
            .await
            .map_err(GatewayError::Zenoh)?;
    }

    let up_in = up.clone();
    let tcp_to_zenoh = tokio::spawn(async move {
        let mut b = vec![0u8; 32 * 1024];
        loop {
            match tcp_read.read(&mut b).await {
                Ok(0) => {
                    let _ = publisher.put(ZBytes::from(Vec::<u8>::new())).await;
                    break;
                }
                Ok(n) => {
                    up_in.fetch_add(n as u64, Ordering::Relaxed);
                    if publisher.put(ZBytes::from(b[..n].to_vec())).await.is_err() {
                        break;
                    }
                }
                Err(_) => {
                    let _ = publisher.put(ZBytes::from(Vec::<u8>::new())).await;
                    break;
                }
            }
        }
    });

    let down_in = down.clone();
    let zenoh_to_tcp = tokio::spawn(async move {
        loop {
            let sample = match subscriber.recv_async().await {
                Ok(s) => s,
                Err(_) => break,
            };
            let bytes = sample.payload().to_bytes().to_vec();
            if bytes.is_empty() {
                break;
            }
            down_in.fetch_add(bytes.len() as u64, Ordering::Relaxed);
            if tcp_write.write_all(&bytes).await.is_err() {
                break;
            }
        }
        let _ = tcp_write.shutdown().await;
    });

    let _ = tokio::join!(tcp_to_zenoh, zenoh_to_tcp);
    Ok((up.load(Ordering::Relaxed), down.load(Ordering::Relaxed)))
}

fn parse_host_header(buf: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(buf).ok()?;
    for line in text.split("\r\n") {
        if line.is_empty() {
            return None;
        }
        for prefix in ["Host:", "host:", "HOST:"] {
            if let Some(rest) = line.strip_prefix(prefix) {
                return Some(rest.trim().to_string());
            }
        }
    }
    None
}

fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_header_parsing() {
        let req = b"GET / HTTP/1.1\r\nHost: device-1.cloud.example.com\r\nUser-Agent: x\r\n\r\n";
        assert_eq!(
            parse_host_header(req).as_deref(),
            Some("device-1.cloud.example.com")
        );
    }

    #[test]
    fn host_header_missing() {
        let req = b"GET / HTTP/1.1\r\nUser-Agent: x\r\n\r\n";
        assert!(parse_host_header(req).is_none());
    }
}
