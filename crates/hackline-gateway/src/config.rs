//! TOML config loader. Schema documented in `DOCS/CONFIG.md`.
//! Unknown keys are an error so a typo doesn't silently disable
//! something.

use std::path::Path;

use serde::Deserialize;

use crate::error::GatewayError;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayConfig {
    #[serde(default)]
    pub listen: Option<String>,
    /// Optional public HTTP host-routing listener (SCOPE.md §13
    /// Phase 2). When set, the gateway accepts HTTP requests on this
    /// address and proxies them through the matching `http` tunnel
    /// based on the `Host:` header.
    #[serde(default)]
    pub http_listen: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    pub zenoh: ZenohConfig,
    #[serde(default)]
    pub tunnels: Vec<TunnelEntry>,
    #[serde(default)]
    pub log: LogConfig,
    /// Optional TLS block (SCOPE.md §13 Phase 5). When present the
    /// gateway terminates TLS itself — no Caddy required. Requires
    /// the `tls` crate feature.
    #[serde(default)]
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZenohConfig {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub listen: Vec<String>,
    #[serde(default)]
    pub connect: Vec<String>,
    #[serde(default)]
    pub tls: Option<ZenohTlsConfig>,
}

/// Optional TLS block for Zenoh transport. When present, Zenoh
/// endpoints should use the `tls/` scheme instead of `tcp/`.
/// In router mode, set `server_certificate` + `server_private_key`
/// + `client_auth`. In client mode, set `client_certificate` +
///   `client_private_key`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZenohTlsConfig {
    /// CA certificate used to verify the peer's certificate.
    pub root_ca_certificate: String,
    /// Server certificate (router/peer mode).
    #[serde(default)]
    pub server_certificate: Option<String>,
    /// Server private key (router/peer mode).
    #[serde(default)]
    pub server_private_key: Option<String>,
    /// Client certificate (client mode).
    #[serde(default)]
    pub client_certificate: Option<String>,
    /// Client private key (client mode).
    #[serde(default)]
    pub client_private_key: Option<String>,
    /// Require connecting peers to present a valid client cert.
    #[serde(default)]
    pub client_auth: bool,
    /// Skip hostname/domain verification on the peer's cert.
    /// Needed for peer-to-peer TLS on LAN where peers have no domain.
    #[serde(default = "default_verify_name")]
    pub verify_name_on_connect: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TunnelEntry {
    pub zid: String,
    pub device_port: u16,
    pub listen_port: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
        }
    }
}

/// TLS termination config. Three mutually exclusive modes:
///
/// 1. **ACME**: set `acme_domain` + `acme_email`. The gateway runs an
///    HTTP-01 challenge responder on port 80 and obtains certs from
///    Let's Encrypt (or the staging endpoint if `acme_staging = true`).
///
/// 2. **Manual certs**: set `cert_path` + `key_path`. The gateway loads
///    PEM files from disk.
///
/// 3. **Self-signed**: set `self_signed = true`. The gateway generates
///    an ephemeral cert on startup (dev/testing only).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsConfig {
    /// ACME domain (e.g. "cloud.example.com"). Triggers ACME mode.
    #[serde(default)]
    pub acme_domain: Option<String>,
    /// ACME contact email (required with acme_domain).
    #[serde(default)]
    pub acme_email: Option<String>,
    /// Use Let's Encrypt staging endpoint (rate-limit safe).
    #[serde(default)]
    pub acme_staging: bool,
    /// Directory to cache ACME account + certs. Defaults to
    /// `$STATE_DIR/acme/`.
    #[serde(default)]
    pub acme_cache_dir: Option<String>,
    /// Renewal threshold: re-acquire when the cached cert is within
    /// this many days of expiry. Default 30, matching the Let's
    /// Encrypt baseline (90-day cert, renew at ≅30 days remaining).
    #[serde(default = "default_renew_before_days")]
    pub acme_renew_before_days: u32,
    /// How often the renewer wakes to re-check expiry. Default 12 h.
    /// The renewer is cheap when nothing needs doing (one PEM parse),
    /// so the interval can stay generous.
    #[serde(default = "default_renew_check_secs")]
    pub acme_check_interval_secs: u64,

    /// Path to PEM-encoded certificate chain. Triggers manual mode.
    #[serde(default)]
    pub cert_path: Option<String>,
    /// Path to PEM-encoded private key (must match cert_path).
    #[serde(default)]
    pub key_path: Option<String>,

    /// Generate a throwaway self-signed cert on startup.
    #[serde(default)]
    pub self_signed: bool,
}

