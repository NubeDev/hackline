//! `hackline org` — create / list / inspect tenant orgs.
//! SCOPE.md §13 Phase 4. Owner-only for create + list; every caller
//! can inspect their own org via `GET /v1/orgs/me`.

use crate::client::Client;
use crate::output;

pub async fn create(c: &Client, slug: &str, name: Option<&str>, json: bool) -> anyhow::Result<()> {
    let body = match name {
        Some(n) => serde_json::json!({ "slug": slug, "name": n }),
        None => serde_json::json!({ "slug": slug }),
    };
    let resp = c.post("/v1/orgs", &body).await?;
    if json {
        output::print_json(&resp);
        return Ok(());
    }
    println!(
        "Created org {} (id={}, name={})",
        resp["slug"].as_str().unwrap_or(""),
        resp["id"],
        resp["name"].as_str().unwrap_or(""),
    );
    Ok(())
}

pub async fn list(c: &Client, json: bool) -> anyhow::Result<()> {
    let resp = c.get("/v1/orgs").await?;
    if json {
        output::print_json(&resp);
        return Ok(());
    }
    let empty = vec![];
    let arr = resp.as_array().unwrap_or(&empty);
    let rows: Vec<Vec<String>> = arr
        .iter()
        .map(|o| {
            vec![
                o["id"].to_string(),
                o["slug"].as_str().unwrap_or("").to_string(),
                o["name"].as_str().unwrap_or("").to_string(),
            ]
        })
        .collect();
    output::print_table(&["ID", "SLUG", "NAME"], &rows);
    Ok(())
}

pub async fn inspect(c: &Client, json: bool) -> anyhow::Result<()> {
    let resp = c.get("/v1/orgs/me").await?;
    if json {
        output::print_json(&resp);
        return Ok(());
    }
    println!(
        "id={}\nslug={}\nname={}\ncreated_at={}",
        resp["id"],
        resp["slug"].as_str().unwrap_or(""),
        resp["name"].as_str().unwrap_or(""),
        resp["created_at"],
    );
    Ok(())
}
