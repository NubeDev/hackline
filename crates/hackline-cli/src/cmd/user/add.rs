//! `hackline user add --name NAME --role ROLE`.

use crate::client::Client;
use crate::output;

pub async fn run(c: &Client, name: &str, role: &str, json: bool) -> anyhow::Result<()> {
    let body = serde_json::json!({ "name": name, "role": role });
    let resp = c.post("/v1/users", &body).await?;
    if json {
        output::print_json(&resp);
    } else {
        println!("User created: id={}", resp["user"]["id"]);
        println!(
            "Token (save this, shown once): {}",
            resp["token"].as_str().unwrap_or("")
        );
    }
    Ok(())
}