impl TlsConfig {
    /// Which TLS mode the user configured. Returns an error if the
    /// combination is ambiguous or incomplete.
    pub fn mode(&self) -> Result<TlsMode, GatewayError> {
        let has_acme = self.acme_domain.is_some();
        let has_manual = self.cert_path.is_some() || self.key_path.is_some();
        let has_self = self.self_signed;

        match (has_acme, has_manual, has_self) {
            (true, false, false) => {
                if self.acme_email.is_none() {
                    return Err(GatewayError::Config(
                        "[tls] acme_domain requires acme_email".into(),
                    ));
                }
                Ok(TlsMode::Acme)
            }
            (false, true, false) => {
                if self.cert_path.is_none() || self.key_path.is_none() {
                    return Err(GatewayError::Config(
                        "[tls] manual mode requires both cert_path and key_path".into(),
                    ));
                }
                Ok(TlsMode::Manual)
            }
            (false, false, true) => Ok(TlsMode::SelfSigned),
            (false, false, false) => Err(GatewayError::Config(
                "[tls] block present but no mode configured".into(),
            )),
            _ => Err(GatewayError::Config(
                "[tls] ambiguous: set exactly one of acme_domain, cert_path/key_path, or self_signed".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsMode {
    Acme,
    Manual,
    SelfSigned,
}

fn default_mode() -> String {
    "client".into()
}
fn default_log_level() -> String {
    "info".into()
}
fn default_log_format() -> String {
    "pretty".into()
}
fn default_verify_name() -> bool {
    true
}
fn default_renew_before_days() -> u32 {
    30
}
fn default_renew_check_secs() -> u64 {
    12 * 60 * 60
}

impl GatewayConfig {
    pub fn load(path: &Path) -> Result<Self, GatewayError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| GatewayError::Config(format!("{path:?}: {e}")))?;
        let cfg: Self =
            toml::from_str(&text).map_err(|e| GatewayError::Config(format!("{path:?}: {e}")))?;
        Ok(cfg)
    }

    pub fn to_zenoh_config(&self) -> Result<zenoh::Config, GatewayError> {
        let mut config = zenoh::Config::default();
        config
            .insert_json5("mode", &format!(r#""{}""#, self.zenoh.mode))
            .map_err(|e| GatewayError::Config(format!("zenoh mode: {e}")))?;
        if !self.zenoh.listen.is_empty() {
            let json = serde_json::to_string(&self.zenoh.listen)
                .map_err(|e| GatewayError::Config(format!("zenoh listen: {e}")))?;
            config
                .insert_json5("listen/endpoints", &json)
                .map_err(|e| GatewayError::Config(format!("zenoh listen: {e}")))?;
        }
        if !self.zenoh.connect.is_empty() {
            let json = serde_json::to_string(&self.zenoh.connect)
                .map_err(|e| GatewayError::Config(format!("zenoh connect: {e}")))?;
            config
                .insert_json5("connect/endpoints", &json)
                .map_err(|e| GatewayError::Config(format!("zenoh connect: {e}")))?;
        }
        config
            .insert_json5("scouting/multicast/enabled", "false")
            .map_err(|e| GatewayError::Config(format!("zenoh scouting: {e}")))?;
        if let Some(tls) = &self.zenoh.tls {
            config
                .insert_json5(
                    "transport/link/tls/root_ca_certificate",
                    &format!(r#""{}""#, tls.root_ca_certificate),
                )
                .map_err(|e| GatewayError::Config(format!("zenoh tls root_ca: {e}")))?;
            if let Some(cert) = &tls.server_certificate {
                config
                    .insert_json5(
                        "transport/link/tls/listen_certificate",
                        &format!(r#""{}""#, cert),
                    )
                    .map_err(|e| GatewayError::Config(format!("zenoh tls server_cert: {e}")))?;
            }
            if let Some(key) = &tls.server_private_key {
                config
                    .insert_json5(
                        "transport/link/tls/listen_private_key",
                        &format!(r#""{}""#, key),
                    )
                    .map_err(|e| GatewayError::Config(format!("zenoh tls server_key: {e}")))?;
            }
            if let Some(cert) = &tls.client_certificate {
                config
                    .insert_json5(
                        "transport/link/tls/connect_certificate",
                        &format!(r#""{}""#, cert),
                    )
                    .map_err(|e| GatewayError::Config(format!("zenoh tls client_cert: {e}")))?;
            }
            if let Some(key) = &tls.client_private_key {
                config
                    .insert_json5(
                        "transport/link/tls/connect_private_key",
                        &format!(r#""{}""#, key),
                    )
                    .map_err(|e| GatewayError::Config(format!("zenoh tls client_key: {e}")))?;
            }
            if tls.client_auth {
                config
                    .insert_json5("transport/link/tls/enable_mtls", "true")
                    .map_err(|e| GatewayError::Config(format!("zenoh tls client_auth: {e}")))?;
            }
            if !tls.verify_name_on_connect {
                config
                    .insert_json5("transport/link/tls/verify_name_on_connect", "false")
                    .map_err(|e| GatewayError::Config(format!("zenoh tls verify_name: {e}")))?;
            }
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tls(f: impl FnOnce(&mut TlsConfig)) -> TlsConfig {
        let mut cfg = TlsConfig {
            acme_domain: None,
            acme_email: None,
            acme_staging: false,
            acme_cache_dir: None,
            acme_renew_before_days: default_renew_before_days(),
            acme_check_interval_secs: default_renew_check_secs(),
            cert_path: None,
            key_path: None,
            self_signed: false,
        };
        f(&mut cfg);
        cfg
    }

    #[test]
    fn tls_mode_self_signed() {
        let cfg = tls(|c| c.self_signed = true);
        assert_eq!(cfg.mode().unwrap(), TlsMode::SelfSigned);
    }

    #[test]
    fn tls_mode_manual() {
        let cfg = tls(|c| {
            c.cert_path = Some("/tmp/cert.pem".into());
            c.key_path = Some("/tmp/key.pem".into());
        });
        assert_eq!(cfg.mode().unwrap(), TlsMode::Manual);
    }

    #[test]
    fn tls_mode_manual_needs_both() {
        let cfg = tls(|c| c.cert_path = Some("/tmp/cert.pem".into()));
        assert!(cfg.mode().is_err());
    }

    #[test]
    fn tls_mode_acme() {
        let cfg = tls(|c| {
            c.acme_domain = Some("example.com".into());
            c.acme_email = Some("admin@example.com".into());
        });
        assert_eq!(cfg.mode().unwrap(), TlsMode::Acme);
    }

    #[test]
    fn tls_mode_acme_needs_email() {
        let cfg = tls(|c| c.acme_domain = Some("example.com".into()));
        assert!(cfg.mode().is_err());
    }

    #[test]
    fn tls_mode_empty_errors() {
        let cfg = tls(|_| {});
        assert!(cfg.mode().is_err());
    }

    #[test]
    fn tls_mode_ambiguous_errors() {
        let cfg = tls(|c| {
            c.self_signed = true;
            c.acme_domain = Some("example.com".into());
            c.acme_email = Some("admin@example.com".into());
        });
        assert!(cfg.mode().is_err());
    }

    #[test]
    fn tls_block_parses_from_toml() {
        let toml_str = r#"
[zenoh]
mode = "client"

[tls]
self_signed = true
"#;
        let cfg: GatewayConfig = toml::from_str(toml_str).unwrap();
        assert!(cfg.tls.is_some());
        assert_eq!(cfg.tls.unwrap().mode().unwrap(), TlsMode::SelfSigned);
    }

    #[test]
    fn no_tls_block_parses() {
        let toml_str = r#"
[zenoh]
mode = "client"
"#;
        let cfg: GatewayConfig = toml::from_str(toml_str).unwrap();
        assert!(cfg.tls.is_none());
    }
}
