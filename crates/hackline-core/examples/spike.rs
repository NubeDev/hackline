//! Spike: two Zenoh peers in one process, proving the TCP↔Zenoh bridge
//! works end to end.
//!
//! Starts a trivial TCP echo server on 127.0.0.1:9998, then wires:
//!   connect to 127.0.0.1:9999 → (gateway peer) → Zenoh → (agent peer) → 127.0.0.1:9998
//!
//! Test: `nc 127.0.0.1 9999`, type lines, see them echoed back.
//!       Or: `cargo run --example spike` in one terminal, `nc` in another.

use anyhow::Result;
use hackline_core::bridge;
use hackline_proto::Zid;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tracing::{info, warn};

const AGENT_ZID: &str = "aa01";
const LOCAL_PORT: u16 = 9998;
const PUBLIC_PORT: u16 = 9999;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info,hackline_core=debug")
        .init();

    let zid = Zid::new(AGENT_ZID)?;

    // 1. Start a trivial echo server on LOCAL_PORT.
    tokio::spawn(run_echo_server(LOCAL_PORT));

    // 2. Open two Zenoh sessions: one "agent", one "gateway". Both peers,
    //    connected via loopback.
    let agent_cfg = peer_config(7447, 7448)?;
    let gw_cfg = peer_config(7448, 7447)?;

    let agent_session = Arc::new(hackline_core::session::open(agent_cfg).await?);
    let gw_session = Arc::new(hackline_core::session::open(gw_cfg).await?);

    // Give sessions a moment to discover each other.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // 3. Agent side: declare queryable, accept bridges.
    let agent_s = agent_session.clone();
    let agent_zid = zid.clone();
    tokio::spawn(async move {
        if let Err(e) = run_agent(&agent_s, &agent_zid, LOCAL_PORT).await {
            warn!("agent error: {e}");
        }
    });

    // 4. Gateway side: listen on PUBLIC_PORT, initiate bridges.
    info!("listening on 127.0.0.1:{PUBLIC_PORT} — connect with: nc 127.0.0.1 {PUBLIC_PORT}");
    run_gateway(&gw_session, &zid, PUBLIC_PORT, LOCAL_PORT).await?;

    Ok(())
}

async fn run_agent(session: &zenoh::Session, zid: &Zid, port: u16) -> Result<()> {
    let ke = hackline_proto::keyexpr::connect("default", zid, port);
    let queryable = session
        .declare_queryable(&ke)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    info!(ke = %ke, "agent queryable ready");

    loop {
        match queryable.recv_async().await {
            Ok(query) => {
                let s = session.clone();
                let z = zid.clone();
                tokio::spawn(async move {
                    if let Err(e) = bridge::accept_bridge(&s, "default", &z, port, query).await {
                        warn!("accept_bridge error: {e}");
                    }
                });
            }
            Err(e) => {
                warn!("queryable recv error: {e}");
                break;
            }
        }
    }
    Ok(())
}

async fn run_gateway(
    session: &zenoh::Session,
    zid: &Zid,
    public_port: u16,
    device_port: u16,
) -> Result<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{public_port}")).await?;
    loop {
        let (tcp, addr) = listener.accept().await?;
        info!(%addr, "accepted connection");
        let s = session.clone();
        let z = zid.clone();
        tokio::spawn(async move {
            if let Err(e) =
                bridge::initiate_bridge(&s, "default", &z, device_port, tcp, Some(addr.to_string()))
                    .await
            {
                warn!(%addr, "initiate_bridge error: {e}");
            }
            info!(%addr, "connection closed");
        });
    }
}

async fn run_echo_server(port: u16) {
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .expect("bind echo server");
    info!(port, "echo server listening");
    loop {
        let (stream, _) = listener.accept().await.expect("accept");
        tokio::spawn(async move {
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let echoed = format!("{line}\n");
                if writer.write_all(echoed.as_bytes()).await.is_err() {
                    break;
                }
            }
        });
    }
}

fn peer_config(listen_port: u16, connect_port: u16) -> Result<zenoh::Config> {
    let mut config = zenoh::Config::default();
    config
        .insert_json5("mode", r#""peer""#)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    config
        .insert_json5(
            "listen/endpoints",
            &format!(r#"["tcp/127.0.0.1:{listen_port}"]"#),
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    config
        .insert_json5(
            "connect/endpoints",
            &format!(r#"["tcp/127.0.0.1:{connect_port}"]"#),
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    // Disable multicast scouting to avoid interference.
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(config)
}
