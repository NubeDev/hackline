# 2026-05-14 — Goal 5: Commands, api, HTTP host-routing

## Plan

| # | Step | Status |
|---|---|---|
| 0  | V003 migration: `cmd_outbox` table per SCOPE.md §7.2 (UUID `cmd_id`, payload BLOB, TTL, ack columns, attempts, partial index on pending) | [x] |
| 1  | `hackline-proto::msg`: `CmdEnvelope`, `CmdAck`, `CmdResult`, `ApiRequest`, `ApiReply` wire types + JSON round-trip tests | [x] |
| 2  | `hackline-proto::keyexpr`: `msg_cmd`, `msg_cmd_ack`, `msg_api`, fan-in patterns (`MSG_CMD_ACK_FANIN`), `parse_msg_cmd_ack_keyexpr` | [x] |
| 3  | `db::cmd_outbox` repository: `enqueue` (cap + TTL enforced at write), `mark_delivered`, `record_ack`, `list_by_device`, `get_by_cmd_id`, `cancel`, `list_pending`, `list_pending_for_device` | [x] |
| 4  | `cmd_delivery`: gateway task subscribed to `hackline/*/msg/cmd-ack/**`, writes ack rows; second task drains pending outbox on enqueue trigger + on liveliness online | [x] |
| 5  | `hackline-client`: `subscribe_cmd(topic)` returning a stream of `CmdHandle` with `ack()` / `ack_with()`; dedup is caller's job (SCOPE.md §8.1 at-least-once) | [x] |
| 6  | `hackline-client`: `serve_api(topic, handler)` — declares a queryable, calls handler with `ApiRequest`, replies with `ApiReply` JSON | [x] |
| 7  | REST: `POST /v1/devices/:id/cmd/:topic` — enqueues row, sends delivery trigger, returns `{ cmd_id }` | [x] |
| 8  | REST: `GET /v1/devices/:id/cmd?status=...` cursor-paginated; `DELETE /v1/cmd/:cmd_id` to cancel a queued row | [x] |
| 9  | REST: `POST /v1/devices/:id/api/:topic` — synchronous Zenoh `get` against `hackline/<zid>/msg/api/<topic>` with `timeout_ms`; returns `{ reply, content_type }`; maps liveliness-miss to `503 device_unreachable` | [x] |
| 10 | HTTP host-routing: shared axum listener accepts requests on the configured public HTTP port; the `Host:` header `device-<id>.<base>` selects an `http` tunnel and proxies bytes (incl. WebSocket upgrade) through the existing TCP bridge | [x] |
| 11 | `customer` role + per-device-scope enforcement: scope helper in `auth::scope`, applied to `/v1/devices/:id/cmd/*`, `/api/*`, and the HTTP host-router | [x] |
| 12 | CLI: `hackline cmd send`, `hackline cmd list`, `hackline cmd cancel`, `hackline api call` | [x] |
| 13 | Integration test (`cmd_round_trip`): publish via SDK `subscribe_cmd`, gateway delivers, device acks, outbox row reaches `acked`. Plus `api_round_trip`: SDK `serve_api` echoes a payload through the synchronous REST endpoint | [x] |
| 14 | `cargo check --workspace` clean; `cargo test --workspace` green | [x] |

## Outcome

End-to-end Phase 2 demoable.

`tests/cmd_plane.rs::cmd_round_trip`:
1. Open in-memory SQLite DB, run migrations through V003.
2. Insert a `devices` row for ZID `bb22`.
3. Open two `zenoh::Session`s on loopback ephemeral ports — "device"
   and "gateway".
4. Spawn `cmd_delivery::spawn` against the gateway session.
5. Build a `ClientSession` on the device session and call
   `subscribe_cmd("block.install")` — the stream yields `CmdHandle`s.
6. Enqueue a command via `db::cmd_outbox::enqueue` and fire the
   delivery trigger.
7. The device-side stream sees the `CmdEnvelope`; the test handler
   calls `cmd.ack(CmdResult::Done)`; the gateway's cmd-ack
   subscriber writes the ack into the outbox row.
8. Polling `cmd_outbox::get_by_cmd_id` until `ack_result = 'done'`
   confirms the round-trip within 5 s.

`tests/cmd_plane.rs::api_round_trip`:
1. Same two-peer setup.
2. Device SDK `serve_api("ping", |req| Ok(ApiReply::json({ "pong": req.payload })))`.
3. Gateway issues a Zenoh `get` against `hackline/<zid>/msg/api/ping`
   with a JSON request; receives the reply within the timeout;
   returns it through the synchronous REST handler.

Manual demo (separate processes):

```bash
# Terminal 1 — gateway
cargo run -p hackline-gateway --bin serve -- gateway.toml

# Terminal 2 — device app (uses hackline-client)
#   subscribes block.install, serves api/ping

# Terminal 3 — operator
hackline cmd send --device 1 --topic block.install \
                  --payload '{"block":"foo","version":"1.2.3"}'
# -> { "cmd_id": "..." }

hackline cmd list --device 1
# -> table with status=pending|delivered|acked

curl -s -H "Authorization: Bearer $TOK" \
     -X POST \
     -H 'content-type: application/json' \
     -d '{"payload":{"x":1},"timeout_ms":2000}' \
     http://127.0.0.1:8080/v1/devices/1/api/ping | jq

# HTTP host-routing
curl -H 'Host: device-1.cloud.example.com' http://127.0.0.1:8081/
# -> response from the device's local :8080 admin UI
```

