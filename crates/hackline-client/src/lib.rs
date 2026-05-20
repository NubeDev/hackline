//! Device-side SDK. A thin wrapper around `zenoh::Session` that
//! enforces hackline's message-plane conventions: validated topics,
//! `MsgEnvelope` JSON over Zenoh, keyexprs scoped to the session's
//! own ZID. Phase 1.5 ships `publish_event` and `publish_log`;
//! `serve_api` / `subscribe_cmd` land in Phase 2.
//!
//! The SDK never opens a second auth layer — auth is Zenoh ACL on
//! the session itself (SCOPE.md §3.5 / §8.2).

use std::sync::Arc;

use hackline_proto::keyexpr;
use hackline_proto::msg::{
    ApiReply, ApiRequest, CmdAck, CmdEnvelope, CmdResult, LogLevel, MsgEnvelope,
};
use hackline_proto::zid::Zid;
use std::future::Future;
use thiserror::Error;
use zenoh::bytes::ZBytes;
use zenoh::Session;

/// SDK error type. Surfaces config validation, Zenoh transport
/// failures, and JSON serialisation problems without forcing callers
/// to depend on Zenoh's own error type.
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("zenoh: {0}")]
    Zenoh(#[from] zenoh::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid zid: {0}")]
    Zid(String),
    #[error("invalid topic: {0}")]
    Topic(String),
}

/// SDK session. Holds a `zenoh::Session` and the device's `Zid` so
/// every published keyexpr is scoped to the session's own namespace
/// (SCOPE.md §3.5 trust model — publishing under another zid is an
/// ACL violation and would be rejected by the router anyway).
#[derive(Clone)]
pub struct ClientSession {
    inner: Arc<Session>,
    /// Tenant org slug (SCOPE.md §13 Phase 4). Determines the
    /// `hackline/<org>/...` namespace prefix that every published
    /// keyexpr lives under. The SDK rejects calls that would publish
    /// outside this prefix; the Zenoh ACL is the same line of
    /// defence at the router edge.
    org: String,
    zid: Zid,
}

impl ClientSession {
    /// Wrap an already-open `zenoh::Session`. v0.1 leaves session
    /// opening to the host application (rubix-agent, tests) because
    /// the right config layer differs per consumer; the SDK adds the
    /// hackline-specific message-plane conventions on top.
    pub fn from_session(session: Arc<Session>, org: impl Into<String>, zid: Zid) -> Self {
        Self {
            inner: session,
            org: org.into(),
            zid,
        }
    }

    /// Convenience constructor that derives the device `Zid` from the
    /// session's own Zenoh ZID. Fails if the Zenoh ZID isn't a valid
    /// `hackline_proto::Zid` (length 2..=32, lowercase hex).
    pub fn from_session_auto(
        session: Arc<Session>,
        org: impl Into<String>,
    ) -> Result<Self, ClientError> {
        let raw = session.zid().to_string();
        let zid = Zid::new(&raw).map_err(|e| ClientError::Zid(e.to_string()))?;
        Ok(Self::from_session(session, org, zid))
    }

    pub fn zid(&self) -> &Zid {
        &self.zid
    }

    pub fn org(&self) -> &str {
        &self.org
    }

    /// Publish a fire-and-forget event under
    /// `hackline/<zid>/msg/event/<topic>`. Best-effort delivery: the
    /// reliable Zenoh transport guarantees in-order delivery while
    /// the link is up, but the gateway will miss anything published
    /// during an offline window (SCOPE.md §8.1).
    pub async fn publish_event(
        &self,
        topic: &str,
        payload: serde_json::Value,
    ) -> Result<(), ClientError> {
        validate_topic(topic)?;
        let env = MsgEnvelope::new_event(payload);
        let ke = keyexpr::msg_event(&self.org, &self.zid, topic);
        self.publish(&ke, &env).await
    }

    /// Publish a structured log under
    /// `hackline/<zid>/msg/log/<topic>`. Same delivery semantics as
    /// `publish_event`; the gateway routes it to the `logs` table
    /// instead of `events` purely by keyexpr.
    pub async fn publish_log(
        &self,
        level: LogLevel,
        topic: &str,
        payload: serde_json::Value,
    ) -> Result<(), ClientError> {
        validate_topic(topic)?;
        let env = MsgEnvelope::new_log(level, payload);
        let ke = keyexpr::msg_log(&self.org, &self.zid, topic);
        self.publish(&ke, &env).await
    }

