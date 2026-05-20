//! Zenoh device-id newtype.
//!
//! Canonical form: lowercase hex, no separators, length 2..=32.
//! Parsing and validation live here; no other crate is allowed to
//! roundtrip a raw `String` for a ZID.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::ProtoError;

/// A validated Zenoh device identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[cfg_attr(feature = "specta", specta(transparent))]
#[serde(try_from = "String", into = "String")]
pub struct Zid(String);

impl Zid {
    /// Parse and validate a raw string as a ZID.
    pub fn new(raw: &str) -> Result<Self, ProtoError> {
        let s = raw.to_ascii_lowercase();
        if s.len() < 2 || s.len() > 32 {
            return Err(ProtoError::InvalidZid(format!(
                "length {} outside 2..=32",
                s.len()
            )));
        }
        if !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ProtoError::InvalidZid("non-hex character".into()));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Zid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Zid {
    type Error = ProtoError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(&s)
    }
}

impl From<Zid> for String {
    fn from(z: Zid) -> String {
        z.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_zid() {
        assert!(Zid::new("ab").is_ok());
        assert!(Zid::new("0123456789abcdef").is_ok());
        assert!(Zid::new("AABB").unwrap().as_str() == "aabb");
    }

    #[test]
    fn rejects_bad_zid() {
        assert!(Zid::new("").is_err());
        assert!(Zid::new("a").is_err());
        assert!(Zid::new("zz").is_err());
        assert!(Zid::new(&"a".repeat(33)).is_err());
    }
}
