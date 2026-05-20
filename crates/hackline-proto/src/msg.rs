//! Message-plane wire types — what flows on
//! `hackline/<zid>/msg/event/...` and `hackline/<zid>/msg/log/...`.
//!
//! Per SCOPE.md §5.2 the envelope carries a sender-generated id, a
//! timestamp, a content type, a small headers map (trace ids, log
//! level), and an opaque payload. v0.1 encodes the whole envelope as
//! JSON over Zenoh (debuggability wins, payloads are small). The
//! `content_type` is reserved so bincode can swap in later without
//! re-versioning the keyexpr namespace.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CONTENT_TYPE_JSON: &str = "application/json";

/// Common envelope for events and logs. The `payload` is opaque to
/// the gateway — stored as a JSON value blob in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MsgEnvelope {
    pub id: Uuid,
    /// Unix milliseconds since epoch.
    pub ts: i64,
    #[serde(default = "default_content_type")]
    pub content_type: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    // `serde_json::Value` has no `Type` impl in our specta feature
    // set (we deliberately omit specta's `serde_json` feature
    // because its recursive `Vec<Value>` shape stack-overflows the
    // TS exporter). The override renders this field as TS
    // `unknown` — the wire is still arbitrary JSON, the contract
    // simply doesn't pretend to know its shape.
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Unknown))]
    pub payload: serde_json::Value,
}

fn default_content_type() -> String {
    CONTENT_TYPE_JSON.into()
}

/// Reserved header key carrying the log level on
/// `hackline/<zid>/msg/log/...` envelopes. Events do not set it.
pub const HEADER_LOG_LEVEL: &str = "level";

/// Five-level log severity. Lowercase string on the wire and in DB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "trace" => Some(Self::Trace),
            "debug" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "warn" => Some(Self::Warn),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

impl MsgEnvelope {
    /// Build a fresh event envelope. Callers set the payload; id, ts,
    /// content_type are filled in.
    pub fn new_event(payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            ts: now_ms(),
            content_type: CONTENT_TYPE_JSON.into(),
            headers: BTreeMap::new(),
            payload,
        }
    }

    /// Build a fresh log envelope. Level is stored in `headers.level`
    /// so the same envelope shape works for both planes.
    pub fn new_log(level: LogLevel, payload: serde_json::Value) -> Self {
        let mut e = Self::new_event(payload);
        e.headers
            .insert(HEADER_LOG_LEVEL.into(), level.as_str().into());
        e
    }

    /// Extract the log level from headers (info if missing or unknown).
    pub fn log_level(&self) -> LogLevel {
        self.headers
            .get(HEADER_LOG_LEVEL)
            .and_then(|s| LogLevel::parse(s))
            .unwrap_or_default()
    }
}

/// Durable command envelope flowing gateway→device on
/// `hackline/<zid>/msg/cmd/<topic>`. `cmd_id` is the idempotency key
/// on the device side (SCOPE.md §8.1); the device dedupes on it
/// across redeliveries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CmdEnvelope {
    pub cmd_id: Uuid,
    pub topic: String,
    pub enqueued_at: i64,
    pub expires_at: i64,
    pub envelope: MsgEnvelope,
}

/// Ack flowing device→gateway on
/// `hackline/<zid>/msg/cmd-ack/<cmd_id>` after the device's handler
/// finishes (or rejects) the command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CmdAck {
    pub cmd_id: Uuid,
    pub result: CmdResult,
    #[serde(default)]
    pub detail: Option<String>,
}

/// Outcome reported by the device-side cmd handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum CmdResult {
    Accepted,
    Rejected,
    Failed,
    Done,
}

impl CmdResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
            Self::Done => "done",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "accepted" => Some(Self::Accepted),
            "rejected" => Some(Self::Rejected),
            "failed" => Some(Self::Failed),
            "done" => Some(Self::Done),
            _ => None,
        }
    }
}

/// Request carried by a Zenoh `get` against
/// `hackline/<zid>/msg/api/<topic>`. Synchronous round-trip — the
/// gateway holds the HTTP connection open until the reply arrives
/// or the timeout fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ApiRequest {
    #[serde(default = "default_content_type")]
    pub content_type: String,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Unknown))]
    pub payload: serde_json::Value,
}

/// Reply published by the device-side `serve_api` handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ApiReply {
    #[serde(default = "default_content_type")]
    pub content_type: String,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Unknown))]
    pub payload: serde_json::Value,
}

impl ApiReply {
    pub fn json(payload: serde_json::Value) -> Self {
        Self {
            content_type: CONTENT_TYPE_JSON.into(),
            payload,
        }
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_round_trip() {
        let env = MsgEnvelope::new_event(serde_json::json!({"k": 1}));
        let c = CmdEnvelope {
            cmd_id: Uuid::nil(),
            topic: "block.install".into(),
            enqueued_at: 1,
            expires_at: 2,
            envelope: env,
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: CmdEnvelope = serde_json::from_str(&s).unwrap();
        assert_eq!(back.topic, "block.install");
    }

    #[test]
    fn cmd_result_serde() {
        let a = CmdAck {
            cmd_id: Uuid::nil(),
            result: CmdResult::Done,
            detail: None,
        };
        let s = serde_json::to_string(&a).unwrap();
        assert!(s.contains("\"done\""));
        let back: CmdAck = serde_json::from_str(&s).unwrap();
        assert_eq!(back.result, CmdResult::Done);
    }
}
