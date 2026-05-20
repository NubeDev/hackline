# 2026-05-14 — Goal 0: Zenoh bridge spike

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Add workspace deps (zenoh, tokio, serde, uuid) | [x] |
| 1 | hackline-proto: wire types (Zid, ConnectRequest/Ack, keyexpr builders) | [x] |
| 2 | hackline-core: pub/sub bridge (paired up/down channels per connection) | [x] |
| 3 | Spike example: two peers in one process, TCP echo through Zenoh | [x] |
| 4 | cargo check + test, verify with netcat | [x] |

## Design decision

Zenoh 1.x query/reply is request-response, not a streaming bidi
channel. The bridge uses the pub/sub fallback from SCOPE §11.1:

1. Gateway sends a `get` on `hackline/<zid>/tcp/<port>/connect`
2. Agent's queryable opens `127.0.0.1:<port>`, replies with
   `ConnectAck { ok, request_id }`
3. Data flows on paired pub/sub:
   - `hackline/<zid>/stream/<request_id>/gw` (gateway → agent)
   - `hackline/<zid>/stream/<request_id>/dev` (agent → gateway)
4. Either side closing publishes a zero-length sentinel, then drops.

One query per connection for the handshake; pub/sub for the byte
stream. `request_id` is a UUID.

## Outcome

Goal 0 complete. Bytes flow end-to-end:
`nc → gateway TCP → Zenoh pub/sub → agent → local echo server → Zenoh pub/sub → gateway → nc`

Key findings:
- Zenoh query/reply must be explicitly dropped after replying, otherwise
  the gateway's `get()` holds the query slot open until timeout and
  emits noisy "Didn't receive final reply" warnings.
- Split into two timeouts: `QUERY_TIMEOUT` (2s) for the Zenoh get, and
  `ACK_TIMEOUT` (10s) as the outer tokio deadline. The shorter query
  timeout lets Zenoh finalize the query before the data transfer begins.
- 32KB read buffer in `run_bridge` — sufficient for interactive
  protocols, may need tuning for bulk transfers later.
