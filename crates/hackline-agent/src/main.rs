//! `hackline-agent` binary entry point. Argv parsing and the logging
//! subscriber install live here; everything else is a library function.

mod config;
mod connect;
mod diag;
mod error;
mod info;
mod liveliness;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use config::AgentConfig;
use diag::{ConnectionEvent, DiagState};
use hackline_proto::Zid;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Captured before any awaits so `info::spawn` can derive a
    // monotonic uptime that doesn't include arbitrary boot delays.
    let started_at = Instant::now();

    let config_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("agent.toml"));

    let cfg = AgentConfig::load(&config_path)?;

    init_tracing(&cfg.log.level, &cfg.log.format);

    info!(config = ?config_path, ports = ?cfg.allowed_ports, "starting hackline-agent");

    let zid = Zid::new(&cfg.zid)?;

    let zenoh_cfg = cfg.to_zenoh_config()?;
    let session = Arc::new(hackline_core::session::open(zenoh_cfg).await?);

    info!(%zid, zenoh_zid = %session.zid(), "zenoh session open");

    // Liveliness token is held for the lifetime of the agent — the
    // gateway's liveliness watcher upserts the device row on the
    // `Put` sample and bumps `last_seen_at`. Dropped on process exit
    // (or session loss), at which point the gateway gets a `Delete`.
    let _liveliness = liveliness::declare(&session, &cfg.org, &zid).await?;

    // Diag state owns the session arc and the active-port map. The
    // connect loop and the diag HTTP handlers both go through it,
    // so adding/removing ports at runtime stays consistent.
    let diag_state = Arc::new(DiagState::new(
        cfg.zid.clone(),
        cfg.label.clone(),
        cfg.org.clone(),
        session.zid().to_string(),
        cfg.zenoh.mode.clone(),
        cfg.zenoh.listen.clone(),
        cfg.zenoh.connect.clone(),
        session.clone(),
        zid.clone(),
    ));

    // Declare the startup set of port queryables. Each one runs as
    // a detached task owned by `DiagState`; removing a port from
    // the UI aborts that task and undeclares the keyexpr.
    connect::start_initial_ports(diag_state.clone(), &cfg.allowed_ports).await?;

    if cfg.diag.enabled {
        let addr = diag::parse_bind(&cfg.diag.bind)?;
        let s = diag_state.clone();
        tokio::spawn(async move {
            if let Err(e) = diag::serve(addr, s).await {
                warn!("diag UI failed to start: {e}");
            }
        });
    }

    // Background queryable for `hackline/<org>/<zid>/info`. Detached;
    // a failure here logs but does not bring down the agent — connect
    // queryables are the load-bearing path.
    let _info_handle = info::spawn(
        session.clone(),
        cfg.org.clone(),
        zid.clone(),
        cfg.allowed_ports.clone(),
        started_at,
    );

    // Block on Ctrl-C so the liveliness token and active-port
    // queryables stay declared until the operator stops the process.
    if let Err(e) = tokio::signal::ctrl_c().await {
        warn!("ctrl_c listener failed: {e}");
    }
    info!("shutting down");
    Ok(())
}

/// Best-effort wall-clock seconds for diag log entries. Falls back to
/// 0 if the system clock is before the epoch (it isn't, but the
/// `Result` exists, so we handle it without panicking the agent).
pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Convenience constructor used by `connect.rs` so the bridge
/// callsite stays one line.
pub(crate) fn conn_event(
    port: u16,
    request_id: String,
    peer: Option<String>,
    outcome: &str,
) -> ConnectionEvent {
    ConnectionEvent {
        at_unix: now_unix(),
        port,
        request_id,
        peer,
        outcome: outcome.to_string(),
    }
}

fn init_tracing(level: &str, format: &str) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level));

    match format {
        "json" => {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(env_filter)
                .init();
        }
        _ => {
            tracing_subscriber::fmt().with_env_filter(env_filter).init();
        }
    }
}
