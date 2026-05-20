//! `hackline-gateway serve` — boot the gateway and bind every listener.

use std::path::PathBuf;
use std::sync::Arc;

use hackline_gateway::api;
use hackline_gateway::cmd_delivery::{self, CmdNotifier};
use hackline_gateway::config::GatewayConfig;
use hackline_gateway::db::{claim, migrations, pool};
use hackline_gateway::events_bus::MsgBus;
use hackline_gateway::liveliness;
use hackline_gateway::metrics::Metrics;
use hackline_gateway::msg_fanin;
use hackline_gateway::state::AppState;
use hackline_gateway::tunnel::{http_router, manager};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("gateway.toml"));

    let cfg = GatewayConfig::load(&config_path)?;

    init_tracing(&cfg.log.level, &cfg.log.format);

    info!(config = ?config_path, "starting hackline-gateway");

    let db_path = cfg.database.as_deref().unwrap_or("gateway.db");
    let db = pool::open(std::path::Path::new(db_path))?;

    {
        let conn = db.get()?;
        migrations::run(&conn)?;
    }
    info!(db = db_path, "database ready");

    {
        let conn = db.get()?;
        let scheme = if cfg.tls.is_some() { "https" } else { "http" };
        match claim::ensure_pending(&conn)? {
            Some(token) => {
                info!("gateway unclaimed — claim token printed below");
                println!("\n  CLAIM TOKEN: {token}\n");
                println!(
                    "  Use: hackline login --server {scheme}://{listen_addr} --token {token}\n",
                    listen_addr = cfg.listen.as_deref().unwrap_or("127.0.0.1:8080")
                );
            }
            None => {
                info!("gateway already claimed (or claim pending from previous boot)");
            }
        }
    }

    let zenoh_cfg = cfg.to_zenoh_config()?;
    let session = Arc::new(hackline_core::session::open(zenoh_cfg).await?);
    info!(zid = %session.zid(), "zenoh session open");

    let (tunnel_tx, tunnel_rx) = tokio::sync::mpsc::channel(64);

    let metrics = Metrics::new();

    let msg_bus = MsgBus::new();
    let _fanin_handles = msg_fanin::spawn(
        session.clone(),
        db.clone(),
        msg_bus.clone(),
        metrics.clone(),
    )
    .await?;

    let cmd_notifier = CmdNotifier::new();
    let _cmd_handles = cmd_delivery::spawn(
        session.clone(),
        db.clone(),
        cmd_notifier.clone(),
        metrics.clone(),
    )
    .await?;

    let _liveliness_handle = liveliness::spawn(session.clone(), db.clone()).await?;

    let state = AppState {
        db: db.clone(),
        zenoh: session.clone(),
        tunnel_tx,
        msg_bus,
        cmd_notifier,
        metrics: metrics.clone(),
        rtt_cache: hackline_gateway::rtt_cache::RttCache::new(std::time::Duration::from_secs(1)),
    };

    let listen_addr = cfg.listen.as_deref().unwrap_or("127.0.0.1:8080");
    let app = api::router::build(state);

    // Resolve TLS state (if configured). The feature gate ensures the
    // tls module is only compiled when the `tls` feature is active;
    // at runtime an absent `[tls]` block means plain TCP.
    #[cfg(feature = "tls")]
    let tls_state = match &cfg.tls {
        Some(tls_cfg) => Some(hackline_gateway::tls::init(tls_cfg).await?),
        None => None,
    };
    #[cfg(not(feature = "tls"))]
    let tls_state: Option<()> = {
        if cfg.tls.is_some() {
            anyhow::bail!("[tls] block in config but gateway compiled without `tls` feature");
        }
        None
    };

    info!(
        addr = listen_addr,
        tls = tls_state.is_some(),
        "REST API listening"
    );

    // The same TLS material that terminates the REST listener also
    // wraps tunnel TCP sockets. Cloning out the acceptor swap here
    // keeps the `tls_state` value available for `serve_rest` below
    // while the tunnel manager owns its own clone of the swap.
    #[cfg(feature = "tls")]
    let tunnel_tls: hackline_gateway::tunnel::tcp_listener::TunnelTls =
        tls_state.as_ref().map(|s| s.acceptor.clone());
    #[cfg(not(feature = "tls"))]
    let tunnel_tls: hackline_gateway::tunnel::tcp_listener::TunnelTls = None;

    // ACME renewer: a background task that re-acquires the cached
    // cert before it expires and hot-swaps it into both the REST
    // RustlsConfig and the tunnel acceptor. `None` for non-ACME modes;
    // the corresponding select arm collapses to `pending()`.
    #[cfg(feature = "tls")]
    let renewal_task = match (tls_state.as_ref(), cfg.tls.as_ref()) {
        (Some(state), Some(tls_cfg)) => {
            hackline_gateway::tls::spawn_renewal(state.clone(), tls_cfg.clone())
        }
        _ => None,
    };
    #[cfg(not(feature = "tls"))]
    let renewal_task: Option<()> = None;

    // Run axum and tunnel manager concurrently. The fan-in subscriber
    // tasks own their own loops and don't need to be in the select —
    // dropping their handles when the process exits is enough.
    // Optional HTTP host-routing listener — proxies
    // `device-<id>.<base>` HTTPS-fronted requests into the matching
    // `http` tunnel. Off unless the operator configured `http_listen`.
    let http_router_fut = {
        let db = db.clone();
        let session = session.clone();
        let metrics = metrics.clone();
        let addr = cfg.http_listen.clone();
        async move {
            match addr {
                Some(a) => http_router::run(db, session, metrics, &a).await,
                None => std::future::pending::<Result<(), _>>().await,
            }
        }
    };

    tokio::select! {
        result = serve_rest(listen_addr, app, &tls_state) => {
            result?;
        }
        result = manager::run(db, session, metrics, tunnel_rx, tunnel_tls) => {
            result?;
        }
        result = http_router_fut => {
            result?;
        }
        result = renewal_arm(renewal_task) => {
            // The renewer loops forever; if the join future resolves
            // it's either a panic (JoinError) or the loop returned
            // an unrecoverable error. Either way: surface and exit.
            result?;
        }
    }

    Ok(())
}

#[cfg(feature = "tls")]
async fn renewal_arm(
    handle: Option<tokio::task::JoinHandle<Result<(), hackline_gateway::error::GatewayError>>>,
) -> anyhow::Result<()> {
    match handle {
        Some(h) => {
            let res = h.await?;
            res?;
            Ok(())
        }
        None => std::future::pending().await,
    }
}

#[cfg(not(feature = "tls"))]
async fn renewal_arm(_: Option<()>) -> anyhow::Result<()> {
    std::future::pending().await
}

/// Serve the REST API — plain TCP or TLS depending on config.
#[cfg(feature = "tls")]
async fn serve_rest(
    addr: &str,
    app: axum::Router,
    tls: &Option<hackline_gateway::tls::TlsState>,
) -> anyhow::Result<()> {
    match tls {
        Some(tls_state) => {
            let addr: std::net::SocketAddr = addr.parse()?;
            axum_server::bind_rustls(addr, tls_state.axum_config.clone())
                .serve(app.into_make_service())
                .await?;
            Ok(())
        }
        None => {
            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, app).await?;
            Ok(())
        }
    }
}

#[cfg(not(feature = "tls"))]
async fn serve_rest(addr: &str, app: axum::Router, _tls: &Option<()>) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
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