    async fn publish(&self, ke: &str, env: &MsgEnvelope) -> Result<(), ClientError> {
        let bytes = serde_json::to_vec(env)?;
        self.inner
            .put(ke.to_owned(), ZBytes::from(bytes))
            .await
            .map_err(ClientError::Zenoh)?;
        Ok(())
    }

    /// Subscribe to durable commands on
    /// `hackline/<own-zid>/msg/cmd/<topic>`. Topic may contain Zenoh
    /// wildcards (`*` / `**`) on the subscribe side per SCOPE.md
    /// §5.5; pass `**` to receive every cmd topic for this device.
    ///
    /// Returns a stream that yields `CmdHandle` items; each handle
    /// owns the matching ack keyexpr so the caller cannot smuggle a
    /// wrong `cmd_id` back. At-least-once delivery: the SDK does not
    /// dedupe across redeliveries — the device app checks the
    /// `cmd_id` against its own idempotency record before doing the
    /// work (SCOPE.md §8.1).
    pub async fn subscribe_cmd(&self, topic: &str) -> Result<CmdStream, ClientError> {
        validate_topic_subscribe(topic)?;
        let ke = keyexpr::msg_cmd(&self.org, &self.zid, topic);
        let sub = self
            .inner
            .declare_subscriber(ke)
            .await
            .map_err(ClientError::Zenoh)?;
        Ok(CmdStream {
            session: self.inner.clone(),
            org: self.org.clone(),
            zid: self.zid.clone(),
            sub,
        })
    }

    /// Serve a synchronous typed RPC on
    /// `hackline/<own-zid>/msg/api/<topic>`. The handler is invoked
    /// for every incoming query; its returned `ApiReply` is JSON-
    /// encoded and sent as the single query reply.
    ///
    /// Returns a `ServingApi` guard — drop it to undeclare the
    /// queryable. Spawning the handler internally is the caller's
    /// choice; this method awaits the next query in a loop on a
    /// dedicated task it spawns itself.
    pub async fn serve_api<F, Fut>(
        &self,
        topic: &str,
        mut handler: F,
    ) -> Result<ServingApi, ClientError>
    where
        F: FnMut(ApiRequest) -> Fut + Send + 'static,
        Fut: Future<Output = Result<ApiReply, ClientError>> + Send,
    {
        validate_topic(topic)?;
        let ke = keyexpr::msg_api(&self.org, &self.zid, topic);
        let q = self
            .inner
            .declare_queryable(ke.clone())
            .await
            .map_err(ClientError::Zenoh)?;
        let join = tokio::spawn(async move {
            while let Ok(query) = q.recv_async().await {
                let req_bytes = query
                    .payload()
                    .map(|p| p.to_bytes().to_vec())
                    .unwrap_or_default();
                let req: ApiRequest = match serde_json::from_slice(&req_bytes) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!("api request decode failed: {e}");
                        continue;
                    }
                };
                let reply = match handler(req).await {
                    Ok(r) => r,
                    Err(e) => ApiReply::json(serde_json::json!({ "error": e.to_string() })),
                };
                let bytes = match serde_json::to_vec(&reply) {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::warn!("api reply encode failed: {e}");
                        continue;
                    }
                };
                let key = query.key_expr().clone();
                if let Err(e) = query.reply(key, bytes).await {
                    tracing::warn!("api reply send failed: {e}");
                }
            }
        });
        Ok(ServingApi {
            _join: ServingApiHandle(Some(join)),
        })
    }
}

/// Stream of incoming commands. Yields `CmdHandle` values via
/// `recv()`. Drop the stream to stop receiving new commands.
pub struct CmdStream {
    session: Arc<Session>,
    org: String,
    zid: Zid,
    sub: zenoh::pubsub::Subscriber<zenoh::handlers::FifoChannelHandler<zenoh::sample::Sample>>,
}

