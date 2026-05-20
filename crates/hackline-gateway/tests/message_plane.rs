//! End-to-end test: device publishes via `hackline_client`, the
//! gateway fan-in subscriber persists into the `events` / `logs`
//! ring buffers and rebroadcasts on the in-process bus. Runs two
//! Zenoh peers in one process on loopback ports, exactly like the
//! Phase 0 spike — no router required.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hackline_client::ClientSession;
use hackline_gateway::db::{events, logs, migrations, pool};
use hackline_gateway::events_bus::{MsgBus, MsgEvent};
use hackline_gateway::msg_fanin;
use hackline_proto::msg::LogLevel;
use hackline_proto::Zid;
use serde_json::json;

fn peer_config(listen_port: u16, connect_port: u16) -> anyhow::Result<zenoh::Config> {
    let mut cfg = zenoh::Config::default();
    cfg.insert_json5("mode", r#""peer""#).unwrap();
    cfg.insert_json5(
        "listen/endpoints",
        &format!(r#"["tcp/127.0.0.1:{listen_port}"]"#),
    )
    .unwrap();
    cfg.insert_json5(
        "connect/endpoints",
        &format!(r#"["tcp/127.0.0.1:{connect_port}"]"#),
    )
    .unwrap();
    cfg.insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    Ok(cfg)
}

async fn unique_loopback_ports() -> (u16, u16) {
    // OS-assigned ephemeral ports so concurrent test runs don't
    // collide. Bind, capture the port, drop the socket immediately
    // before Zenoh re-binds.
    let a = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let b = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let pa = a.local_addr().unwrap().port();
    let pb = b.local_addr().unwrap().port();
    drop(a);
    drop(b);
    (pa, pb)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn event_round_trip() -> anyhow::Result<()> {
    let tmp = tempdir();
    let db_path = tmp.join("gateway.db");
    let db = pool::open(&db_path)?;
    {
        let conn = db.get()?;
        migrations::run(&conn)?;
    }

    let device_zid = "aa11";
    {
        let conn = db.get()?;
        conn.execute(
            "INSERT INTO devices (zid, label, created_at) VALUES (?1, 'test', unixepoch())",
            rusqlite::params![device_zid],
        )?;
    }

    let (port_a, port_b) = unique_loopback_ports().await;
    let gw_session = Arc::new(hackline_core::session::open(peer_config(port_a, port_b)?).await?);
    let dev_session = Arc::new(hackline_core::session::open(peer_config(port_b, port_a)?).await?);

    // Let the two peers discover each other before declaring sub.
    tokio::time::sleep(Duration::from_millis(400)).await;

    let bus = MsgBus::new();
    let mut rx = bus.subscribe();
    let _fanin = msg_fanin::spawn(
        gw_session.clone(),
        db.clone(),
        bus.clone(),
        hackline_gateway::metrics::Metrics::new(),
    )
    .await?;

    tokio::time::sleep(Duration::from_millis(200)).await;

    let zid = Zid::new(device_zid)?;
    let client = ClientSession::from_session(dev_session.clone(), "default", zid);
    client
        .publish_event("graph.slot.temp.changed", json!({ "v": 21.4 }))
        .await?;
    client
        .publish_log(LogLevel::Warn, "audit.entry", json!({ "msg": "hello" }))
        .await?;

    let mut saw_event = false;
    let mut saw_log = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline && !(saw_event && saw_log) {
        match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
            Ok(Ok(MsgEvent::Event { row, .. })) => {
                assert_eq!(row.topic, "graph.slot.temp.changed");
                assert_eq!(row.payload["v"], 21.4);
                saw_event = true;
            }
            Ok(Ok(MsgEvent::Log { row, .. })) => {
                assert_eq!(row.topic, "audit.entry");
                assert_eq!(row.level, "warn");
                saw_log = true;
            }
            _ => continue,
        }
    }
    assert!(saw_event, "did not receive event broadcast");
    assert!(saw_log, "did not receive log broadcast");

    // Cursor API sees the same rows.
    let conn = db.get()?;
    let evs = events::list(&conn, 1, None, None, None, None, 10)?;
    assert_eq!(evs.len(), 1);
    assert_eq!(evs[0].topic, "graph.slot.temp.changed");

    let lgs = logs::list(&conn, 1, None, None, None, None, None, 10)?;
    assert_eq!(lgs.len(), 1);
    assert_eq!(lgs[0].level, "warn");

    Ok(())
}

fn tempdir() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "hackline-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}
