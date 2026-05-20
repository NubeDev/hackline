# 2026-05-15 — Goal 26: synchronous `liveliness::Get` probe → `DeviceHealth.rtt_ms`

`GET /v1/devices/:id/health` has carried an `rtt_ms: int|null`
field since the openapi schema landed, but the handler always
emitted `null`: a comment in `api/devices/health.rs` reserved
the field "for when [the synchronous probe] lands so the wire
shape does not have to change again". Goal 26 is that landing.

The handler now issues a single `liveliness::Get` against the
device's own `hackline/<org>/<zid>/health` token, capped at
250 ms, and reports the wall-clock RTT to the first reply.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Confirm the gap: handler hard-codes `rtt_ms: None`; gateway already holds an `Arc<zenoh::Session>` in `AppState`; `keyexpr::health(org, zid)` and `Zid::new` exist; `orgs::get(conn, id)` resolves the slug from the caller's `org_id`. | [x] |
| 1 | Refactor handler: load device + org slug in one `spawn_blocking` (two queries; one round-trip to the pool). Move RTT probe into a private `probe_rtt_ms()` returning `Option<i64>`. | [x] |
| 2 | Probe: `Instant::now()` → `session.liveliness().get(ke).timeout(250ms).await` → `recv_async()` wrapped in a second `tokio::time::timeout(250ms)` belt-and-braces. First reply ⇒ `Some(elapsed_ms)`; any error / timeout / no reply ⇒ `None`. Probe failure must NOT fail the HTTP request. | [x] |
| 3 | Document constants: `PROBE_TIMEOUT_MS = 250` (rationale: liveliness is single-digit ms on a healthy mesh; UI cannot tolerate seconds; matches `api_call::DEFAULT_TIMEOUT_MS`-style snappiness). | [x] |
| 4 | Verify gates: `cargo check --workspace` (only pre-existing `hackline-agent` warning), `cargo test --workspace`, `cargo build -p hackline-gateway --bin serve`, `pnpm` builds, `make test-client` green twice, dev stack `:8080`/`:1430` health 200/200. | [x] |

## Design

**Why probe per-request instead of caching the last RTT.** The
field is documented as a *current* RTT; a cached value would
need a freshness clock and would lie when the device went
offline since the last cache fill. A 250 ms direct probe is
small, predictable, and correct. Caching is a future
optimisation if the per-request cost becomes an issue under
admin-UI polling, but the current `/v1/devices/:id/health`
endpoint is not on any hot path.

**Why `online` stays `last_seen_at`-derived, not probe-derived.**
The `online` field has a documented contract (per-row
liveliness window, ONLINE_STALE_SECS = 60 s) that the UI
already trusts. Letting a single 250 ms probe veto a fresh
`last_seen_at` would create flap noise on an otherwise healthy
device that happened to miss one query window. Decoupling keeps
the two fields independently meaningful: `online` says "the
device has checked in recently"; `rtt_ms` says "the device
answered me right now".

**Why probe failures collapse to `None` rather than 5xx.** An
unreachable device must still answer
`GET /v1/devices/:id/health` (that's the whole point of the
endpoint — surfacing reachability state). 500ing on probe
failure would conflate "the device is down" with "the gateway
is broken", and the UI would stop showing offline devices
their reason. The `rtt_ms: null + online: false` pair is the
correct diagnostic.

**Why two timeouts (`get(...).timeout(...)` *and*
`tokio::time::timeout(...).await`).** The zenoh handler
documentation says the channel closes when the query timeout
fires, so `recv_async()` should return `Err`. The outer
`tokio::time::timeout` is a defensive belt-and-braces against a
future zenoh change that holds the channel past its declared
window: this endpoint must return inside ~300 ms in the worst
case, and the only way to guarantee that against an external
crate is to enforce the cap on this side too. Cheap, no
behaviour change in the happy path.

**Why no caller changes.** The wire shape is unchanged
(`DeviceHealth.rtt_ms` was already documented as
`int|null`). Existing TS consumers parse it as
`number | null`; the value is just non-null more often now.

