//! axum extractor that authenticates `Authorization: Bearer <token>`
//! against the `users` table and returns an `AuthedUser` to handlers.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::auth::token;
use crate::db::users::{self, User};
use crate::state::AppState;

/// Injected into handlers that require authentication.
#[derive(Debug, Clone)]
pub struct AuthedUser(pub User);

#[derive(Debug)]
pub struct AuthError(String);

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({ "error": self.0 })),
        )
            .into_response()
    }
}

impl FromRequestParts<AppState> for AuthedUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Bearer token in the Authorization header is the canonical
        // form. The `?token=` query fallback exists for
        // EventSource-based SSE clients (browsers can't attach
        // headers to an EventSource). Same secret, same trust level
        // — admin.js and the TS client both rely on this.
        let header_token = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .map(|s| s.to_owned());

        let raw_token = match header_token {
            Some(t) => t,
            None => parts
                .uri
                .query()
                .and_then(token_from_query)
                .ok_or_else(|| AuthError("missing Authorization header".into()))?,
        };

        let hash = token::sha256_hex(&raw_token);
        let db = state.db.clone();

        let user = tokio::task::spawn_blocking(move || {
            let conn = db.get().map_err(|e| AuthError(e.to_string()))?;
            let u = users::get_by_token_hash(&conn, &hash)
                .map_err(|e| AuthError(e.to_string()))?
                .ok_or_else(|| AuthError("invalid token".into()))?;
            let _ = users::touch(&conn, u.id);
            Ok::<_, AuthError>(u)
        })
        .await
        .unwrap()?;

        Ok(AuthedUser(user))
    }
}

/// Extract `token=<value>` from a raw query string, decoding the
/// minimal subset of percent-encoding likely to occur in token
/// payloads. Hackline tokens are base64url so `+`, `=`, and `/` are
/// the realistic cases; anything fancier we treat literally rather
/// than pulling in a full URL parser.
fn token_from_query(q: &str) -> Option<String> {
    for pair in q.split('&') {
        let (k, v) = pair.split_once('=')?;
        if k == "token" {
            return Some(percent_decode_minimal(v));
        }
    }
    None
}

fn percent_decode_minimal(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) =
                (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2]))
            {
                out.push((h << 4 | l) as char);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(' ');
        } else {
            out.push(bytes[i] as char);
        }
        i += 1;
    }
    out
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
