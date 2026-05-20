//! Proto-level error type. Anything that can fail while parsing or
//! constructing a wire value lives here.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtoError {
    #[error("invalid ZID: {0}")]
    InvalidZid(String),

    #[error("invalid key expression: {0}")]
    InvalidKeyExpr(String),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}