impl CmdStream {
    /// Await the next command. Returns `None` when the underlying
    /// Zenoh subscriber closes (session dropped or undeclared).
    pub async fn recv(&self) -> Option<CmdHandle> {
        loop {
            let sample = self.sub.recv_async().await.ok()?;
            let bytes = sample.payload().to_bytes().to_vec();
            let env: CmdEnvelope = match serde_json::from_slice(&bytes) {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("cmd envelope decode failed: {e}");
                    continue;
                }
            };
            return Some(CmdHandle {
                session: self.session.clone(),
                org: self.org.clone(),
                zid: self.zid.clone(),
                cmd_id: env.cmd_id,
                envelope: env,
            });
        }
    }
}

/// One in-flight command. The handle owns the matching cmd-ack
/// keyexpr so the call site cannot publish an ack with the wrong
/// `cmd_id`.
pub struct CmdHandle {
    session: Arc<Session>,
    org: String,
    zid: Zid,
    pub cmd_id: uuid::Uuid,
    pub envelope: CmdEnvelope,
}

impl CmdHandle {
    pub fn payload(&self) -> &serde_json::Value {
        &self.envelope.envelope.payload
    }

    pub fn topic(&self) -> &str {
        &self.envelope.topic
    }

    /// Publish a `done`/`failed`/etc ack. Best-effort like every
    /// other message-plane publish; the gateway sees redeliveries
    /// until it observes the ack.
    pub async fn ack(&self, result: CmdResult) -> Result<(), ClientError> {
        self.ack_with(result, None).await
    }

    pub async fn ack_with(
        &self,
        result: CmdResult,
        detail: Option<String>,
    ) -> Result<(), ClientError> {
        let ack = CmdAck {
            cmd_id: self.cmd_id,
            result,
            detail,
        };
        let bytes = serde_json::to_vec(&ack)?;
        let ke = keyexpr::msg_cmd_ack(&self.org, &self.zid, &self.cmd_id);
        self.session
            .put(ke, ZBytes::from(bytes))
            .await
            .map_err(ClientError::Zenoh)?;
        Ok(())
    }
}

/// Guard returned by `serve_api`; drop to stop serving.
pub struct ServingApi {
    _join: ServingApiHandle,
}

struct ServingApiHandle(Option<tokio::task::JoinHandle<()>>);
impl Drop for ServingApiHandle {
    fn drop(&mut self) {
        if let Some(h) = self.0.take() {
            h.abort();
        }
    }
}

/// Permissive variant for subscribe paths: `*` and `**` segments
/// are allowed (Zenoh wildcards), but `/` inside a segment isn't.
fn validate_topic_subscribe(topic: &str) -> Result<(), ClientError> {
    if topic.is_empty() {
        return Err(ClientError::Topic("topic must not be empty".into()));
    }
    for seg in topic.split('.') {
        if seg.is_empty() {
            return Err(ClientError::Topic(format!(
                "empty segment in topic `{topic}`"
            )));
        }
        if seg.contains('/') {
            return Err(ClientError::Topic(format!(
                "segment `{seg}` contains reserved character"
            )));
        }
    }
    Ok(())
}

/// Reject topics that would break the keyexpr conversion or smuggle
/// wildcards on a publish path (SCOPE.md §5.5).
fn validate_topic(topic: &str) -> Result<(), ClientError> {
    if topic.is_empty() {
        return Err(ClientError::Topic("topic must not be empty".into()));
    }
    for seg in topic.split('.') {
        if seg.is_empty() {
            return Err(ClientError::Topic(format!(
                "empty segment in topic `{topic}`"
            )));
        }
        if seg.contains('/') || seg.contains('*') {
            return Err(ClientError::Topic(format!(
                "segment `{seg}` contains reserved character"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_validation() {
        assert!(validate_topic("foo").is_ok());
        assert!(validate_topic("foo.bar.baz").is_ok());
        assert!(validate_topic("").is_err());
        assert!(validate_topic(".foo").is_err());
        assert!(validate_topic("foo..bar").is_err());
        assert!(validate_topic("foo.*").is_err());
        assert!(validate_topic("foo/bar").is_err());
    }
}