`cargo check --workspace` clean (two pre-existing dead-code warnings
in `hackline-agent` remain).  `cargo test --workspace` green: proto
round-trip tests, glob/keyexpr unit tests, `cmd_round_trip`,
`api_round_trip`, and the existing `event_round_trip` all pass.

## Design

**Push-on-enqueue + push-on-online**. SCOPE.md §14 Q2 left the
delivery shape open; the chosen strawman is push-on-enqueue (the REST
handler that writes the outbox row also fires an `mpsc` trigger that
wakes the per-device delivery task) plus a periodic sweep that
re-publishes anything in `pending` when the device's liveliness
token reappears. Zenoh's reliable transport handles the in-link
retransmits; SQLite owns the durable replay across device-offline
windows. Pull-from-device was rejected because it would require the
device to advertise a "ready to receive cmd N" queryable and the
gateway to keep per-device offset state — strictly more moving parts
for the same semantics.

**`cmd-ack` is one fan-in subscriber, not per-device.** Mirrors the
events/logs fan-in: `hackline/*/msg/cmd-ack/**` subscribed once at
boot, sample parsed back to `(zid, cmd_id)`, `record_ack` writes the
result. Lets a single device serve many cmd-ack publishers (one per
in-flight handler) without the gateway tracking which subscriber
covers which cmd_id.

**`subscribe_cmd` yields handles, not raw envelopes.** The SDK ack
contract — publish `CmdAck { cmd_id, result, detail }` on
`hackline/<own-zid>/msg/cmd-ack/<cmd_id>` — is easy to get wrong from
the call site. Wrapping the envelope in `CmdHandle { cmd_id,
envelope, ack(...) }` puts the keyexpr and the envelope shape inside
the SDK so app code can't smuggle the wrong cmd_id back. Dedup
remains the caller's job because the SDK doesn't have durable state
on the device — SCOPE.md §8.1 says explicitly that
"at-least-once" with `cmd_id` as the idempotency key is the contract.

**`serve_api` keyexpr is one queryable per topic, not one wildcard
queryable**. A wildcard queryable forces the SDK to demux on the
key inside the handler, which loses the per-topic typed handler
ergonomics. One queryable per call to `serve_api` matches the SCOPE.md
example exactly and keeps the per-topic logic isolated; the cost is
one declared queryable per typed RPC, which Zenoh handles trivially.

**Synchronous `/v1/devices/:id/api/:topic` uses Zenoh `get` directly,
not a pub/sub round-trip.** Already the right shape — request/reply
maps onto Zenoh queryables, and the bridge code in
`hackline-core::bridge` already proves the pattern (one query, one
reply, drop the receiver). Liveliness-miss becomes `503
device_unreachable`; query-timeout becomes `504 device_timeout`.

**HTTP host-routing is one shared axum listener that proxies into
the existing TCP bridge.** Building a separate hyper-based router
would duplicate the bytes-through-Zenoh path that
`tunnel::bridge::initiate_bridge` already implements. Instead, the
HTTP host-routing listener accepts a TCP connection, peeks the
`Host:` header out of the first request line (single-pass read, no
hyper parser), looks up the matching `http` tunnel, and then runs
the bytes through `initiate_bridge` exactly like a TCP tunnel.
WebSocket `Upgrade` works for free because we are not framing — the
upgrade headers and the subsequent WS frames are bytes through the
same pipe. Keep-alive across different hostnames on a single TCP
connection is not supported (the client must open a new connection
for a different host); HTTP/2 host-routing is Phase 3.

**Customer role enforcement at the edge.** The existing
`AuthedUser` extractor returns the row; `auth::scope` grows
`check_device(&user, device_id) -> Result<()>` that returns
`Unauthorized` if the user's role is `customer` and the requested
`device_id` is not in their `device_scope` JSON array. Applied at
the four entry points where a customer could reach a device:
`POST .../cmd/*`, `GET .../cmd`, `POST .../api/*`, and the HTTP
host-router. Owner / admin / support / viewer keep their existing
behaviour (`device_scope = '*'`).

**Cmd payload size cap mirrors the table CHECK.** SCOPE.md §7.2
caps `payload` at 64 KiB; the REST handler refuses the request with
`413 payload_too_large` rather than letting SQLite raise a CHECK
constraint failure at INSERT time.

**Pruning at write time.** SCOPE.md §7.3 requires write-time
enforcement of per-device caps on `cmd_outbox`. `db::cmd_outbox::enqueue`
opens a transaction, refuses if the device already has
`CMD_MAX_PER_DEVICE` rows whose oldest is not yet past `expires_at`,
inserts, commits. The background sweep handles TTL expiry of stale
delivered rows (SCOPE.md §7.3 `cmd.history_days`).
