//! `hackline events tail` and `hackline events history`.
//!
//! `tail` opens the SSE stream at `/v1/events/stream` and prints one
//! JSON line per delivery — `--device` / `--topic` narrow the live
//! filter server-side. `history` paginates `/v1/events` through the
//! `next_cursor` field.
//!
//! Matches the SCOPE.md §13 Phase 1.5 demo: a publisher writes,
//! `hackline events tail --device ID` shows the row live; the same
//! row is queryable through `events history`.

use anyhow::{bail, Context};
use futures::StreamExt;

use crate::client::Client;
use crate::output;

pub async fn tail(
    c: &Client,
    device: Option<i64>,
    topic: Option<&str>,
    kind: StreamKind,
) -> anyhow::Result<()> {
    let mut url = format!("{}{}", c.base_url, kind.stream_path());
    let mut sep = '?';
    if let Some(d) = device {
        url.push_str(&format!("{sep}device={d}"));
        sep = '&';
    }
    if let Some(t) = topic {
        url.push_str(&format!("{sep}topic={t}"));
    }

    let resp = reqwest::Client::new()
        .get(&url)
        .bearer_auth(&c.token)
        .header("accept", "text/event-stream")
        .send()
        .await?;
    if !resp.status().is_success() {
        bail!(
            "HTTP {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("read SSE chunk")?;
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(end) = buf.find("\n\n") {
            let event: String = buf.drain(..end + 2).collect();
            if let Some(data) = parse_sse_data(&event) {
                println!("{data}");
            }
        }
    }
    Ok(())
}

pub async fn history(
    c: &Client,
    device: Option<i64>,
    topic: Option<&str>,
    limit: i64,
    json: bool,
    kind: StreamKind,
) -> anyhow::Result<()> {
    let mut path = format!("{}?limit={limit}", kind.list_path());
    if let Some(d) = device {
        path.push_str(&format!("&device={d}"));
    }
    if let Some(t) = topic {
        path.push_str(&format!("&topic={t}"));
    }
    let resp = c.get(&path).await?;
    if json {
        output::print_json(&resp);
        return Ok(());
    }
    let empty = vec![];
    let arr = resp["entries"].as_array().unwrap_or(&empty);
    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|e| {
            vec![
                e["id"].to_string(),
                e["device_id"].to_string(),
                e["ts"].to_string(),
                e["topic"].as_str().unwrap_or("").to_string(),
                e["payload"].to_string(),
            ]
        })
        .collect();
    output::print_table(&["ID", "DEVICE", "TS", "TOPIC", "PAYLOAD"], &rows);
    Ok(())
}

/// Which stream family to follow. The two endpoints share the same
/// SSE framing and row shape, so the only thing that varies is the
/// path.
#[derive(Debug, Clone, Copy)]
pub enum StreamKind {
    Events,
    Logs,
}

impl StreamKind {
    fn stream_path(self) -> &'static str {
        match self {
            Self::Events => "/v1/events/stream",
            Self::Logs => "/v1/log/stream",
        }
    }
    fn list_path(self) -> &'static str {
        match self {
            Self::Events => "/v1/events",
            Self::Logs => "/v1/log",
        }
    }
}

/// Pull the `data:` line out of a single SSE event block (terminated
/// by a blank line). Ignores `event:` / `id:` / comments — the row
/// payload itself already carries everything the caller needs.
fn parse_sse_data(event: &str) -> Option<String> {
    let mut out = String::new();
    for line in event.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(rest.trim_start());
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}
