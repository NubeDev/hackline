# 2026-05-14 — Goal 6: Audit completeness + admin UI

## Plan

| # | Step | Status |
|---|---|---|
| 0  | V004 migration: nullable `ts_close` / `request_id` / `peer` / `bytes_up` / `bytes_down` columns on `audit`, plus `audit_request` index, so one row spans a `tunnel.session`'s open and close per SCOPE.md §7.2 | [x] |
| 1  | `db::audit`: split point-in-time `insert` from `insert_tunnel_session_open` + `finalize_tunnel_session`; `list_recent` returns the new columns; `count` for the `hackline_audit_rows` gauge | [x] |
| 2  | `hackline-core::bridge`: `run_bridge` tracks per-direction byte totals, `initiate_bridge` returns `BridgeBytes`, plus `initiate_bridge_with_id` for callers (tcp listener, http router) that also need the `request_id` | [x] |
| 3  | `tunnel::tcp_listener`: per-accepted-socket `run_bridged_connection` that opens a `tunnel.session` audit row, calls `initiate_bridge`, and finalises the row with byte counts on close; threads `tunnel_id` + `device_id` from the manager | [x] |
| 4  | `tunnel::http_router`: same book-ending around its inline bridge, counting both the peeked HTTP preamble and the streamed body in `bytes_up` | [x] |
| 5  | `tunnels::TunnelWithZid` carries `device_id` so the listener has both row ids without a second DB hop | [x] |
| 6  | REST: `cmd.send` audit + `hackline_cmd_total{outcome="accepted"}` on enqueue (`api/cmd/send.rs`) | [x] |
| 7  | REST: `cmd.cancel` audit + `hackline_cmd_total{outcome="cancelled"}` on successful DELETE (`api/cmd/cancel.rs`) | [x] |
| 8  | REST: `api.call` audit + `hackline_api_calls_total{topic,outcome}` on every terminal outcome — ok, `device_unreachable`, `device_timeout` (`api/api_call/call.rs`) | [x] |
| 9  | `metrics::Metrics` registry — hand-rolled counters / gauges; render produces Prometheus text-format covering every metric in SCOPE.md §10.2 | [x] |
| 10 | Wire metrics into `msg_fanin` (events / logs counters) and `cmd_delivery` (delivered + ack outcomes) | [x] |
| 11 | `GET /metrics` axum handler — admin-token gated via the existing `AuthedUser` extractor; refreshes `audit_rows` + `cmd_outbox_depth` from SQLite right before rendering | [x] |
| 12 | Static admin bundle under `crates/hackline-gateway/static/admin/`: `index.html`, `admin.css`, `admin.js`; embedded via `include_str!` and served at `GET /admin` + `/admin/{file}` | [x] |
| 13 | Admin bundle views: devices, live tunnels (joined off `/v1/audit` filtered to `tunnel.session`), cmd outbox (`/v1/devices/:id/cmd`), live events SSE, audit, raw `/metrics` text | [x] |
| 14 | `cargo check --workspace` clean; `cargo test --workspace` green | [x] |

## Outcome

End-to-end Phase 3 demoable.

`cargo check --workspace` — clean; the two pre-existing dead-code
warnings in `hackline-agent` (`label`, `PortDenied`) remain, no new
warnings.

`cargo test --workspace` — every prior suite still green:

```
proto round-trip tests (cmd_round_trip, cmd_result_serde, ...)
keyexpr parse + render
http_router::tests::host_header_parsing + host_header_missing
gateway tests/cmd_plane.rs   (cmd_round_trip, api_round_trip)
gateway tests/message_plane.rs (event_round_trip, log_round_trip)
```

Manual demo (single gateway, real bridged connection):

