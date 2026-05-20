//! `hackline device list`.

use crate::client::Client;
use crate::output;

pub async fn run(c: &Client, json: bool) -> anyhow::Result<()> {
    let resp = c.get("/v1/devices").await?;
    if json {
        output::print_json(&resp);
        return Ok(());
    }
    let empty = vec![];
    let arr = resp.as_array().unwrap_or(&empty);
    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|d| {
            vec![
                d["id"].to_string(),
                d["zid"].as_str().unwrap_or("").to_string(),
                d["label"].as_str().unwrap_or("").to_string(),
            ]
        })
        .collect();
    output::print_table(&["ID", "ZID", "LABEL"], &rows);
    Ok(())
}
