//! TLS termination for the gateway (gated behind the `tls` feature).
//!
//! Three modes:
//! - **ACME**: obtains certs from Let's Encrypt via HTTP-01 challenge.
//! - **Manual**: loads PEM cert+key from disk.
//! - **Self-signed**: generates an ephemeral cert on startup.
//!
//! All modes produce a `TlsState` that owns:
//! - `axum_config`: the `RustlsConfig` axum-server's TLS listener uses
//!   for the REST API. Hot-reloadable via `RustlsConfig::reload_from_pem`.
//! - `acceptor`: an `Arc<ArcSwap<TlsAcceptor>>` shared with every
//!   tunnel TCP listener. Renewal swaps the inner `Arc` so the very
//!   next handshake on any listener picks up the new cert without
//!   touching in-flight connections.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use axum_server::tls_rustls::RustlsConfig;
use tokio_rustls::TlsAcceptor;
use tracing::{info, warn};

use crate::config::{TlsConfig, TlsMode};
use crate::error::GatewayError;

/// Shared TLS state. Cloning is cheap: `RustlsConfig` is internally
/// `Arc`, the acceptor swap is an `Arc`, and the cache dir is an
/// `Arc<PathBuf>`. Every clone observes the same renewals.
#[derive(Clone)]
pub struct TlsState {
    pub axum_config: RustlsConfig,
    pub acceptor: Arc<ArcSwap<TlsAcceptor>>,
    /// Where ACME caches account + cert PEMs. `None` for non-ACME
    /// modes; the renewer is only spawned when this is `Some`.
    pub acme_cache_dir: Option<Arc<PathBuf>>,
}

/// Build TLS state from the parsed `[tls]` config block.
pub async fn init(cfg: &TlsConfig) -> Result<TlsState, GatewayError> {
    let mode = cfg.mode()?;
    match mode {
        TlsMode::SelfSigned => init_self_signed(cfg).await,
        TlsMode::Manual => init_manual(cfg).await,
        TlsMode::Acme => init_acme(cfg).await,
    }
}

// ── Self-signed (dev only) ──────────────────────────────────────────

async fn init_self_signed(_cfg: &TlsConfig) -> Result<TlsState, GatewayError> {
    warn!("TLS: generating self-signed certificate (not for production)");

    let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    let cert_params = rcgen::CertificateParams::new(subject_alt_names)
        .map_err(|e| GatewayError::Config(format!("rcgen params: {e}")))?;
    let key_pair = rcgen::KeyPair::generate()
        .map_err(|e| GatewayError::Config(format!("rcgen keygen: {e}")))?;
    let cert = cert_params
        .self_signed(&key_pair)
        .map_err(|e| GatewayError::Config(format!("rcgen self_signed: {e}")))?;

    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    build_state_from_pem(cert_pem.as_bytes(), key_pem.as_bytes(), None).await
}

// ── Manual certs ────────────────────────────────────────────────────

async fn init_manual(cfg: &TlsConfig) -> Result<TlsState, GatewayError> {
    let cert_path = cfg
        .cert_path
        .as_deref()
        .ok_or_else(|| GatewayError::Config("[tls] manual mode: cert_path required".into()))?;
    let key_path = cfg
        .key_path
        .as_deref()
        .ok_or_else(|| GatewayError::Config("[tls] manual mode: key_path required".into()))?;

    info!(
        cert = cert_path,
        key = key_path,
        "TLS: loading manual certs"
    );

    let cert_pem = tokio::fs::read(cert_path)
        .await
        .map_err(|e| GatewayError::Config(format!("read {cert_path}: {e}")))?;
    let key_pem = tokio::fs::read(key_path)
        .await
        .map_err(|e| GatewayError::Config(format!("read {key_path}: {e}")))?;

    build_state_from_pem(&cert_pem, &key_pem, None).await
}

// ── ACME (Let's Encrypt) ────────────────────────────────────────────

