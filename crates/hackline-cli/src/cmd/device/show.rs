//! `hackline device show ID`.

use crate::client::Client;
use crate::output;

pub async fn run(c: &Client, id: i64, json: bool) -> anyhow::Result<()> {
    let resp = c.get(&format!("/v1/devices/{id}")).await?;
    if json {
        output::print_json(&resp);
    } else {
        println!("ID:    {}", resp["id"]);
        println!("ZID:   {}", resp["zid"].as_str().unwrap_or(""));
        println!("Label: {}", resp["label"].as_str().unwrap_or(""));
    }
    Ok(())
}
