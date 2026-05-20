# Zenoh key expressions

The agent ↔ gateway wire surface. Builders live in
[`hackline-proto::keyexpr`](../crates/hackline-proto/src/keyexpr.rs);
payload schemas are in the sibling files.

## Catalogue

| Key expression | Direction | Payload |
|---|---|---|
| `hackline/<zid>/info` | gateway → agent (query) | `AgentInfo` reply |
| `hackline/<zid>/tcp/<port>/connect` | gateway → agent (query) | `ConnectRequest` / `ConnectAck` |
| `hackline/<zid>/health` | agent (liveliness token) | — |

Where `<zid>` is the device's Zenoh ID, canonicalised as lowercase hex
(no separators, length 2..=32). Validation lives in
[`hackline-proto::zid::Zid`](../crates/hackline-proto/src/zid.rs).

## Streaming bytes

The Phase 1 spike validates which of two shapes Zenoh 1.x supports
cleanly:

1. **Streaming query reply.** One `get` whose reply channel carries
   raw bytes in both directions for the lifetime of the TCP
   connection.
2. **Paired pub/sub.** `connect` returns a `request_id`; data flows
   on `hackline/<zid>/tcp/<port>/<request_id>/up` (gateway → agent)
   and `…/down` (agent → gateway) until either side closes.

If shape 1 is unworkable, shape 2 is the documented fallback. Either
way, `hackline-proto` carries the schema and `hackline-core` owns the
bridging code so callers see the same trait.

## ACL

The agent's Zenoh ACL must restrict it to **serving**
`hackline/<own-zid>/**`. It must not be permitted to *query*
`hackline/**` — otherwise a compromised device can scan its peers.

The gateway is the only principal authorised to query
`hackline/*/**`. Concentration of trust on the gateway is a stated
property; see [`AUTH.md`](./AUTH.md).
