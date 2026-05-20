//! End-to-end Goal 5 round-trips. Two Zenoh peers in one process —
//! "gateway" + "device". The gateway hosts the cmd-delivery loop
//! and the synchronous api-call path; the device hosts a
//! `ClientSession` with `subscribe_cmd` and `serve_api`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hackline_client::ClientSession;
use hackline_gateway::cmd_delivery::{self, CmdNotifier};
use hackline_gateway::db::{cmd_outbox, migrations, pool};
use hackline_proto::msg::{ApiReply, ApiRequest, CmdResult};
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
    let a = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let b = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let pa = a.local_addr().unwrap().port();
    let pb = b.local_addr().unwrap().port();
    drop(a);
    drop(b);
    (pa, pb)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cmd_round_trip() -> anyhow::Result<()> {
    let tmp = tempdir();
    let db_path = tmp.join("gateway.db");
    let db = pool::open(&db_path)?;
    {
        let conn = db.get()?;
        migrations::run(&conn)?;
    }

    let device_zid = "bb22";
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

    tokio::time::sleep(Duration::from_millis(400)).await;

    let notifier = CmdNotifier::new();
    let _handles = cmd_delivery::spawn(
        gw_session.clone(),
        db.clone(),
        notifier.clone(),
        hackline_gateway::metrics::Metrics::new(),
    )
    .await?;

    let zid = Zid::new(device_zid)?;
    let client = ClientSession::from_session(dev_session.clone(), "default", zid.clone());

    let stream = client.subscribe_cmd("block.install").await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let cmd_id = uuid::Uuid::new_v4();
    let cmd_id_str = cmd_id.to_string();
    {
        let mut conn = db.get()?;
        cmd_outbox::enqueue(
            &mut conn,
            &cmd_id_str,
            1,
            "block.install",
            "application/json",
            &json!({ "block": "demo" }),
            now_ms(),
            now_ms() + 60_000,
        )?;
    }
    notifier.notify();

    // Device-side handler: wait for the cmd, ack it.
    let handler = tokio::spawn(async move {
        let handle = stream.recv().await.expect("cmd handle");
        assert_eq!(handle.topic(), "block.install");
        assert_eq!(handle.payload()["block"], "demo");
        handle.ack(CmdResult::Done).await.unwrap();
    });

    // Poll the outbox until the ack lands or we time out.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut acked = false;
    while tokio::time::Instant::now() < deadline {
        let conn = db.get()?;
        if let Some(row) = cmd_outbox::get_by_cmd_id(&conn, &cmd_id_str)? {
            if row.ack_result.as_deref() == Some("done") {
                acked = true;
                break;
            }
        }
        drop(conn);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let _ = handler.await;
    assert!(acked, "cmd ack never reached the outbox");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn api_round_trip() -> anyhow::Result<()> {
    let tmp = tempdir();
    let db_path = tmp.join("gateway.db");
    let db = pool::open(&db_path)?;
    {
        let conn = db.get()?;
        migrations::run(&conn)?;
    }

    let device_zid = "cc33";
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

    tokio::time::sleep(Duration::from_millis(400)).await;

    let zid = Zid::new(device_zid)?;
    let client = ClientSession::from_session(dev_session.clone(), "default", zid.clone());
    let _serving = client
        .serve_api("ping", |req: ApiRequest| async move {
            Ok(ApiReply::json(json!({ "pong": req.payload })))
        })
        .await?;

    tokio::time::sleep(Duration::from_millis(200)).await;

    let ke = hackline_proto::keyexpr::msg_api("default", &zid, "ping");
    let req = ApiRequest {
        content_type: "application/json".into(),
        payload: json!({"x": 1}),
    };
    let replies = gw_session
        .get(&ke)
        .payload(zenoh::bytes::ZBytes::from(serde_json::to_vec(&req)?))
        .timeout(Duration::from_secs(2))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let reply = tokio::time::timeout(Duration::from_secs(2), replies.recv_async())
        .await?
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let bytes = reply.result().unwrap().payload().to_bytes().to_vec();
    let api: ApiReply = serde_json::from_slice(&bytes)?;
    assert_eq!(api.payload["pong"]["x"], 1);

    Ok(())
}

fn tempdir() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "hackline-cmd-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