async fn init_acme(cfg: &TlsConfig) -> Result<TlsState, GatewayError> {
    let domain = cfg.acme_domain.as_deref().unwrap();

    let cache_dir = resolve_cache_dir(cfg);
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .map_err(|e| GatewayError::Config(format!("create {cache_dir:?}: {e}")))?;

    let (cert_file, key_file) = cert_paths(&cache_dir, domain);

    // If we have a cached cert and it's still well outside the renewal
    // window, use it; otherwise re-acquire before serving — booting
    // with a soon-to-expire cert would defer the problem only as long
    // as the renewer's first tick.
    let (cert_pem, key_pem) = if cert_file.exists() && key_file.exists() {
        let cert_pem = tokio::fs::read(&cert_file)
            .await
            .map_err(|e| GatewayError::Config(format!("read {cert_file:?}: {e}")))?;
        let key_pem = tokio::fs::read(&key_file)
            .await
            .map_err(|e| GatewayError::Config(format!("read {key_file:?}: {e}")))?;

        if needs_renewal(&cert_pem, cfg.acme_renew_before_days)? {
            info!(
                domain,
                "TLS/ACME: cached cert within renewal window, re-acquiring"
            );
            acquire_and_cache(cfg, &cache_dir).await?
        } else {
            info!(cert = ?cert_file, "TLS/ACME: using cached certificate");
            (cert_pem, key_pem)
        }
    } else {
        acquire_and_cache(cfg, &cache_dir).await?
    };

    build_state_from_pem(&cert_pem, &key_pem, Some(Arc::new(cache_dir))).await
}

fn resolve_cache_dir(cfg: &TlsConfig) -> PathBuf {
    cfg.acme_cache_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::state_dir()
                .or_else(dirs::data_local_dir)
                .unwrap_or_else(|| PathBuf::from("."))
                .join("hackline")
                .join("acme")
        })
}

fn cert_paths(cache_dir: &Path, domain: &str) -> (PathBuf, PathBuf) {
    (
        cache_dir.join(format!("{domain}.cert.pem")),
        cache_dir.join(format!("{domain}.key.pem")),
    )
}

/// Run the ACME flow once, write cert + key to the cache dir, and
/// return the bytes for in-memory use. Used both at first boot and
/// by the renewer.
async fn acquire_and_cache(
    cfg: &TlsConfig,
    cache_dir: &Path,
) -> Result<(Vec<u8>, Vec<u8>), GatewayError> {
    let domain = cfg.acme_domain.as_deref().unwrap();
    let email = cfg.acme_email.as_deref().unwrap();
    let (cert_file, key_file) = cert_paths(cache_dir, domain);

    info!(
        domain,
        "TLS/ACME: requesting certificate from Let's Encrypt"
    );

    let directory_url = if cfg.acme_staging {
        instant_acme::LetsEncrypt::Staging.url()
    } else {
        instant_acme::LetsEncrypt::Production.url()
    };

    let account_file = cache_dir.join("account.json");
    let account = load_or_create_account(&account_file, directory_url, email).await?;

    let identifiers = [instant_acme::Identifier::Dns(domain.to_string())];
    let mut order = account
        .new_order(&instant_acme::NewOrder::new(&identifiers))
        .await
        .map_err(|e| GatewayError::Config(format!("ACME new_order: {e}")))?;

    let mut challenge_map = std::collections::HashMap::new();
    {
        let mut authz_stream = order.authorizations();
        while let Some(mut authz) = authz_stream
            .next()
            .await
            .transpose()
            .map_err(|e| GatewayError::Config(format!("ACME authorization: {e}")))?
        {
            let mut ch = authz
                .challenge(instant_acme::ChallengeType::Http01)
                .ok_or_else(|| GatewayError::Config("ACME: no HTTP-01 challenge offered".into()))?;
            let ka = ch.key_authorization();
            challenge_map.insert(ch.token.clone(), ka.as_str().to_string());
            ch.set_ready()
                .await
                .map_err(|e| GatewayError::Config(format!("ACME set_ready: {e}")))?;
        }
    }

    let challenge_server = spawn_challenge_server(Arc::new(challenge_map)).await?;

    let retries = instant_acme::RetryPolicy::default();
    order
        .poll_ready(&retries)
        .await
        .map_err(|e| GatewayError::Config(format!("ACME poll_ready: {e}")))?;

    let key_pem = order
        .finalize()
        .await
        .map_err(|e| GatewayError::Config(format!("ACME finalize: {e}")))?;

    let cert_chain = order
        .poll_certificate(&retries)
        .await
        .map_err(|e| GatewayError::Config(format!("ACME certificate: {e}")))?;

    challenge_server.abort();

    tokio::fs::write(&cert_file, cert_chain.as_bytes())
        .await
        .map_err(|e| GatewayError::Config(format!("write {cert_file:?}: {e}")))?;
    tokio::fs::write(&key_file, key_pem.as_bytes())
        .await
        .map_err(|e| GatewayError::Config(format!("write {key_file:?}: {e}")))?;

    info!(domain, "TLS/ACME: certificate obtained and cached");

    Ok((cert_chain.into_bytes(), key_pem.into_bytes()))
}

