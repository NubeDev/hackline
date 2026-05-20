//! `hackline cmd send|list|cancel` and `hackline api call`.
//!
//! Thin wrappers over the REST surface — `POST /v1/devices/:id/cmd/:topic`,
//! `GET /v1/devices/:id/cmd`, `DELETE /v1/cmd/:cmd_id`, and
//! `POST /v1/devices/:id/api/:topic`.

use crate::client::Client;
use crate::output;

pub async fn send(
    c: &Client,
    device_id: i64,
    topic: &str,
    payload: serde_json::Value,
    expires_in_ms: Option<i64>,
    json: bool,
) -> anyhow::Result<()> {
    let mut body = serde_json::json!({ "payload": payload });
    if let Some(ms) = expires_in_ms {
        body["expires_in_ms"] = serde_json::json!(ms);
    }
    let resp = c
        .post(&format!("/v1/devices/{device_id}/cmd/{topic}"), &body)
        .await?;
    if json {
        output::print_json(&resp);
    } else {
        println!(
            "cmd queued: cmd_id={}",
            resp["cmd_id"].as_str().unwrap_or("")
        );
    }
    Ok(())
}

pub async fn list(
    c: &Client,
    device_id: i64,
    status: Option<&str>,
    json: bool,
) -> anyhow::Result<()> {
    let mut path = format!("/v1/devices/{device_id}/cmd?limit=100");
    if let Some(s) = status {
        path.push_str(&format!("&status={s}"));
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
                e["cmd_id"].as_str().unwrap_or("").to_string(),
                e["topic"].as_str().unwrap_or("").to_string(),
                e["status"].as_str().unwrap_or("").to_string(),
                e["attempts"].to_string(),
            ]
        })
        .collect();
    output::print_table(&["CMD_ID", "TOPIC", "STATUS", "ATTEMPTS"], &rows);
    Ok(())
}

pub async fn cancel(c: &Client, cmd_id: &str) -> anyhow::Result<()> {
    c.delete(&format!("/v1/cmd/{cmd_id}")).await?;
    println!("cmd {cmd_id} cancelled");
    Ok(())
}

pub async fn api_call(
    c: &Client,
    device_id: i64,
    topic: &str,
    payload: serde_json::Value,
    timeout_ms: u64,
    json: bool,
) -> anyhow::Result<()> {
    let body = serde_json::json!({
        "payload": payload,
        "timeout_ms": timeout_ms,
    });
    let resp = c
        .post(&format!("/v1/devices/{device_id}/api/{topic}"), &body)
        .await?;
    if json {
        output::print_json(&resp);
    } else {
        println!("{}", serde_json::to_string_pretty(&resp["reply"])?);
    }
    Ok(())
}
