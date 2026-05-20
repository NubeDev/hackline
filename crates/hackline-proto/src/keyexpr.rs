//! Key-expression builders. The catalogue is in `DOCS/KEYEXPRS.md`;
//! every keyexpr in that table is built by exactly one function in
//! this file so a typo can't slip into the wire.
//!
//! Every key starts with `hackline/<org_slug>/<zid>/...` (SCOPE.md
//! §5.1, §13 Phase 4). The org slug is the tenant boundary; the
//! Zenoh ACL grants each org its own `hackline/<slug>/**` subtree so
//! a compromised device in one org cannot even subscribe to another
//! org's traffic.

use uuid::Uuid;

use crate::zid::Zid;

/// `hackline/<org>/<zid>/tcp/<port>/connect`
pub fn connect(org: &str, zid: &Zid, port: u16) -> String {
    format!("hackline/{org}/{zid}/tcp/{port}/connect")
}

/// `hackline/<org>/<zid>/info`
pub fn info(org: &str, zid: &Zid) -> String {
    format!("hackline/{org}/{zid}/info")
}

/// `hackline/<org>/<zid>/health`
pub fn health(org: &str, zid: &Zid) -> String {
    format!("hackline/{org}/{zid}/health")
}

/// Gateway-side wildcard for liveliness watchers — every org's
/// every device. Match keys parse with `parse_health_keyexpr`.
pub const HEALTH_FANIN: &str = "hackline/*/*/health";

/// Parse a concrete health keyexpr `hackline/<org>/<zid>/health`
/// back into `(org, zid)`. Returns `None` on shape mismatch.
pub fn parse_health_keyexpr(ke: &str) -> Option<(String, Zid)> {
    let mut parts = ke.split('/');
    if parts.next()? != "hackline" {
        return None;
    }
    let org = parts.next()?.to_owned();
    let zid_raw = parts.next()?;
    if parts.next()? != "health" {
        return None;
    }
    if parts.next().is_some() {
        return None;
    }
    let zid = Zid::new(zid_raw).ok()?;
    Some((org, zid))
}

/// `hackline/<org>/<zid>/stream/<request_id>/gw` — gateway → agent data.
pub fn stream_gw(org: &str, zid: &Zid, request_id: &Uuid) -> String {
    format!("hackline/{org}/{zid}/stream/{request_id}/gw")
}

/// `hackline/<org>/<zid>/stream/<request_id>/dev` — agent → gateway data.
pub fn stream_dev(org: &str, zid: &Zid, request_id: &Uuid) -> String {
    format!("hackline/{org}/{zid}/stream/{request_id}/dev")
}

/// Dotted topic → keyexpr suffix. `graph.slot.temp.changed` →
/// `graph/slot/temp/changed`. SCOPE.md §5.5 forbids `.` inside a
/// topic segment; callers validate before publishing.
pub fn topic_to_keyexpr_suffix(topic: &str) -> String {
    topic.replace('.', "/")
}

/// `hackline/<org>/<zid>/msg/event/<topic-as-keyexpr>`
pub fn msg_event(org: &str, zid: &Zid, topic: &str) -> String {
    format!(
        "hackline/{org}/{zid}/msg/event/{}",
        topic_to_keyexpr_suffix(topic)
    )
}

/// `hackline/<org>/<zid>/msg/log/<topic-as-keyexpr>`
pub fn msg_log(org: &str, zid: &Zid, topic: &str) -> String {
    format!(
        "hackline/{org}/{zid}/msg/log/{}",
        topic_to_keyexpr_suffix(topic)
    )
}

/// Gateway-side fan-in subscription for every org's events.
pub const MSG_EVENT_FANIN: &str = "hackline/*/*/msg/event/**";

/// Gateway-side fan-in subscription for every org's logs.
pub const MSG_LOG_FANIN: &str = "hackline/*/*/msg/log/**";

/// Gateway-side fan-in subscription for every org's cmd acks.
pub const MSG_CMD_ACK_FANIN: &str = "hackline/*/*/msg/cmd-ack/**";

/// `hackline/<org>/<zid>/msg/cmd/<topic-as-keyexpr>` — gateway → device.
pub fn msg_cmd(org: &str, zid: &Zid, topic: &str) -> String {
    format!(
        "hackline/{org}/{zid}/msg/cmd/{}",
        topic_to_keyexpr_suffix(topic)
    )
}

/// `hackline/<org>/<zid>/msg/cmd-ack/<cmd_id>` — device → gateway.
pub fn msg_cmd_ack(org: &str, zid: &Zid, cmd_id: &uuid::Uuid) -> String {
    format!("hackline/{org}/{zid}/msg/cmd-ack/{cmd_id}")
}