async fn load_or_create_account(
    path: &Path,
    directory_url: &str,
    email: &str,
) -> Result<instant_acme::Account, GatewayError> {
    let builder = instant_acme::Account::builder()
        .map_err(|e| GatewayError::Config(format!("ACME account builder: {e}")))?;

    if path.exists() {
        let json = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| GatewayError::Config(format!("read {path:?}: {e}")))?;
        let creds: instant_acme::AccountCredentials = serde_json::from_str(&json)
            .map_err(|e| GatewayError::Config(format!("parse account: {e}")))?;
        let account = builder
            .from_credentials(creds)
            .await
            .map_err(|e| GatewayError::Config(format!("load ACME account: {e}")))?;
        info!("ACME: loaded cached account");
        return Ok(account);
    }

    let (account, creds) = builder
        .create(
            &instant_acme::NewAccount {
                contact: &[&format!("mailto:{email}")],
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            directory_url.to_string(),
            None,
        )
        .await
        .map_err(|e| GatewayError::Config(format!("create ACME account: {e}")))?;

    let json = serde_json::to_string_pretty(&creds)
        .map_err(|e| GatewayError::Config(format!("serialize account: {e}")))?;
    tokio::fs::write(path, json.as_bytes())
        .await
        .map_err(|e| GatewayError::Config(format!("write {path:?}: {e}")))?;

    info!("ACME: created new account");
    Ok(account)
}

/// Ephemeral HTTP server on port 80 that responds to
/// `GET /.well-known/acme-challenge/<token>` with the key authorization.
async fn spawn_challenge_server(
    tokens: Arc<std::collections::HashMap<String, String>>,
) -> Result<tokio::task::JoinHandle<()>, GatewayError> {
    use axum::{extract::Path as AxumPath, routing::get, Router};

    let app = Router::new().route(
        "/.well-known/acme-challenge/{token}",
        get(move |AxumPath(token): AxumPath<String>| {
            let tokens = tokens.clone();
            async move {
                match tokens.get(&token) {
                    Some(proof) => (axum::http::StatusCode::OK, proof.clone()),
                    None => (axum::http::StatusCode::NOT_FOUND, String::new()),
                }
            }
        }),
    );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:80")
        .await
        .map_err(|e| GatewayError::Config(format!("bind :80 for ACME challenge: {e}")))?;
    info!("ACME: HTTP-01 challenge responder listening on :80");

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    Ok(handle)
}

// ── Shared helpers ──────────────────────────────────────────────────

async fn build_state_from_pem(
    cert_pem: &[u8],
    key_pem: &[u8],
    acme_cache_dir: Option<Arc<PathBuf>>,
) -> Result<TlsState, GatewayError> {
    let axum_config = RustlsConfig::from_pem(cert_pem.to_vec(), key_pem.to_vec())
        .await
        .map_err(|e| GatewayError::Config(format!("rustls config: {e}")))?;

    let acceptor = build_acceptor(cert_pem, key_pem)?;

    Ok(TlsState {
        axum_config,
        acceptor: Arc::new(ArcSwap::from_pointee(acceptor)),
        acme_cache_dir,
    })
}

fn build_acceptor(cert_pem: &[u8], key_pem: &[u8]) -> Result<TlsAcceptor, GatewayError> {
    let certs = rustls_pemfile::certs(&mut std::io::BufReader::new(cert_pem))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| GatewayError::Config(format!("parse cert PEM: {e}")))?;
    let key = rustls_pemfile::private_key(&mut std::io::BufReader::new(key_pem))
        .map_err(|e| GatewayError::Config(format!("parse key PEM: {e}")))?
        .ok_or_else(|| GatewayError::Config("no private key in PEM".into()))?;

    let mut server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| GatewayError::Config(format!("rustls server config: {e}")))?;
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}

impl TlsState {
    /// Atomically replace both the REST listener's rustls config and
    /// the tunnel acceptor with new cert material. In-flight TLS
    /// sessions on either side keep running on their existing keys
    /// (rustls does not require renegotiation on cert rotation); the
    /// next handshake picks up the new cert.
    pub async fn reload(&self, cert_pem: &[u8], key_pem: &[u8]) -> Result<(), GatewayError> {
        self.axum_config
            .reload_from_pem(cert_pem.to_vec(), key_pem.to_vec())
            .await
            .map_err(|e| GatewayError::Config(format!("rustls reload: {e}")))?;
        let new_acceptor = build_acceptor(cert_pem, key_pem)?;
        self.acceptor.store(Arc::new(new_acceptor));
        Ok(())
    }
}

// ── Cert expiry parsing ─────────────────────────────────────────────

