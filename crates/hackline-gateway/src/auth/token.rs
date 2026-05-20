//! Token generation, SHA-256 hashing, constant-time compare via
//! `subtle`. Tokens are 32-byte URL-safe; only the hash hits disk.

use rand::RngCore;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// A raw token (shown to the user once) and its SHA-256 hex hash
/// (persisted in SQLite).
pub struct TokenPair {
    pub raw: String,
    pub hash: String,
}

/// Generate a cryptographically random 32-byte token, return both
/// the URL-safe base64 raw form and its SHA-256 hex hash.
pub fn generate() -> TokenPair {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let raw = base64_url_encode(&bytes);
    let hash = sha256_hex(&raw);
    TokenPair { raw, hash }
}

/// SHA-256 hex digest of a raw token string.
pub fn sha256_hex(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    hex::encode(digest)
}

/// Constant-time comparison of two hex-encoded hashes.
pub fn hashes_match(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

fn base64_url_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
