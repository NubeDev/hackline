# 2026-05-15 — Goal 28: list-endpoint health fan-out (`GET /v1/devices/health`)

Goal 26 made the per-device `rtt_ms` real; goal 27 cached it. The
admin UI's devices page renders many cards at once, and the
natural ask is "show the online dot and the RTT badge per row
without the UI having to fan out N requests itself". Doing the
fan-out client-side would also defeat the cache (every card
would issue its own probe before the cache is warm).

This goal adds `GET /v1/devices/health` — a single call that
returns one health entry per device in the caller's org. The
fan-out runs inside the gateway with `futures::join_all`; every
probe goes through the same `RttCache` from goal 27, so a steady
poll loop hits the cache on every device after the first window.

The wire shape of `GET /v1/devices` (item-level `Device`) is
unchanged; the new endpoint sits alongside as a parallel call,
which keeps the existing TS client and openapi `Device` schema
intact and avoids the "online/rtt_ms appear sometimes" quantum
that an opt-in `?health=1` flag would produce.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Confirm the shape: `Device` in openapi has no `online`/`rtt_ms`; the existing per-device handler in `api/devices/health.rs` already does cached probe + `last_seen_at`-derived `online`; `futures` is already a workspace dep; the router registers `/v1/devices/{id}` so the new literal segment must be added before it (or use a path that doesn't shadow). | [x] |
| 1 | Extract the probe + freshness logic out of `api/devices/health.rs` into `api/devices/health_probe.rs` (`pub(super)` helpers) so both the per-id handler and the new list handler share one implementation. | [x] |
| 2 | Add `api/devices/list_health.rs` with `GET /v1/devices/health` returning `{ items: DeviceHealthEntry[] }` (`{ device_id, online, last_seen_at, rtt_ms }`). One blocking task loads all devices + the org slug; one `join_all` over per-device probe futures (each capped at 250 ms by the existing probe code, each cache-aware). | [x] |
| 3 | Wire the new route. Register `/v1/devices/health` *before* `/v1/devices/{id}` in `api/router.rs` so axum's matchit picks the literal segment first. | [x] |
| 4 | Add `DeviceHealthEntry` + `DeviceHealthList` schemas to `DOCS/openapi.yaml`; document the endpoint under `/v1/devices/health` (collection-level health, parallel probe with 250 ms per-row budget, cached). | [x] |
| 5 | Verify gates: `cargo check --workspace` (only pre-existing `hackline-agent` warning), `cargo test --workspace`, `cargo build -p hackline-gateway --bin serve`, `pnpm` builds, `make test-client` green twice, dev stack `:8080`/`:1430` skipped (operator-owned). | [x] |

## Design

**Why a new endpoint instead of extending `Device`.** Adding
`online` + `rtt_ms` to `Device` would either (a) make the list
endpoint synchronously probe every device on every call —
slow, defeats the cache's per-device key, and breaks the rule
that mutating-state operations and pure reads stay separate;
or (b) add nullable fields whose null state would conflate
"never probed" with "probed and got null", losing the
diagnostic value goal 26 set up. A separate endpoint also
lets the UI choose its refresh cadence (e.g. devices list once
on mount, health every 2 s) without coupling the two.

**Why `{ items: [...] }` and not a bare array.** The codebase
already uses `{ items, next_cursor }` for paginated endpoints
(audit, events, cmd outbox) and that wrapper is documented as
"future-proof for cursor pagination". Health responses are
naturally bounded (one per device in the org, and devices are
already a small set), so `next_cursor` is omitted entirely
rather than always-null — an explicit "this endpoint is not
paginated" signal that matches how other non-paginated
collection endpoints (`GET /v1/devices`, `GET /v1/tunnels`)
return bare arrays. The wrapper here exists because each
*entry* needs an explicit `device_id` field, so the response
must be an object keyed list and the wrapper makes that
explicit. Bare-array would mean the caller has to keep
`items[i].device_id` as the only join key — fine, but the
wrapper costs nothing and matches the page-envelope shape
clients already know.

**Why `device_id` in the entry, not as a map key.** JSON
object keys are stringly-typed; clients would have to parse
them back to `i64` for any join with the device list. Keeping
the entry shape `{ device_id, online, last_seen_at, rtt_ms }`
preserves the int type and matches the per-device endpoint's
shape (which is the single-row equivalent of one of these
entries, sans `device_id`).

**Why one blocking task for the device list, then async fan-
out.** The DB read happens once; the probes are network
I/O. Running them in parallel inside the gateway costs ~one
liveliness query per device (most of which hit the cache), so
the wall-clock budget is `max(per-probe budget) ≈ 250 ms`
regardless of N. Doing them serially would be `N × 250 ms`
worst case — a 10-device org would block 2.5 s on a fully
cold cache. `futures::join_all` is the right primitive: no
ordering requirement, no early-exit on error (a single
failed probe yields `rtt_ms: null` and the others continue).

**Why no per-call timeout budget on top of the 250 ms per-
probe cap.** Each probe is already individually capped, and
they run in parallel; the total wall clock cannot exceed the
per-probe cap by much regardless of org size. Adding an outer
timeout would either mask probes that did complete (lying to
the caller) or be redundant (the probes already hit their
cap). KISS.

**Why `pub(super)` extraction instead of leaving the helper
in `health.rs`.** Two callers now need it; cross-module
access from a sibling needs at least `pub(super)`. The
helper is small enough (compute `online`, do the cached
probe) that giving it its own file matches the
single-responsibility-per-file convention from CLAUDE.md.

**Why no test for the live fan-out.** Same shape as
goals 26/27: testing requires a real Zenoh session with N
agents declaring liveliness tokens. The cache contract was
pinned by unit tests in goal 27; the per-device probe contract
was pinned by goal 26 (failure ⇒ `None`, not 5xx); the
fan-out is `join_all` over the per-device call, which is
mechanical composition. Unit-testing `join_all` isn't
testing our code.

**Why register `/v1/devices/health` before `/v1/devices/{id}`.**
axum 0.8's matchit router prefers literal segments over
captures, but the route registration order is what
disambiguates when two routes could match. Putting the
literal first is defensive: even if a future axum version
changed precedence, the literal would still win.

## Outcome

- `cargo check --workspace` clean except the pre-existing
  `hackline-agent PortDenied` warning (CLAUDE.md rule 3).
- `cargo test --workspace` green; gateway lib unit tests grew
  29 → 33 (the new `health_probe` module ships with four
  tests pinning `online_from_last_seen` against the
  `ONLINE_STALE_SECS` window, `None`, and clock-skew edge
  cases). The `rtt_cache` tests from goal 27 still pass.
- `cargo build -p hackline-gateway --bin serve` clean.
- `pnpm -C clients/hackline-ts build` clean (no TS surface
  changed; the new endpoint deliberately has no client method
  yet — listed as goal-29 candidate).
- `pnpm -C ui/hackline-ui typecheck` + `build` clean (bundle
  unchanged at 258.59 KB / 78.40 KB gz).
- `make test-client` 6 files / 12 tests green twice in a row.
- Dev stack health skipped — same operator-owned-lifecycle
  reasoning as goal 27.

Files modified:

- `crates/hackline-gateway/src/api/devices/mod.rs` — add
  `health_probe` (private) and `list_health` (public) modules.
- `crates/hackline-gateway/src/api/devices/health.rs` —
  refactored to call the shared `health_probe` helpers; module
  doc-comment updated.
- `crates/hackline-gateway/src/api/router.rs` — register
  `/v1/devices/health` before the `/{id}` capture.
- `DOCS/openapi.yaml` — new path + `DeviceHealthEntry` and
  `DeviceHealthList` schemas.

Files added:

- `crates/hackline-gateway/src/api/devices/health_probe.rs`
  — shared `online`, clock, and cached-probe helpers, with
  four unit tests pinning the `online` window contract.
- `crates/hackline-gateway/src/api/devices/list_health.rs`
  — the new fan-out handler.

Working tree intentionally dirty for operator review.

## What I deferred and why

- **Cursor pagination on the new endpoint.** Health responses
  are bounded by org device count today; if an org grows past
  a few hundred devices the per-call probe budget itself
  becomes the natural ceiling, not the response size.
- **Per-call timeout budget.** Covered above.
- **Singleflight on cache miss for fan-out.** A list call on a
  cold cache can issue N parallel probes, but each is
  per-device — there's no overlap to deduplicate within one
  call. Cross-call dedup would be the goal-27 deferred
  singleflight, not new work here.

## What's next (goal 29 candidates)

- **Stand up a first GitHub Actions workflow** for
  `make test-client` + Rust + UI gates (touches the secrets-
  surface decision; flagged as such).
- **SSE integration test in `@hackline/client`** (depends on
  the goal-15 `wire.ts::Event` vs `types.ts::GatewayEvent`
  reconciliation; listed for visibility).
- **TS client method for the new endpoint.**
  `getDevicesHealth()` against `/v1/devices/health`. Small;
  could land standalone or bundled with a UI consumer.
- **Operator decisions** (`User`, `CmdOutboxRow`,
  `Device.org_id`, `Device.class`/`online`) — not picked
  autonomously.
