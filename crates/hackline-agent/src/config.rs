//! TOML config loader for the agent. Schema is documented in
//! `DOCS/CONFIG.md`. Validation rejects unknown keys so a typo in
//! `allowed_ports` doesn't silently expose nothing.

use std::path::Path;

use serde::Deserialize;

use crate::error::AgentError;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
    pub zid: String,
    /// Tenant org slug (SCOPE.md §13 Phase 4). Determines the
    /// `hackline/<org>/<zid>/...` Zenoh namespace this agent
    /// declares its queryables under. Single-tenant deployments
    /// leave it at the default; multi-tenant deployments pin each
    /// device to its owner org's slug.
    #[serde(default = "default_org")]
    pub org: String,
    pub allowed_ports: Vec<u16>,
    #[serde(default)]
    pub label: Option<String>,
    pub zenoh: ZenohConfig,
    #[serde(default)]
    pub log: LogConfig,
    /// Loopback-only diagnostic UI. Disabled by default — operators
    /// opt in by setting `[diag] enabled = true`. Bind defaults to
    /// `127.0.0.1:9999`; non-loopback addresses are rejected at
    /// startup so the diag surface never reaches the network.
    #[serde(default)]
    pub diag: DiagConfig,
}

fn default_org() -> String {
    "default".into()
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

fn default_mode() -> String {
    "peer".into()
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_diag_bind")]
    pub bind: String,
}

impl Default for DiagConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: default_diag_bind(),
        }
    }
}

fn default_diag_bind() -> String {
    "127.0.0.1:9999".into()
}

impl AgentConfig {
    pub fn load(path: &Path) -> Result<Self, AgentError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| AgentError::Config(format!("{path:?}: {e}")))?;
        let cfg: Self =
            toml::from_str(&text).map_err(|e| AgentError::Config(format!("{path:?}: {e}")))?;
        if cfg.allowed_ports.is_empty() {
            return Err(AgentError::Config("allowed_ports must not be empty".into()));
        }
        Ok(cfg)
    }

    pub fn to_zenoh_config(&self) -> Result<zenoh::Config, AgentError> {
        let mut config = zenoh::Config::default();
        config
            .insert_json5("mode", &format!(r#""{}""#, self.zenoh.mode))
            .map_err(|e| AgentError::Config(format!("zenoh mode: {e}")))?;
        if !self.zenoh.listen.is_empty() {
            let json = serde_json::to_string(&self.zenoh.listen)
                .map_err(|e| AgentError::Config(format!("zenoh listen: {e}")))?;
            config
                .insert_json5("listen/endpoints", &json)
                .map_err(|e| AgentError::Config(format!("zenoh listen: {e}")))?;
        }
        if !self.zenoh.connect.is_empty() {
            let json = serde_json::to_string(&self.zenoh.connect)
                .map_err(|e| AgentError::Config(format!("zenoh connect: {e}")))?;
            config
                .insert_json5("connect/endpoints", &json)
                .map_err(|e| AgentError::Config(format!("zenoh connect: {e}")))?;
        }
        config
            .insert_json5("scouting/multicast/enabled", "false")
            .map_err(|e| AgentError::Config(format!("zenoh scouting: {e}")))?;
        if let Some(tls) = &self.zenoh.tls {
            config
                .insert_json5(
                    "transport/link/tls/root_ca_certificate",
                    &format!(r#""{}""#, tls.root_ca_certificate),
                )
                .map_err(|e| AgentError::Config(format!("zenoh tls root_ca: {e}")))?;
            if let Some(cert) = &tls.server_certificate {
                config
                    .insert_json5(
                        "transport/link/tls/listen_certificate",
                        &format!(r#""{}""#, cert),
                    )
                    .map_err(|e| AgentError::Config(format!("zenoh tls server_cert: {e}")))?;
            }
            if let Some(key) = &tls.server_private_key {
                config
                    .insert_json5(
                        "transport/link/tls/listen_private_key",
                        &format!(r#""{}""#, key),
                    )
                    .map_err(|e| AgentError::Config(format!("zenoh tls server_key: {e}")))?;
            }
            if let Some(cert) = &tls.client_certificate {
                config
                    .insert_json5(
                        "transport/link/tls/connect_certificate",
                        &format!(r#""{}""#, cert),
                    )
                    .map_err(|e| AgentError::Config(format!("zenoh tls client_cert: {e}")))?;
            }
            if let Some(key) = &tls.client_private_key {
                config
                    .insert_json5(
                        "transport/link/tls/connect_private_key",
                        &format!(r#""{}""#, key),
                    )
                    .map_err(|e| AgentError::Config(format!("zenoh tls client_key: {e}")))?;
            }
            if tls.client_auth {
                config
                    .insert_json5("transport/link/tls/enable_mtls", "true")
                    .map_err(|e| AgentError::Config(format!("zenoh tls client_auth: {e}")))?;
            }
            if !tls.verify_name_on_connect {
                config
                    .insert_json5("transport/link/tls/verify_name_on_connect", "false")
                    .map_err(|e| AgentError::Config(format!("zenoh tls verify_name: {e}")))?;
            }
        }
        Ok(config)
    }
}