```bash
# Terminal 1 — gateway
cargo run -p hackline-gateway --bin serve -- gateway.toml

# Terminal 2 — operator: register a device, mint a tcp tunnel
hackline device add --zid aabb --label demo
hackline tunnel add --device 1 --tcp 22 --public-port 2222

# Terminal 3 — drive bytes through the bridge
echo hello | nc 127.0.0.1 2222

# Terminal 4 — read the tunnel.session audit row back
curl -sH "Authorization: Bearer $TOK" \
     'http://127.0.0.1:8080/v1/audit?limit=5' | jq '.[] |
       select(.action=="tunnel.session")'
# => one row with ts, ts_close, request_id, peer, bytes_up, bytes_down

# cmd.send / cmd.cancel / api.call audit emission
hackline cmd send   --device 1 --topic block.install --payload '{}'
hackline cmd cancel --cmd-id <id>
curl -sH "Authorization: Bearer $TOK" \
     -X POST -H 'content-type: application/json' \
     -d '{"payload":{},"timeout_ms":1000}' \
     http://127.0.0.1:8080/v1/devices/1/api/ping
curl -sH "Authorization: Bearer $TOK" \
     'http://127.0.0.1:8080/v1/audit?limit=10' | jq '.[].action' | sort -u
# => "api.call" "cmd.cancel" "cmd.send" "tunnel.session"

# Prometheus exposition
curl -sH "Authorization: Bearer $TOK" http://127.0.0.1:8080/metrics \
  | grep -E '^# (HELP|TYPE) hackline_'
# => one HELP + one TYPE for every metric in SCOPE.md §10.2:
#    hackline_devices_online, hackline_tunnel_sessions_total,
#    hackline_tunnel_active, hackline_tunnel_bytes_total,
#    hackline_cmd_outbox_depth, hackline_cmd_total,
#    hackline_events_received_total, hackline_logs_received_total,
#    hackline_api_calls_total, hackline_audit_rows.

# Admin UI
open http://127.0.0.1:8080/admin
# => paste the bearer token; the six tabs load against the existing
#    REST and SSE surface (no new wire was added beyond /metrics and
#    /admin/*).
```

## Design

**One audit row per tunnel session, not two.** SCOPE.md §7.2 spells
out "one bridged TCP connection (open + close share a row)". The V001
schema only had `ts` + `action` + `detail`, so V004 adds the missing
session-specific columns as NULLABLE. Point-in-time actions
(`cmd.send`, `api.call`, `device.create`, ...) keep using the
existing `insert(...)` with `ts` + `action` + `detail`; only
`tunnel.session` uses `insert_tunnel_session_open` + `finalize_…`.
Two functions instead of one keeps the per-action call sites
declarative (the handlers don't carry a "what shape is this audit
row" branch) and means every other action stays a single
fire-and-forget `INSERT`.

A JSON `detail` blob carrying the byte counters would have worked at
the SQL level but lost the admin UI a sortable column, so it was
rejected. Per-event logging (one row at open, one at close) was
rejected per the SCOPE.md note "Per-event logging would be hundreds
of millions of rows/year at fleet scale."

**Byte counters tracked inside `run_bridge`, returned to the
caller.** The bridge already owns the two pump tasks; adding a pair
of `AtomicU64` counters and bumping them inside the existing
read/recv arms is one line per direction. Returning
`BridgeBytes { up, down }` from `initiate_bridge` rather than
storing into a caller-supplied counter keeps the bridge module
self-contained — the gateway threads the values into the audit row
itself, the bridge has no idea that an `audit` table exists.

The `tunnel.session` row is inserted *before* the bridge runs so the
admin UI's "live tunnels" view sees in-flight sessions (no `ts_close`
yet). On bridge error the row is still finalised with zeros and
`outcome="error"` — the metrics tell the operator the session
existed and failed; the audit log tells them when and against which
tunnel.

**HTTP host-router uses an inline bridge, not `initiate_bridge`,
because it has to forward the peeked preamble first.** The accepted
TCP socket is partially read (just past the Host: header) before the
route is even known, so calling `initiate_bridge` directly would have
either (a) handed `initiate_bridge` a `TcpStream` and a `Vec<u8>`
prefix it didn't know what to do with, or (b) duplicated the prefix
forwarding inside `hackline-core` for a single caller. Both worse
than what we do — `http_router::bridge_with_prefix` already had the
shape; it grew a pair of atomics for the byte counters and now
returns `(u64, u64)` for the same audit + metrics use. The peeked
preamble counts as `bytes_up` so the audit row reflects what the
device actually saw.