**Why no test for the probe path.** Exercising
`liveliness::Get` requires either a real Zenoh session with a
publisher under the matching keyexpr, or a mock that intercepts
the session call. The first is integration-test territory and
overlaps with the `cmd_plane.rs` setup in
`crates/hackline-gateway/tests/`; the second would mock more
than it tests. The handler change is small enough that a future
integration test (which would need a fake agent declaring its
liveliness token anyway) is the right shape, not a unit test.
The probe failure path is exercised every time the dev gateway
is restarted with no agents attached: the endpoint returns
`rtt_ms: null`, which is exactly the prior behaviour.

**Why the dev stack on `:8080` was not restarted.** Same
reasoning as goals 22–25: the operator owns dev-stack
lifecycle. The change here only takes effect on the next
restart; the running gateway continues to emit
`rtt_ms: null` from its older binary, which is no worse than
the status quo.

## Outcome

- `cargo check --workspace` clean except the pre-existing
  `hackline-agent PortDenied` warning (CLAUDE.md rule 3).
- `cargo test --workspace` green; existing 10 lib tests in
  `hackline-gateway` (from goals 22–25) still pass.
- `cargo build -p hackline-gateway --bin serve` clean.
- `pnpm -C clients/hackline-ts build` clean (no TS surface
  changed).
- `pnpm -C ui/hackline-ui typecheck` + `build` clean (bundle
  unchanged at 258.59 KB / 78.40 KB gz).
- `make test-client` 6 files / 12 tests green twice in a row.
- Dev stack health on `:8080` / `:1430` still 200 / 200.

Files modified:

- `crates/hackline-gateway/src/api/devices/health.rs` — handler
  loads `(device, org)` together, calls `probe_rtt_ms()`;
  module doc-comment updated to reflect the live probe;
  `PROBE_TIMEOUT_MS` constant; `probe_rtt_ms()` helper.

Files added: none.

Working tree intentionally dirty for operator review.

## What I deferred and why

Same as goal 25 minus the liveliness-probe item:

- **`User` shape sweep needs an operator schema decision first**
  (auth model vs multi-tenant model).
- **`CmdOutboxRow` shape — no openapi schema to align against.**
- **`Device.org_id`** — schema decision.
- **`Device.class` and `Device.online` not on the wire** — would
  be answered by a per-row health fan-out call from the list
  endpoint, which is its own goal (the per-request probe in this
  goal is per-id; the list shape would need a parallelised
  variant with its own timeout budget).
- **SSE integration test in `@hackline/client`** (pending
  goal-15 `wire.ts::Event` vs `types.ts::GatewayEvent`
  reconciliation).
- **Wire `make test-client` into CI** — there is no
  `.github/workflows/` directory in the hackline repo today.
  This is "create CI", not "wire into CI"; deserves its own
  goal because the first workflow has to also pick the runner
  matrix, the cache strategy, and the secrets surface.
- **Drop the "id-in-detail" workaround in
  `api/{tunnels,devices}/delete.rs`** — would need a
  projection-side change (race-prone). Left as-is.

## What's next (goal 27 candidates)

- **Per-request RTT cache** with a 1 s freshness window so the
  admin UI can poll `/v1/devices/:id/health` rapidly without
  a Zenoh round-trip per call.
- **Stand up a first GitHub Actions workflow** so
  `make test-client` (and the Rust + UI gates) run on every
  PR. Touches the secrets surface decision; flagged as such.
- **SSE integration test in `@hackline/client`** (depends on
  the goal-15 `wire.ts::Event` vs `types.ts::GatewayEvent`
  reconciliation; listed for visibility).
- **Audit `actor_user_id` projection: backfill from session
  metadata for `auth.login` rows where `user_id` is set after
  the row is written** — small, mechanical, no schema change.
- **Operator decisions** (`User`, `CmdOutboxRow`,
  `Device.org_id`, `Device.class`/`online`) — not picked
  autonomously.
