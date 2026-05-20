//! `hackline device remove ID`.

use crate::client::Client;

pub async fn run(c: &Client, id: i64) -> anyhow::Result<()> {
    c.delete(&format!("/v1/devices/{id}")).await?;
    println!("Device {id} deleted.");
    Ok(())
}
