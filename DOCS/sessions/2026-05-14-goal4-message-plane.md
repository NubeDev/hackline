# 2026-05-14 — Goal 4: Message plane (events + logs)

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Add V002 migration with `events` + `logs` tables (ring-buffer shape per SCOPE.md §7.2) | [x] |
| 1 | Multi-migration runner: iterate over `(version, name, sql)`, append-only | [x] |
| 2 | `hackline-proto::msg::{MsgEnvelope, LogLevel}` + reserved `level` header | [x] |
| 3 | `hackline-proto::keyexpr` helpers: `msg_event`, `msg_log`, `MSG_*_FANIN`, `parse_msg_keyexpr`, `topic_to_keyexpr_suffix` | [x] |
| 4 | New `hackline-client` crate: `ClientSession::publish_event`, `publish_log`, topic validation | [x] |
| 5 | `db::events` + `db::logs` repositories: insert-with-prune in one txn, cursor-paginated `list` with topic GLOB / `level` / `since` / `cursor` filters | [x] |
| 6 | `events_bus::{MsgBus, MsgEvent}` — `tokio::sync::broadcast` carrying persisted `EventRow` / `LogRow` | [x] |
| 7 | `msg_fanin::spawn` — two wildcard subscribers (`hackline/*/msg/event/**`, `.../log/**`), parse keyexpr, resolve zid→device_id, persist, broadcast on bus | [x] |
| 8 | REST: `GET /v1/events`, `GET /v1/log` cursor APIs returning `{ entries, next_cursor }` | [x] |
| 9 | SSE: `GET /v1/events/stream`, `GET /v1/log/stream` with in-process glob filter on `topic` and `device` | [x] |
| 10 | Wire `MsgBus` into `AppState`; serve.rs spawns the fan-in tasks before binding axum | [x] |
| 11 | CLI: `hackline events {tail,history}` + `hackline log {tail,history}` using shared `StreamKind` enum | [x] |
| 12 | Integration test: two in-process Zenoh peers, client publishes, gateway persists+broadcasts, cursor API matches | [x] |
| 13 | `cargo check --workspace` clean; `cargo test --workspace` green | [x] |

## Outcome

End-to-end message plane working. Verified by
`tests/message_plane.rs::event_round_trip`:

1. Open an in-memory SQLite DB, run migrations through V002.
2. Insert a `devices` row for ZID `aa11`.
3. Open two `zenoh::Session`s in `peer` mode on loopback (ephemeral
   ports) — the "device" session and the "gateway" session.
4. Spawn `msg_fanin::spawn(gw_session, db, bus)` — two wildcard
   subscribers come up.
5. Build a `ClientSession` against the device session and call
   `publish_event("graph.slot.temp.changed", {v: 21.4})` and
   `publish_log(Warn, "audit.entry", {msg: "hello"})`.
6. The fan-in task parses the keyexpr, looks up `device_id` for the
   ZID, persists into `events` / `logs`, and re-broadcasts on the
   `MsgBus`. The test's bus subscriber sees both rows within 5 s.
7. The cursor API (`db::events::list`, `db::logs::list`) returns the
   same rows — proves history and live agree on shape.

Manual demo (gateway + device app, separate processes):

```bash
# Terminal 1 — gateway
cargo run -p hackline-gateway --bin serve -- gateway.toml
# (claim, login, hackline device add --zid aa11 --label sensor-1)

# Terminal 2 — follow events live
hackline events tail --device 1

# Terminal 3 — publish from a device app (or the spike binary)
#   uses hackline_client::ClientSession::publish_event
# Tail prints: {"id":1,"device_id":1,"topic":"graph.slot.temp.changed",...}

# Cursor query for history
curl -s -H "Authorization: Bearer $TOK" \
     'http://127.0.0.1:8080/v1/events?device=1&topic=graph.*&limit=20' | jq
```

`cargo check --workspace` clean (two pre-existing dead-code warnings
in `hackline-agent` remain; no new warnings introduced).
`cargo test --workspace` green: proto round-trip + glob match unit
tests + the new integration test all pass.

## Design

**Why a single in-process `MsgBus` rather than per-device channels.**
SSE consumers vary their filter (device id, topic glob) per
connection; persisting rows once and broadcasting `EventRow` /
`LogRow` to one shared `tokio::sync::broadcast` lets every SSE task
filter independently without coordinating with the fan-in.
Lagged subscribers get `BroadcastStreamRecvError::Lagged` and are
expected to reconnect — they can replay missed history via the
cursor API, which is itself the durable contract.

**Keyexpr ↔ topic encoding.** SCOPE.md §5.5 says dotted topics map
to `/`-separated keyexpr suffixes. `hackline-proto::keyexpr`
centralises both directions (`topic_to_keyexpr_suffix` and
`parse_msg_keyexpr`) so no string concatenation happens in the
gateway, the client, or tests. Parsing rejects malformed shapes
(missing segments, non-hex ZIDs, wrong kind) and the fan-in logs and
drops rather than crashing on garbage.

**Ring-buffer pruning sits inside the insert transaction.** SCOPE.md
§7 requires that readers never see the table over-cap, so each
insert opens a transaction, inserts, runs the `DELETE ... LIMIT -1
OFFSET cap` pattern (rusqlite supports the OFFSET-without-LIMIT
trick), then commits. The cap is hard-coded at 10 000 rows per
device per table for v0.1; it will become configurable when the
gateway grows a retention config block.

**`MsgEnvelope` payload as `serde_json::Value`, not `bytes::Bytes`.**
SCOPE.md §5.2 reserves `content_type` for a future bincode swap, so
the wire is JSON for v0.1. Carrying the payload as `Value` keeps the
debuggability win (the SQLite BLOB is a JSON document the operator
can `SELECT` and read), avoids a `bytes` workspace dependency in
`hackline-proto`, and matches the cursor API's response shape one
to one. When bincode lands, payload becomes `Vec<u8>` and the API
shape changes only for binary content types.

**Log level lives in `headers.level`.** Events and logs share one
envelope shape on the wire — the only difference is the keyexpr
family and the addition of a `level` header on logs. The gateway
extracts the header into a dedicated SQL column so callers can
filter by level cheaply without parsing the BLOB. Defaulting to
`info` on a missing header keeps the table's CHECK constraint
satisfied even if a buggy client forgets the header.

**One CLI module for both stream families.** `hackline events tail`
and `hackline log tail` share their entire transport: the same
SSE-framing parser, the same auth headers, the same JSON line
output. `StreamKind` picks the path; everything else is shared.
History (`/v1/events`, `/v1/log`) is also one module with the same
table renderer. Adds two top-level commands without a parallel code
tree.

**Test harness uses OS-assigned ephemeral ports.** `peer_config`
binds two `TcpListener`s on `127.0.0.1:0`, captures the port
numbers, drops the sockets, then re-binds via Zenoh. This survives
concurrent `cargo test` runs and CI; the previous spike pattern of
hard-coded 7447/7448 would race against itself.

**`hackline-client` is its own crate even though the SDK surface is
small.** SCOPE.md §4 hard rule R2 forbids `hackline-client` depending
on the agent or the gateway. Splitting it now sets the dependency
direction correctly so Phase 2 (`subscribe_cmd`, `serve_api`) can
grow inside `hackline-client` without dragging gateway code into a
device-app binary.
