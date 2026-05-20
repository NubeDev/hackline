//! `hackline user list`.

use crate::client::Client;
use crate::output;

pub async fn run(c: &Client, json: bool) -> anyhow::Result<()> {
    let resp = c.get("/v1/users").await?;
    if json {
        output::print_json(&resp);
        return Ok(());
    }
    let empty = vec![];
    let arr = resp.as_array().unwrap_or(&empty);
    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|u| {
            vec![
                u["id"].to_string(),
                u["name"].as_str().unwrap_or("").to_string(),
                u["role"].as_str().unwrap_or("").to_string(),
            ]
        })
        .collect();
    output::print_table(&["ID", "NAME", "ROLE"], &rows);
    Ok(())
}
