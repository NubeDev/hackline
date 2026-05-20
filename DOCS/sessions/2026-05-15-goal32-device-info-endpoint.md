# 2026-05-15 — Goal 32: implement `GET /v1/devices/{id}/info`

The openapi spec has documented this endpoint since the
multi-tenant phase, the TS client has called it on the device
detail page since the UI landed, and the agent has had a
`hackline/<org>/<zid>/info` keyexpr reserved in
`hackline-proto::keyexpr` since goal 4. None of it is wired
up. The agent's `info` module is a one-line doc-comment stub;
the gateway's `info` module is a one-line doc-comment stub;
the gateway router doesn't register the route.

The TS `AgentInfo` shape (`{zid, version, allowed_ports,
uptime_s}`) is what `DeviceDetailPage` renders. The proto
shape (`{label, allowed_ports}`) and openapi shape
(`{label, allowed_ports}`) are different from each other
*and* from what the UI consumes. This goal collapses all
three to the TS shape — which is the one that has a real
consumer — and ships the agent + gateway implementation.

`label` is dropped from the wire: every caller already has
the device's row label out of `GET /v1/devices`, so the agent
echoing it back on a separate keyexpr is redundant noise.
`zid` and `version` are the agent's identity (it's the only
authority for the version it's running); `allowed_ports` is
the agent's policy decision (already in proto today);
`uptime_s` is a runtime fact only the agent can know.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Confirm: agent `info.rs` is a stub; gateway `info.rs` is a stub; route not registered; `keyexpr::info(org, zid)` exists; proto `AgentInfo` is `{Option<String> label, Vec<u16> allowed_ports}`; TS `AgentInfo` is `{zid, version, allowed_ports, uptime_s}`; openapi `AgentInfo` is `{label, allowed_ports}`. | [x] |
| 1 | Reconcile proto `AgentInfo` to `{zid, version, allowed_ports, uptime_s}`. Update its module doc-comment. | [x] |
| 2 | Agent: implement `info::serve(session, org, zid, allowed_ports, started_at)` — declare queryable on `keyexpr::info(org, zid)`, on each query reply with serialised `AgentInfo` (`version = env!("CARGO_PKG_VERSION")`, `uptime_s` derived from `started_at.elapsed().as_secs()`). Spawn from `main.rs` after liveliness, before `connect::serve_connect` (which blocks). Pass an `Instant` captured at process start. | [x] |
| 3 | Gateway: implement `api/devices/info::handler` — load `(device, org_slug)`, query `keyexpr::info`, parse first reply as `AgentInfo`, return JSON. 504 on timeout, 503 on no replies (mirrors `api_call::call.rs`). Register `/v1/devices/{id}/info` in `api/router.rs` (before `/{id}/health`-style literal routes if any conflict — there's none today). | [x] |
| 4 | openapi: rewrite `AgentInfo` schema to `{zid, version, allowed_ports, uptime_s}` with the right types. | [x] |
| 5 | Verify gates: `cargo check --workspace` (only pre-existing `hackline-agent` warning), `cargo test --workspace`, `cargo build -p hackline-gateway --bin serve` + `-p hackline-agent`, `pnpm -C clients/hackline-ts build`, `pnpm -C ui/hackline-ui typecheck`+`build`, `make test-client` green twice. | [x] |

## Design

**Why drop `label` from the wire.** The device row already
carries `label` (it's the operator-assigned name; the agent
doesn't get to define it). Letting the agent report a `label`
on `info` creates two sources of truth that can disagree —
either silently lie, or force the UI to choose. Removing it
removes the question.

**Why `zid` *is* on the wire even though the URL implies it.**
The caller asks the gateway by numeric `id`; the gateway
resolves to the `zid` it knows about; the agent reports the
`zid` it actually loaded from its config. If those disagree
(stale device row, mis-wired agent, claim collision), the
mismatch is exactly what the operator needs to see. Cheap,
non-redundant.

**Why `version = env!("CARGO_PKG_VERSION")`.** The Cargo
version is the only string that's guaranteed to change with
every release and that survives binary stripping. A custom
build-time variable would be more flexible (commit SHA), but
the value buyers care about most is "what release is this
agent on", which is exactly what `CARGO_PKG_VERSION` says.
Adding commit SHA later is non-breaking (extra optional
field).

**Why `uptime_s: u64` from a captured `Instant`, not a
`SystemTime` delta.** `Instant` is monotonic — clock skew
on the device doesn't make uptime go backwards. `u64` seconds
is enough range for any plausible deployment (years).

**Why query timeout in the gateway is 1 s.** The openapi
references `info_query_timeout` but no code reads any such
config today. 1 s is a reasonable first cut: the agent
generates the reply synchronously from in-memory state
(zero I/O), so a healthy mesh resolves in single-digit ms;
1 s is enough slack for a slow-mesh path while keeping the
HTTP request snappy. Configurable later if it proves wrong.
Mirrors goal 26's reasoning for the 250 ms health probe but
allows more room because `info` includes a `version` string
the operator may want even from a slow agent.

**Why 504 (timeout) vs 503 (unreachable).** Same split as
`api_call::call.rs`: the zenoh `recv_async` call distinguishes
"channel closed because the upstream timeout fired" (the
device is up but didn't reply in time → 504) from "channel
closed with no replies in scope" (no agent listens on the
keyexpr → 503). Operators triaging from logs need that split.

**Why no test for the live path.** Same shape as goals
26/28: testing requires a real Zenoh session with an agent
that declares the `info` queryable. The wire shape is
pinned by the proto's serde derive + the handler's
deserialise — any drift breaks the deserialise. The
`getDeviceInfo` TS client method is already present; if the
shape ever drifts, the existing call site renders garbage,
which is recoverable noise rather than a silent corruption.

**Why register the route in alphabetical order with
existing `/v1/devices/{id}/...`.** No precedence concern
(it's a literal segment after the capture); placing it next
to `/health` keeps the related endpoints visually grouped in
the router file.

**Why this is one tick despite spanning four crates.** The
patterns are all present: agent queryable shape =
`connect.rs`, gateway probe shape = `api_call/call.rs` (or
the simpler `health_probe.rs`), proto serde derive is
already on `AgentInfo`. The work is 90% rewiring + 10% new
code. Sized **M**.

## Outcome

- `cargo check --workspace`: clean (only the pre-existing
  `hackline-agent::error::AgentError::PortDenied` dead-code
  warning).
- `cargo test --workspace`: all suites green; gateway lib still
  33 tests, proto suite still 7, no regressions.
- `cargo build -p hackline-gateway --bin serve`: clean.
- `cargo build -p hackline-agent --bin hackline-agent`: clean
  (same `PortDenied` warning).
- `pnpm -C clients/hackline-ts build`: clean.
- `pnpm -C ui/hackline-ui typecheck` + `build`: clean (bundle
  259.31 KB / 78.57 KB gz — unchanged).
- `make test-client`: 6 files / 13 tests green twice in a row.

A second deferred discovery surfaced during the session-doc
write-up: the gateway `info` module's original doc-comment
claimed the path was `/v1/devices/:id/health`. Fixed in the
rewrite (the new module doc-comment names the endpoint
correctly).

## What I deferred and why

- **Configurable `info_query_timeout`.** 1 s is sufficient
  for now; making it operator-tunable adds a config knob
  with no current need. If the field name is referenced
  elsewhere in docs, those references stay accurate; only
  the implementation defers to a hard-coded constant.
- **Commit SHA in `version`.** Build-time embedding is a
  separate concern and additive; can land later.
- **Reverse-proxy / agent-versions list.** Useful for "show
  me every agent on the fabric and its version" — needs an
  events fan-in or a wildcard-query handler, not an
  extension of this endpoint.
- **TS client surface.** `getDeviceInfo(id)` already exists
  and matches the new shape — no client change needed.

## What's next (goal 33 candidates)

- **Stand up first GitHub Actions workflow** for
  `make test-client` + Rust + UI gates.
- **SSE integration test in `@hackline/client`** (depends
  on goal-15 reconciliation).
- **Drop the `device.class === "linux"` gate in
  `DeviceDetailPage`** — the `info` endpoint now works for
  any agent that happens to listen, regardless of `class`
  (which is still not on the wire).
- **Operator decisions** (`User`, `CmdOutboxRow`,
  `Device.class`/`online`).
