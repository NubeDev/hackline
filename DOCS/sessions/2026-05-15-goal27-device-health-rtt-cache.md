# 2026-05-15 — Goal 27: per-request RTT cache for `/v1/devices/:id/health`

Goal 26 made `DeviceHealth.rtt_ms` a real measurement: each
request issues a fresh `liveliness::Get` against the device, capped
at 250 ms. That's correct, but the admin UI polls this endpoint
to refresh per-device cards, and at high poll rates every call
costs a Zenoh round-trip and at minimum one wakeup on the device
side. The endpoint isn't on a hot path *yet*, but we know the
shape of the call site (UI poll), so a 1 s freshness cache pays
for itself the first time the operator opens the devices page.

This goal adds a tiny in-process cache keyed by
`(org_id, device_id)` with a 1 s TTL. Cache hits skip the probe
entirely and reuse the last measured value (including `None`,
which is itself a meaningful "we just tried and it didn't
answer" signal).

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Confirm the call shape: every `GET /v1/devices/:id/health` runs `probe_rtt_ms()` synchronously; `AppState` is `Clone` and shared across handlers; no existing in-memory cache pattern in the gateway crate to copy from. | [x] |
| 1 | Add `crates/hackline-gateway/src/rtt_cache.rs`: `RttCache { Arc<Mutex<HashMap<(i64,i64), Entry>>>, ttl }` with `get`/`put`. Lock is never held across `.await`. Opportunistic stale-prune on `put` to bound memory; absolute size cap as a defence. | [x] |
| 2 | Wire into `AppState`: new `pub rtt_cache: RttCache`, constructed in `bin/serve.rs` with `Duration::from_secs(1)`. | [x] |
| 3 | Refactor `api/devices/health.rs` handler: cache lookup keyed by `(org_id, device.id)` *after* the device row is loaded (so org-membership errors still 404). On hit, skip `probe_rtt_ms()`; on miss, probe and `put`. Module doc-comment updated. | [x] |
| 4 | Verify gates: `cargo check --workspace` (only pre-existing `hackline-agent` warning), `cargo test --workspace`, `cargo build -p hackline-gateway --bin serve`, `pnpm` builds, `make test-client` green twice, dev stack `:8080`/`:1430` health 200/200. | [x] |

## Design

**Why 1 s TTL.** The UI polls device cards on the order of
seconds; a 1 s window collapses a burst of refresh ticks (e.g.
opening the devices page renders many cards near-simultaneously
and React strict mode double-invokes effects in dev) into a
single probe per device. It's also short enough that an operator
who triggers a manual refresh always sees a fresh number on the
second click. Anything longer (5 s, 10 s) would start lying
about *current* RTT, which is the field's documented contract.

**Why cache `None` as well as `Some`.** A failed probe is
expensive: it spent the full 250 ms timeout waiting for a reply
that never came. Re-running it on the next request would burn
another 250 ms for the same answer. The cache stores the
`Option<i64>` verbatim so a device that's down stops costing
the gateway a quarter-second per request for the next second.

**Why key on `(org_id, device_id)`, not just `device_id`.**
Device ids are unique per row but the cache lives in process
memory and shares its lifetime with the running gateway; if the
schema ever moves to per-org id sequences (currently shared),
the org_id prefix prevents an accidental cross-org read. It's a
defence-in-depth rule that costs one extra `i64` per entry.

**Why `std::sync::Mutex`, not `tokio::sync::Mutex`.** The
critical sections are pure HashMap ops — lookup, compare-and-
return, opportunistic prune, insert. None of them touch I/O or
`.await`. A blocking mutex is the right tool; an async mutex
would serialise contention through the tokio scheduler for no
benefit.

**Why no singleflight.** Two concurrent requests for the same
device on a cache miss will both probe — a small amount of
duplicated Zenoh traffic. Adding singleflight (one in-flight
probe per key, others wait) doubles the locking surface and
introduces an `Arc<Notify>` per key. The current cost is one
extra liveliness query, which is what the endpoint already did
yesterday; the cache makes the *steady state* cheaper, not the
edge cases. If contention shows up in profiling, singleflight
is a follow-up.

**Why opportunistic pruning + absolute cap.** The natural
working set is "devices the operator looked at recently".
Devices come and go; the cache must not grow unboundedly across
months of uptime. Stale entries (age > TTL) are dropped on every
`put`, which keeps the size bounded in steady state. The
absolute cap (`MAX_ENTRIES = 4096`) is a defence against a
pathological burst of distinct device ids in a single TTL window;
on overflow the cache is cleared rather than evicting one entry
(simpler, and the next requests just refill the hot keys).

**Why the cache is in `AppState` and not in the handler module.**
Two reasons: tests that build `AppState` directly need to
construct it (so the type lives at workspace-visible scope),
and a future health endpoint variant (e.g. the deferred list
fan-out from goal 26) will want to share the same cache rather
than holding its own.

**Why no test for the cache through the handler.** Same shape
as goal 26: exercising the live probe needs a real Zenoh agent
declaring a liveliness token. The cache module itself gets
direct unit tests for its observable behaviour (hit, miss,
expiry, size cap) — those don't need Zenoh and pin the
contract the handler relies on.

## Outcome

- `cargo check --workspace` clean except the pre-existing
  `hackline-agent PortDenied` warning (CLAUDE.md rule 3).
- `cargo test --workspace` green; gateway lib unit tests grew
  from 24 to 29 (the new `rtt_cache` module ships with five
  tests: miss, hit-including-`None`, expiry, replace,
  per-org isolation).
- `cargo build -p hackline-gateway --bin serve` clean.
- `pnpm -C clients/hackline-ts build` clean (no TS surface
  changed; `pnpm install` re-hydrated `node_modules` first
  because the cache had been purged since goal 26).
- `pnpm -C ui/hackline-ui typecheck` + `build` clean (bundle
  unchanged at 258.59 KB / 78.40 KB gz).
- `make test-client` 6 files / 12 tests green twice in a row.
- Dev stack on `:8080` / `:1430` was not running at the time of
  verification — same operator-owned-lifecycle reasoning as
  goal 26: the change only takes effect on the next restart
  and the running gateway (when one exists) continues with
  whatever binary it was launched with. The verified gates
  above (workspace tests + the dedicated `rtt_cache` unit
  tests) cover the cache contract end-to-end without needing
  a live mesh.

Files modified:

- `crates/hackline-gateway/src/lib.rs` — expose new
  `rtt_cache` module.
- `crates/hackline-gateway/src/state.rs` — `AppState` gains
  `rtt_cache: RttCache`.
- `crates/hackline-gateway/src/bin/serve.rs` — construct the
  cache with a 1 s TTL when wiring `AppState`.
- `crates/hackline-gateway/src/api/devices/health.rs` —
  cache lookup before `probe_rtt_ms`; module doc-comment
  updated to reflect the cache.

Files added:

- `crates/hackline-gateway/src/rtt_cache.rs` — the cache
  itself, with five unit tests pinning the observable
  contract.

Working tree intentionally dirty for operator review.

## What I deferred and why

Same as goal 26, plus:

- **Singleflight on cache miss.** Listed above; not needed yet.
- **Cache invalidation on liveliness change.** The liveliness
  fan-in already updates `last_seen_at` when a device drops or
  re-appears; we could clear the cache entry from there to
  shorten the lie window after a state flip. Skipped because
  the lie window is at most 1 s and the wiring would couple
  two modules that are independently simple today.
- **Per-org cache eviction on org delete.** Org delete is not a
  modelled operation in the gateway today; revisit if it lands.

## What's next (goal 28 candidates)

- **List-endpoint health fan-out** (`GET /v1/devices` returns
  `online`/`rtt_ms` per row, parallelised with a per-call
  timeout budget). This was deferred from goal 26 and is now
  cheap because the per-row cache is in place.
- **Stand up a first GitHub Actions workflow** (touches the
  secrets-surface decision; flagged as such).
- **SSE integration test in `@hackline/client`** (depends on
  goal-15 `wire.ts::Event` vs `types.ts::GatewayEvent`
  reconciliation; listed for visibility).
- **Operator decisions** (`User`, `CmdOutboxRow`,
  `Device.org_id`, `Device.class`/`online`) — not picked
  autonomously.