/// Wildcard subscriber used by `subscribe_cmd` on the device side.
/// Matches every topic under `hackline/<org>/<own-zid>/msg/cmd/<topic>`
/// for a given dotted topic prefix.
pub fn msg_cmd_sub(org: &str, zid: &Zid, topic: &str) -> String {
    msg_cmd(org, zid, topic)
}

/// `hackline/<org>/<zid>/msg/api/<topic-as-keyexpr>` — queryable on
/// device, `get` from gateway.
pub fn msg_api(org: &str, zid: &Zid, topic: &str) -> String {
    format!(
        "hackline/{org}/{zid}/msg/api/{}",
        topic_to_keyexpr_suffix(topic)
    )
}

/// Parse a cmd-ack keyexpr `hackline/<org>/<zid>/msg/cmd-ack/<cmd_id>`
/// back into `(org, zid, cmd_id)`. Returns `None` on shape mismatch.
pub fn parse_msg_cmd_ack_keyexpr(ke: &str) -> Option<(String, Zid, uuid::Uuid)> {
    let mut parts = ke.split('/');
    if parts.next()? != "hackline" {
        return None;
    }
    let org = parts.next()?.to_owned();
    let zid_raw = parts.next()?;
    if parts.next()? != "msg" {
        return None;
    }
    if parts.next()? != "cmd-ack" {
        return None;
    }
    let cmd_id_raw = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let zid = Zid::new(zid_raw).ok()?;
    let cmd_id = uuid::Uuid::parse_str(cmd_id_raw).ok()?;
    Some((org, zid, cmd_id))
}

/// Inbound message-plane keyexpr kinds the gateway recognises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgKind {
    Event,
    Log,
}

/// Parse a concrete inbound key like
/// `hackline/<org>/<zid>/msg/event/foo/bar` back into
/// `(org, zid, kind, dotted-topic)`. Returns `None` if the shape
/// doesn't match — callers log-and-drop rather than crash on a
/// malformed publication from an untrusted device.
pub fn parse_msg_keyexpr(ke: &str) -> Option<(String, Zid, MsgKind, String)> {
    let mut parts = ke.split('/');
    if parts.next()? != "hackline" {
        return None;
    }
    let org = parts.next()?.to_owned();
    let zid_raw = parts.next()?;
    if parts.next()? != "msg" {
        return None;
    }
    let kind = match parts.next()? {
        "event" => MsgKind::Event,
        "log" => MsgKind::Log,
        _ => return None,
    };
    let rest: Vec<&str> = parts.collect();
    if rest.is_empty() {
        return None;
    }
    let zid = Zid::new(zid_raw).ok()?;
    let topic = rest.join(".");
    Some((org, zid, kind, topic))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyexpr_shape() {
        let zid = Zid::new("aabb").unwrap();
        assert_eq!(
            connect("acme", &zid, 22),
            "hackline/acme/aabb/tcp/22/connect"
        );
        assert_eq!(info("acme", &zid), "hackline/acme/aabb/info");
        assert_eq!(health("acme", &zid), "hackline/acme/aabb/health");

        let rid = Uuid::nil();
        assert_eq!(
            stream_gw("acme", &zid, &rid),
            "hackline/acme/aabb/stream/00000000-0000-0000-0000-000000000000/gw"
        );
    }

    #[test]
    fn msg_keyexpr_round_trip() {
        let zid = Zid::new("aabb").unwrap();
        let ke = msg_event("acme", &zid, "graph.slot.temp.changed");
        assert_eq!(ke, "hackline/acme/aabb/msg/event/graph/slot/temp/changed");

        let (org, z, kind, topic) = parse_msg_keyexpr(&ke).unwrap();
        assert_eq!(org, "acme");
        assert_eq!(z.as_str(), "aabb");
        assert_eq!(kind, MsgKind::Event);
        assert_eq!(topic, "graph.slot.temp.changed");

        let log_ke = msg_log("acme", &zid, "audit.entry");
        let (org, _, kind, topic) = parse_msg_keyexpr(&log_ke).unwrap();
        assert_eq!(org, "acme");
        assert_eq!(kind, MsgKind::Log);
        assert_eq!(topic, "audit.entry");
    }

    #[test]
    fn parse_rejects_bad_shapes() {
        assert!(parse_msg_keyexpr("hackline/acme/aabb/msg/event").is_none());
        assert!(parse_msg_keyexpr("hackline/acme/aabb/info").is_none());
        assert!(parse_msg_keyexpr("hackline/acme/ZZ/msg/event/x").is_none());
        assert!(parse_msg_keyexpr("hackline/acme/aabb/msg/cmd/x").is_none());
    }
}
