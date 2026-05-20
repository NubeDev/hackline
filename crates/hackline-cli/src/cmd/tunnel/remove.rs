//! `hackline tunnel remove ID`.

use crate::client::Client;

pub async fn run(c: &Client, id: i64) -> anyhow::Result<()> {
    c.delete(&format!("/v1/tunnels/{id}")).await?;
    println!("Tunnel {id} deleted.");
    Ok(())
}
