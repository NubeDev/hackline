//! Gateway error type. Maps cleanly onto HTTP status codes via the
//! `IntoResponse` impl so handlers can `?` freely.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("config: {0}")]
    Config(String),

    #[error("bridge: {0}")]
    Bridge(#[from] hackline_core::error::BridgeError),

    #[error("zenoh: {0}")]
    Zenoh(#[from] zenoh::Error),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("proto: {0}")]
    Proto(#[from] hackline_proto::error::ProtoError),

    #[error("db: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("db pool: {0}")]
    Pool(#[from] r2d2::Error),

    #[error("not found")]
    NotFound,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            Self::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            Self::Unauthorized(_) => (StatusCode::UNAUTHORIZED, self.to_string()),
            Self::Config(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into()),
        };
        (status, axum::Json(serde_json::json!({ "error": msg }))).into_response()
    }
}
