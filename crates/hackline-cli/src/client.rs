//! Thin `reqwest` wrapper that injects `Authorization: Bearer …`
//! and decodes JSON into local `serde` value types.

use anyhow::{bail, Context};
use reqwest::StatusCode;

use crate::config;

pub struct Client {
    pub base_url: String,
    pub token: String,
    http: reqwest::Client,
}

impl Client {
    pub fn new(base_url: &str, token: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Build from explicit args (if provided) or fall back to cached credentials.
    pub fn from_args_or_cache(
        server: Option<String>,
        token: Option<String>,
    ) -> anyhow::Result<Self> {
        let creds = config::load_credentials()?;
        let base_url = server
            .or(creds.as_ref().map(|c| c.server.clone()))
            .context("no server URL — pass --server or run `hackline login` first")?;
        let tok = token
            .or(creds.map(|c| c.token))
            .context("no token — pass --token or run `hackline login` first")?;
        Ok(Self::new(&base_url, &tok))
    }

    pub async fn get(&self, path: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .http
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .send()
            .await?;
        check_status(resp).await
    }

    pub async fn post(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .http
            .post(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await?;
        check_status(resp).await
    }

    pub async fn post_no_auth(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .http
            .post(format!("{}{path}", self.base_url))
            .json(body)
            .send()
            .await?;
        check_status(resp).await
    }

    pub async fn delete(&self, path: &str) -> anyhow::Result<()> {
        let resp = self
            .http
            .delete(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .send()
            .await?;
        let status = resp.status();
        if status == StatusCode::NO_CONTENT {
            return Ok(());
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("HTTP {status}: {text}");
        }
        Ok(())
    }
}

async fn check_status(resp: reqwest::Response) -> anyhow::Result<serde_json::Value> {
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        bail!("HTTP {status}: {text}");
    }
    let val = resp.json().await?;
    Ok(val)
}
