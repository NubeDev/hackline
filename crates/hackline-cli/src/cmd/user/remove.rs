//! `hackline user remove ID`.

use crate::client::Client;

pub async fn run(c: &Client, id: i64) -> anyhow::Result<()> {
    c.delete(&format!("/v1/users/{id}")).await?;
    println!("User {id} deleted.");
    Ok(())
}
