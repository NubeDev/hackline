//! Credentials cache + env-var overrides. Cache file lives at
//! `$XDG_CONFIG_HOME/hackline/credentials.json`, mode 0600.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Credentials {
    pub server: String,
    pub token: String,
    pub user_id: i64,
    pub name: String,
    /// Org slug captured at login time (SCOPE.md §13 Phase 4). Older
    /// caches without this field default to `default`; clients use
    /// it for telemetry display only — the server enforces real
    /// isolation off the bearer token.
    #[serde(default = "default_org_slug")]
    pub org: String,
}

fn default_org_slug() -> String {
    "default".into()
}

fn credentials_path() -> anyhow::Result<PathBuf> {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("hackline");
    Ok(dir.join("credentials.json"))
}

pub fn load_credentials() -> anyhow::Result<Option<Credentials>> {
    let path = credentials_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&path)?;
    let creds: Credentials = serde_json::from_str(&data)?;
    Ok(Some(creds))
}

pub fn save_credentials(creds: &Credentials) -> anyhow::Result<()> {
    let path = credentials_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(creds)?;
    fs::write(&path, data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
