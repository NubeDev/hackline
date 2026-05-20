//! Bidirectional byte bridge between a TCP stream and a pair of Zenoh
//! pub/sub channels. The connect handshake uses a one-shot query/reply;
//! after the ack, data flows on `…/stream/<request_id>/gw` (gateway→agent)
//! and `…/stream/<request_id>/dev` (agent→gateway) until close.

use std::time::Duration;

use hackline_proto::connect::{ConnectAck, ConnectRequest};
use hackline_proto::keyexpr;
use hackline_proto::Zid;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, warn};
use uuid::Uuid;
use zenoh::bytes::ZBytes;
use zenoh::Session;

use crate::error::BridgeError;

const READ_BUF: usize = 32 * 1024;
const ACK_TIMEOUT: Duration = Duration::from_secs(10);
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);

/// Per-direction byte totals observed by `run_bridge`. The gateway
/// uses these to finalise the `tunnel.session` audit row at close.
#[derive(Debug, Default, Clone, Copy)]
pub struct BridgeBytes {
    /// gateway→device (TCP read on the public listener, pumped onto
    /// the Zenoh publisher).
    pub up: u64,
    /// device→gateway (Zenoh subscriber payloads, pumped onto the
    /// public TCP write half).
    pub down: u64,
}

/// Agent side: accept a connect query, open a local TCP socket, and
/// run the byte bridge until either side closes. On success returns
/// the per-stream `request_id` (matched on the gateway side, useful
/// for diag attribution) and the originating peer address the
/// gateway captured before issuing the query.
pub async fn accept_bridge(
    session: &Session,
    org: &str,
    zid: &Zid,
    port: u16,
    query: zenoh::query::Query,
) -> Result<(Uuid, Option<String>), BridgeError> {
    let payload = query
        .payload()
        .map(|p| p.to_bytes().to_vec())
        .unwrap_or_default();
    let req: ConnectRequest =
        serde_json::from_slice(&payload).map_err(hackline_proto::error::ProtoError::Json)?;
    let request_id = req.request_id;
    let peer = req.peer.clone();

    debug!(%request_id, port, "accepting bridge");

    let tcp = match TcpStream::connect(format!("127.0.0.1:{port}")).await {
        Ok(s) => s,
        Err(e) => {
            let ack = ConnectAck {
                request_id,
                ok: false,
                message: Some(format!("tcp connect failed: {e}")),
            };
            let _ = query
                .reply(
                    keyexpr::connect(org, zid, port),
                    serde_json::to_vec(&ack).unwrap(),
                )
                .await;
            return Err(BridgeError::Io(e));
        }
    };

    let ack = ConnectAck {
        request_id,
        ok: true,
        message: None,
    };
    query
        .reply(
            keyexpr::connect(org, zid, port),
            serde_json::to_vec(&ack).unwrap(),
        )
        .await
        .map_err(BridgeError::Zenoh)?;

    // Drop the query so Zenoh sends the "final reply" frame — without this
    // the gateway's get() hangs until its internal timeout fires.
    drop(query);

    let ke_from_gw = keyexpr::stream_gw(org, zid, &request_id);
    let ke_to_gw = keyexpr::stream_dev(org, zid, &request_id);

    run_bridge(session, tcp, &ke_from_gw, &ke_to_gw).await?;
    Ok((request_id, peer))
}

/// Gateway side: issue a connect query, wait for the ack, and run
/// the byte bridge on the paired pub/sub channels.
pub async fn initiate_bridge(
    session: &Session,
    org: &str,
    zid: &Zid,
    port: u16,
    tcp: TcpStream,
    peer_addr: Option<String>,
) -> Result<BridgeBytes, BridgeError> {
    let (_request_id, bytes) =
        initiate_bridge_with_id(session, org, zid, port, tcp, peer_addr).await?;
    Ok(bytes)
}

/// Like `initiate_bridge` but also returns the `request_id` used on
/// the connect query so callers (the gateway TCP listener and the
/// HTTP host-router) can stamp it onto the `tunnel.session` audit
/// row alongside the byte counters.
pub async fn initiate_bridge_with_id(
    session: &Session,
    org: &str,
    zid: &Zid,
    port: u16,
    tcp: TcpStream,
    peer_addr: Option<String>,
) -> Result<(Uuid, BridgeBytes), BridgeError> {
    initiate_bridge_io_with_id(session, org, zid, port, tcp, peer_addr).await
}