/// Return the leaf cert's `notAfter` as Unix time in seconds. The
/// leaf is the first PEM-encoded `CERTIFICATE` block; ACME and most
/// CAs put the subject cert there with intermediates following.
pub fn cert_not_after_secs(cert_pem: &[u8]) -> Result<i64, GatewayError> {
    let pem = x509_parser::pem::Pem::iter_from_buffer(cert_pem)
        .next()
        .ok_or_else(|| GatewayError::Config("no PEM block in cert".into()))?
        .map_err(|e| GatewayError::Config(format!("parse PEM: {e}")))?;
    let (_, cert) = x509_parser::parse_x509_certificate(&pem.contents)
        .map_err(|e| GatewayError::Config(format!("parse x509: {e}")))?;
    Ok(cert.validity().not_after.timestamp())
}

/// `true` when the cached cert is within `renew_before_days` of
/// expiry (or already expired).
pub fn needs_renewal(cert_pem: &[u8], renew_before_days: u32) -> Result<bool, GatewayError> {
    let not_after = cert_not_after_secs(cert_pem)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let window = i64::from(renew_before_days) * 24 * 60 * 60;
    Ok(not_after - now <= window)
}

// ── Renewal task ────────────────────────────────────────────────────

/// Spawn a background loop that periodically re-checks the cached
/// ACME cert and re-acquires when it falls inside the renewal window.
/// Returns `None` for non-ACME modes (nothing to renew). The caller
/// is responsible for surfacing the join handle so a task panic or
/// terminal error doesn't get silently dropped.
pub fn spawn_renewal(
    state: TlsState,
    cfg: TlsConfig,
) -> Option<tokio::task::JoinHandle<Result<(), GatewayError>>> {
    let cache_dir = state.acme_cache_dir.clone()?;
    let interval = Duration::from_secs(cfg.acme_check_interval_secs.max(60));
    let renew_before = cfg.acme_renew_before_days;
    let domain = cfg.acme_domain.clone()?;

    let handle = tokio::spawn(async move {
        let (cert_file, _key_file) = cert_paths(&cache_dir, &domain);
        info!(
            domain = %domain,
            check_secs = cfg.acme_check_interval_secs,
            renew_before_days = renew_before,
            "ACME renewer started",
        );
        loop {
            tokio::time::sleep(interval).await;
            match try_renew(&state, &cfg, &cache_dir, &cert_file, renew_before).await {
                Ok(true) => info!("ACME: certificate renewed"),
                Ok(false) => {}
                Err(e) => warn!("ACME renewer tick failed: {e}"),
            }
        }
    });
    Some(handle)
}

async fn try_renew(
    state: &TlsState,
    cfg: &TlsConfig,
    cache_dir: &Path,
    cert_file: &Path,
    renew_before: u32,
) -> Result<bool, GatewayError> {
    let cert_pem = tokio::fs::read(cert_file)
        .await
        .map_err(|e| GatewayError::Config(format!("read {cert_file:?}: {e}")))?;
    if !needs_renewal(&cert_pem, renew_before)? {
        return Ok(false);
    }
    let (cert_pem, key_pem) = acquire_and_cache(cfg, cache_dir).await?;
    state.reload(&cert_pem, &key_pem).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cert_with_validity_days(days: i64) -> (String, String) {
        use rcgen::{CertificateParams, KeyPair};
        use time::OffsetDateTime;

        let mut params = CertificateParams::new(vec!["test.example".to_string()]).unwrap();
        params.not_before = OffsetDateTime::now_utc() - time::Duration::days(1);
        params.not_after = OffsetDateTime::now_utc() + time::Duration::days(days);
        let key = KeyPair::generate().unwrap();
        let cert = params.self_signed(&key).unwrap();
        (cert.pem(), key.serialize_pem())
    }

    #[test]
    fn cert_not_after_round_trips() {
        let (pem, _key) = make_cert_with_validity_days(60);
        let not_after = cert_not_after_secs(pem.as_bytes()).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let days_remaining = (not_after - now) / 86_400;
        assert!(
            (58..=60).contains(&days_remaining),
            "expected ~60 days, got {days_remaining}",
        );
    }

    #[test]
    fn needs_renewal_inside_window() {
        let (pem, _) = make_cert_with_validity_days(10);
        assert!(needs_renewal(pem.as_bytes(), 30).unwrap());
    }

    #[test]
    fn needs_renewal_outside_window() {
        let (pem, _) = make_cert_with_validity_days(60);
        assert!(!needs_renewal(pem.as_bytes(), 30).unwrap());
    }
}