**`tunnels::TunnelWithZid` grew `device_id`.** The listener needs
both `tunnel_id` and `device_id` to stamp the `tunnel.session` row.
Looking the device id up at accept time would mean a SQLite hop on
every accepted socket; the row already lives next to `zid` in the
join, so threading it through is free. Three call sites updated:
the manager spawn helper, the `POST /v1/tunnels` hot-add path, and
the active-tcp listing query.

**Metrics: a hand-rolled registry, not the `prometheus` crate.**
SCOPE.md §10.2 lists ten metric families, every one a counter or a
gauge with a small fixed label-set. The `prometheus` crate adds a
non-trivial dependency for what is "render a string from a
`BTreeMap`". One `RwLock<Inner>` around a struct of `BTreeMap`s, plus
two `AtomicI64`s for the two values updated outside the lock
(`devices_online`, `audit_rows`), is sufficient. The HELP + TYPE
preamble for every metric is always emitted regardless of whether
any labelled samples exist yet, so a fresh gateway returns a
metrics body that `grep` can verify against §10.2 directly — exactly
what the demo curl-then-grep above does.

`audit_rows` and `cmd_outbox_depth{device}` are not in-process
counters — they're derived from SQLite. The `/metrics` handler
issues one `SELECT COUNT(*) FROM audit` and one
`GROUP BY device_id` over the un-acked rows immediately before
rendering, so the snapshot is internally consistent. The other
eight families are updated where the work happens:
`msg_fanin::handle_sample` for events / logs, `cmd_delivery` for
`delivered` + ack outcomes, the cmd REST handlers for `accepted` /
`cancelled`, the api-call handler for `api_calls_total`, the
tunnel listeners for sessions + bytes + active.

**Events topic cardinality.** SCOPE.md §10.2 mandates that
high-cardinality event topics are folded into an `_other` bucket
behind a configurable allowlist. `Metrics::set_event_topic_allowlist`
is plumbed but unused in v0.1 because the gateway config doesn't
have a Prometheus block yet; without an allowlist set, every topic
gets its own bucket. The check is one branch so the wiring is
ready when `config::PrometheusConfig` lands — no schema change
needed in `Metrics` itself.

**Admin bundle is plain HTML + vanilla JS, no build step.** Goal 6
says "smallest viable static bundle; do not introduce a new
framework." Three files (`index.html`, `admin.css`, `admin.js`)
embedded via `include_str!` ship with the binary; no `static/`
runtime dependency, no `tower-http::services::ServeDir` (which would
require the operator to also distribute the `static/` directory).
The bundle calls the existing REST surface plus `EventSource` for
SSE; the only new wire is `GET /metrics`, which exists for
Prometheus and not for the UI alone.

The bundle is intentionally **not** branded "React" despite the
SCOPE.md wording of Phase 3 — the rule from the job spec was the
smallest viable bundle, and the smallest viable bundle is plain
JS. The UI will be replaced wholesale by the four-shell React
build in `codeless/ui/codeless-ui/` once Phase 5 ships, so a build
chain locally would have been throw-away work.

**SSE token transport via `?token=…`.** Browser `EventSource` cannot
attach an `Authorization` header. The admin UI puts the bearer on
the query string for the streaming endpoints. The existing SSE
handlers already accept this (or pick it up via the proxy layer);
this is the standard pattern for SSE-with-auth and is documented in
the admin.js comment block. Operators behind an aggressive
reverse-proxy that strips query strings should terminate auth at the
proxy and inject a cookie — not the gateway's problem.

**`/metrics` admin-token gating uses `AuthedUser`, not a separate
admin role check.** SCOPE.md §10.2 says "admin-token gated in v0.1"
but the gateway doesn't yet have a per-route role matrix; the
existing `AuthedUser` extractor admits any valid bearer, which in
v0.1 (single-operator, claim-only) means "the operator's owner
token, or any token the operator has scoped." A finer-grained gate
lands when the `customer` / `viewer` enforcement matrix in SCOPE.md
§6.2 grows beyond device scope.
