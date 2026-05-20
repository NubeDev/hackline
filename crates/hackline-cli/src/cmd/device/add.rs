//! `hackline device add --zid ZID --label TEXT`.

use crate::client::Client;
use crate::output;

pub async fn run(c: &Client, zid: &str, label: &str, json: bool) -> anyhow::Result<()> {
    let body = serde_json::json!({ "zid": zid, "label": label });
    let resp = c.post("/v1/devices", &body).await?;
    if json {
        output::print_json(&resp);
    } else {
        println!(
            "Device created: id={}, zid={}, label={}",
            resp["id"],
            resp["zid"].as_str().unwrap_or(""),
            resp["label"].as_str().unwrap_or("")
        );
    }
    Ok(())
}
