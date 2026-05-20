//! `POST /v1/tunnels` — opens a new public listener for `kind = tcp`
//! or registers a host route for `kind = http`.

use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use crate::auth::middleware::AuthedUser;
use crate::db::{audit, devices, tunnels};
use crate::error::GatewayError;
use crate::state::AppState;
use crate::tunnel::manager::TunnelEvent;

#[derive(Deserialize)]
pub struct CreateTunnel {
    pub device_id: i64,
    pub kind: String,
    pub local_port: i64,
    pub public_hostname: Option<String>,
    pub public_port: Option<i64>,
}

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
    Json(body): Json<CreateTunnel>,
) -> Result<(axum::http::StatusCode, Json<tunnels::Tunnel>), GatewayError> {
    let db = state.db.clone();
    let conn = db.get()?;
    let org_id = caller.org_id;
    let device_id = body.device_id;
    // Cross-org isolation: verify the device belongs to the caller's
    // org before opening a public listener for it. Returns 404 — see
    // db::devices::get_in_org for the leak-the-minimum design note.
    {
        let conn = db.get()?;
        tokio::task::spawn_blocking(move || devices::get_in_org(&conn, org_id, device_id))
            .await
            .unwrap()?;
    }
    let tunnel = tokio::task::spawn_blocking(move || {
        tunnels::insert(
            &conn,
            body.device_id,
            &body.kind,
            body.local_port,
            body.public_hostname.as_deref(),
            body.public_port,
        )
    })
    .await
    .unwrap()?;

    // Hot-start the TCP listener if applicable.
    if tunnel.kind == "tcp" {
        if let Some(public_port) = tunnel.public_port {
            let conn = db.get()?;
            let tid = tunnel.id;
            let did = tunnel.device_id;
            let lp = tunnel.local_port;
            if let Ok((device, org_slug)) = tokio::task::spawn_blocking(move || {
                let device = devices::get_in_org(&conn, org_id, did)?;
                let org = crate::db::orgs::get(&conn, device.org_id)?;
                Ok::<_, GatewayError>((device, org.slug))
            })
            .await
            .unwrap()
            {
                let twz = tunnels::TunnelWithZid {
                    id: tid,
                    device_id: did,
                    zid: device.zid,
                    org_slug,
                    kind: "tcp".into(),
                    local_port: lp as u16,
                    public_port: public_port as u16,
                    enabled: true,
                };
                let _ = state.tunnel_tx.send(TunnelEvent::Added(twz)).await;
            }
        }
    }

    // SCOPE.md §7.2: `tunnel.create` requires `kind`, `local_port`,
    // and the `public_*` fields the operator asked for. The new
    // tunnel + device ids go in `detail` rather than the FK columns
    // — audit's FKs to `tunnels(id)` and `devices(id)` are plain
    // (no `ON DELETE SET NULL`), so a row pointing at them would
    // block the operator from ever deleting the tunnel/device.
    {
        let db = state.db.clone();
        let user_id = caller.id;
        let detail = serde_json::json!({
            "tunnel_id": tunnel.id,
            "device_id": tunnel.device_id,
            "kind": tunnel.kind,
            "local_port": tunnel.local_port,
            "public_hostname": tunnel.public_hostname,
            "public_port": tunnel.public_port,
        })
        .to_string();
        let _ = tokio::task::spawn_blocking(move || -> Result<(), GatewayError> {
            let conn = db.get()?;
            audit::insert(
                &conn,
                Some(user_id),
                None,
                None,
                "tunnel.create",
                Some(&detail),
            )
        })
        .await;
    }

    Ok((axum::http::StatusCode::CREATED, Json(tunnel)))
}
