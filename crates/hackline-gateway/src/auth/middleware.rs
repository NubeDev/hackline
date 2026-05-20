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
        let header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AuthError("missing Authorization header".into()))?;

        let raw_token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AuthError("expected Bearer token".into()))?;

        let hash = token::sha256_hex(raw_token);
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
