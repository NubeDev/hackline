//! `hackline tunnel list`.

use crate::client::Client;
use crate::output;

pub async fn run(c: &Client, json: bool) -> anyhow::Result<()> {
    let resp = c.get("/v1/tunnels").await?;
    if json {
        output::print_json(&resp);
        return Ok(());
    }
    let empty = vec![];
    let arr = resp.as_array().unwrap_or(&empty);
    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|t| {
            vec![
                t["id"].to_string(),
                t["device_id"].to_string(),
                t["kind"].as_str().unwrap_or("").to_string(),
                t["local_port"].to_string(),
                t["public_port"].to_string(),
            ]
        })
        .collect();
    output::print_table(&["ID", "DEVICE", "KIND", "LOCAL", "PUBLIC"], &rows);
    Ok(())
}