/// Generic variant for callers that have already wrapped the accepted
/// socket in another protocol layer (the gateway uses this for
/// TLS-terminated tunnel listeners, where the wrapped value is a
/// `tokio_rustls::server::TlsStream<TcpStream>`). Keeping the TCP
/// signature intact above means the device-side `accept_bridge` and
/// every existing call site stay unchanged.
pub async fn initiate_bridge_io_with_id<S>(
    session: &Session,
    org: &str,
    zid: &Zid,
    port: u16,
    io: S,
    peer_addr: Option<String>,
) -> Result<(Uuid, BridgeBytes), BridgeError>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let request_id = Uuid::new_v4();
    let req = ConnectRequest {
        request_id,
        peer: peer_addr,
    };

    debug!(%request_id, %zid, port, "initiating bridge");

    let ke = keyexpr::connect(org, zid, port);
    let replies = session
        .get(&ke)
        .payload(ZBytes::from(serde_json::to_vec(&req).unwrap()))
        .timeout(QUERY_TIMEOUT)
        .await
        .map_err(BridgeError::Zenoh)?;

    let reply = tokio::time::timeout(ACK_TIMEOUT, replies.recv_async())
        .await
        .map_err(|_| BridgeError::AckTimeout)?
        .map_err(BridgeError::Zenoh)?;

    drop(replies);

    let ack_bytes = reply
        .result()
        .map_err(|e| BridgeError::Rejected(format!("{e:?}")))?
        .payload()
        .to_bytes()
        .to_vec();
    let ack: ConnectAck =
        serde_json::from_slice(&ack_bytes).map_err(hackline_proto::error::ProtoError::Json)?;

    if !ack.ok {
        return Err(BridgeError::Rejected(ack.message.unwrap_or_default()));
    }

    let ke_to_agent = keyexpr::stream_gw(org, zid, &request_id);
    let ke_from_agent = keyexpr::stream_dev(org, zid, &request_id);

    let bytes = run_bridge(session, io, &ke_from_agent, &ke_to_agent).await?;
    Ok((request_id, bytes))
}

/// Pump bytes between an arbitrary `AsyncRead + AsyncWrite` and a
/// Zenoh pub/sub pair until either side closes. Generic so the
/// gateway can pass a raw `TcpStream` for plain tunnels and a
/// `tokio_rustls::server::TlsStream<TcpStream>` for TLS-terminated
/// ones without duplicating the pump.
async fn run_bridge<S>(
    session: &Session,
    io: S,
    subscribe_ke: &str,
    publish_ke: &str,
) -> Result<BridgeBytes, BridgeError>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    // `tokio::io::split` works on any `AsyncRead + AsyncWrite`, unlike
    // `TcpStream::into_split` which is `TcpStream`-specific. The split
    // halves carry an internal lock; for IO that's only ever touched
    // from one direction at a time per half (which is the case here:
    // the read half stays in the tcp→zenoh task, the write half stays
    // in the zenoh→tcp task) the locking is uncontended.
    let (mut read_half, mut write_half) = tokio::io::split(io);
    let up = Arc::new(AtomicU64::new(0));
    let down = Arc::new(AtomicU64::new(0));

    let publisher = session
        .declare_publisher(publish_ke.to_owned())
        .await
        .map_err(BridgeError::Zenoh)?;

    let subscriber = session
        .declare_subscriber(subscribe_ke.to_owned())
        .await
        .map_err(BridgeError::Zenoh)?;

    let pub_ke = publish_ke.to_owned();
    let up_in = up.clone();

    // TCP → Zenoh
    let tcp_to_zenoh = tokio::spawn(async move {
        let mut buf = vec![0u8; READ_BUF];
        loop {
            match read_half.read(&mut buf).await {
                Ok(0) => {
                    debug!(ke = %pub_ke, "tcp read EOF, publishing close sentinel");
                    let _ = publisher.put(ZBytes::from(Vec::<u8>::new())).await;
                    break;
                }
                Ok(n) => {
                    up_in.fetch_add(n as u64, Ordering::Relaxed);
                    if publisher
                        .put(ZBytes::from(buf[..n].to_vec()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(e) => {
                    warn!("tcp read error: {e}");
                    let _ = publisher.put(ZBytes::from(Vec::<u8>::new())).await;
                    break;
                }
            }
        }
    });

    let sub_ke = subscribe_ke.to_owned();
    let down_in = down.clone();

    // Zenoh → TCP
    let zenoh_to_tcp = tokio::spawn(async move {
        while let Ok(sample) = subscriber.recv_async().await {
            let bytes = sample.payload().to_bytes();
            if bytes.is_empty() {
                debug!(ke = %sub_ke, "received close sentinel");
                break;
            }
            down_in.fetch_add(bytes.len() as u64, Ordering::Relaxed);
            if write_half.write_all(&bytes).await.is_err() {
                break;
            }
        }
    });

    let _ = tokio::try_join!(tcp_to_zenoh, zenoh_to_tcp);
    Ok(BridgeBytes {
        up: up.load(Ordering::Relaxed),
        down: down.load(Ordering::Relaxed),
    })
}
