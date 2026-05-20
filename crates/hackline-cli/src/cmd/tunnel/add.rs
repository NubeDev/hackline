//! `hackline tunnel add --device-id ID --kind tcp --local-port PORT [--public-port N]`.

use crate::client::Client;
use crate::output;

pub async fn run(
    c: &Client,
    device_id: i64,
    kind: &str,
    local_port: i64,
    public_port: Option<i64>,
    public_hostname: Option<&str>,
    json: bool,
) -> anyhow::Result<()> {
    let mut body = serde_json::json!({
        "device_id": device_id,
        "kind": kind,
        "local_port": local_port,
    });
    if let Some(pp) = public_port {
        body["public_port"] = serde_json::json!(pp);
    }
    if let Some(ph) = public_hostname {
        body["public_hostname"] = serde_json::json!(ph);
    }
    let resp = c.post("/v1/tunnels", &body).await?;
    if json {
        output::print_json(&resp);
    } else {
        println!(
            "Tunnel created: id={}, kind={}, local_port={}, public_port={}",
            resp["id"],
            resp["kind"].as_str().unwrap_or(""),
            resp["local_port"],
            resp["public_port"]
        );
    }
    Ok(())
}
