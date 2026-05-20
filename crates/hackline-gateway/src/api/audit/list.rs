//! `GET /v1/audit?limit=N` — recent audit entries.

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::auth::middleware::AuthedUser;
use crate::db::audit;
use crate::error::GatewayError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct AuditQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

/// Public projection of `db::audit::AuditEntry` matching
/// `DOCS/openapi.yaml` §AuditEntry. The DB row carries
/// `tunnel.session`-shaped extras (`ts_close`, `bytes_up`,
/// `bytes_down`, `peer`, `request_id`) that are an internal
/// implementation detail; the wire only documents the
/// point-in-time fields plus a `subject` derived from whichever FK
/// the row references.
#[derive(Serialize)]
pub struct AuditEntryView {
    pub id: i64,
    pub at: i64,
    pub actor_user_id: Option<i64>,
    pub action: String,
    pub subject: String,
    pub detail: serde_json::Value,
}

/// Page envelope per `DOCS/openapi.yaml` §AuditPage:
/// `{ items, next_cursor }`. Audit list is not yet cursor-paginated
/// so `next_cursor` is always `None`; the wrapper exists so a future
/// pagination addition is non-breaking. Other paginated endpoints
/// (events, cmd outbox, logs) share the same envelope shape.
#[derive(Serialize)]
pub struct AuditPage {
    pub items: Vec<AuditEntryView>,
    pub next_cursor: Option<i64>,
}

pub async fn handler(
    State(state): State<AppState>,
    AuthedUser(caller): AuthedUser,
    Query(q): Query<AuditQuery>,
) -> Result<Json<AuditPage>, GatewayError> {
    let conn = state.db.get()?;
    let org_id = caller.org_id;
    let rows = tokio::task::spawn_blocking(move || audit::list_recent(&conn, org_id, q.limit))
        .await
        .unwrap()?;
    let items = rows.into_iter().map(project).collect();
    Ok(Json(AuditPage {
        items,
        next_cursor: None,
    }))
}

/// `subject = "<resource>:<id>"` per openapi. Tunnel beats device
/// beats user — the most specific FK wins so an audit reader can
/// jump straight to the entity that caused the row.
///
/// Falls back to `detail.tunnel_id` / `detail.device_id` when the
/// FK column is NULL. The delete handlers in
/// `api/{tunnels,devices}/delete.rs` deliberately insert the
/// post-delete audit row with FK = NULL and the orphaned id
/// stuffed into `detail`, because the FK would dangle (V007 made
/// the column `ON DELETE SET NULL`, so an FK-bearing insert
/// would be silently nulled the next time the parent went
/// anyway). Without this fallback those `tunnel.delete` /
/// `device.delete` rows render with an empty subject in the UI
/// even though the originating entity is right there in
/// `detail`.
///
/// Rows with neither an FK nor a recoverable id (e.g.
/// `auth.login` before user resolution) emit an empty subject;
/// openapi requires the field to be present, not non-empty.
fn project(row: audit::AuditEntry) -> AuditEntryView {
    let detail = match row.detail.as_deref() {
        Some(s) => serde_json::from_str::<serde_json::Value>(s).unwrap_or_else(|_| {
            // The DB column has historically held free-form text as
            // well as JSON. Wrap non-JSON in a documented envelope so
            // the openapi `type: object` invariant holds.
            serde_json::json!({ "raw": s })
        }),
        None => serde_json::Value::Null,
    };
    let subject = if let Some(t) = row.tunnel_id {
        format!("tunnel:{t}")
    } else if let Some(d) = row.device_id {
        format!("device:{d}")
    } else if let Some(t) = detail_id(&detail, "tunnel_id") {
        format!("tunnel:{t}")
    } else if let Some(d) = detail_id(&detail, "device_id") {
        format!("device:{d}")
    } else if let Some(u) = row.user_id {
        format!("user:{u}")
    } else {
        String::new()
    };
    AuditEntryView {
        id: row.id,
        at: row.ts,
        actor_user_id: row.user_id,
        action: row.action,
        subject,
        detail,
    }
}

/// Read a numeric id from `detail.<key>`. Accepts both i64 and
/// the lossless u64 encoding `serde_json` emits for very large
/// values; row ids fit in i64 in practice but the conversion is
/// explicit so the projection cannot lose precision silently.
fn detail_id(detail: &serde_json::Value, key: &str) -> Option<i64> {
    detail.get(key).and_then(|v| v.as_i64())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::audit::AuditEntry;

    fn row(id: i64, action: &str) -> AuditEntry {
        AuditEntry {
            id,
            ts: 0,
            user_id: None,
            device_id: None,
            tunnel_id: None,
            action: action.to_string(),
            detail: None,
            ts_close: None,
            request_id: None,
            peer: None,
            bytes_up: None,
            bytes_down: None,
        }
    }

    #[test]
    fn subject_prefers_tunnel_fk_over_device_fk() {
        let mut r = row(1, "tunnel.session");
        r.tunnel_id = Some(7);
        r.device_id = Some(3);
        assert_eq!(project(r).subject, "tunnel:7");
    }

    #[test]
    fn subject_falls_back_to_device_fk_when_tunnel_null() {
        let mut r = row(1, "cmd.send");
        r.device_id = Some(3);
        assert_eq!(project(r).subject, "device:3");
    }

    #[test]
    fn subject_recovered_from_detail_tunnel_id_after_delete() {
        let mut r = row(1, "tunnel.delete");
        r.user_id = Some(2);
        r.detail = Some(r#"{"tunnel_id": 42}"#.to_string());
        let v = project(r);
        assert_eq!(v.subject, "tunnel:42");
        assert_eq!(v.actor_user_id, Some(2));
    }

    #[test]
    fn subject_recovered_from_detail_device_id_after_delete() {
        let mut r = row(1, "device.delete");
        r.user_id = Some(2);
        r.detail = Some(r#"{"device_id": 99}"#.to_string());
        assert_eq!(project(r).subject, "device:99");
    }

    #[test]
    fn subject_user_only_when_no_other_signal() {
        let mut r = row(1, "auth.login");
        r.user_id = Some(5);
        assert_eq!(project(r).subject, "user:5");
    }

    #[test]
    fn subject_empty_when_no_fks_and_no_recoverable_detail() {
        let r = row(1, "auth.login");
        assert_eq!(project(r).subject, "");
    }

    /// Free-form text in `detail` predates the JSON convention.
    /// The projection must still produce a valid object so the
    /// openapi `detail: object` invariant holds, and the subject
    /// fallback must not panic when there is no `tunnel_id` /
    /// `device_id` key to read.
    #[test]
    fn non_json_detail_is_wrapped_and_subject_falls_through() {
        let mut r = row(1, "auth.login");
        r.user_id = Some(5);
        r.detail = Some("legacy free text".to_string());
        let v = project(r);
        assert_eq!(v.subject, "user:5");
        assert_eq!(v.detail, serde_json::json!({ "raw": "legacy free text" }));
    }
}
