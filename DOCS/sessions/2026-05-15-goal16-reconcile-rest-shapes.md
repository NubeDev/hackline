# 2026-05-15 — Goal 16: reconcile the goal-15 REST shape mismatches

Goal 15's test file comments documented three places where the
`@hackline/client` types lied about (or skipped) the real REST shape.
This goal ships the implementations and flips those tests from
"lock in current behaviour" to real assertions:

1. `health()` typed return (`{ ok: true }` → `{ status: "ok" }`,
   matching `DOCS/openapi.yaml` and `DOCS/REST-API.md`).
2. `GET /v1/devices/:id/health` route landing — handler implemented
   to the openapi `DeviceHealth` shape, mounted, TS type aligned.
3. `device.create` / `device.delete` / `tunnel.create` /
   `tunnel.delete` audit rows — the well-known actions documented in
   `SCOPE.md` §7.2 that had never been written.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | `crates/hackline-gateway/src/api/devices/health.rs`: real handler returning `{online, last_seen_at, rtt_ms}`; `online` derived from `last_seen_at` within `ONLINE_STALE_SECS = 60`; `rtt_ms` reserved for the future synchronous probe (see Design) | [x] |
| 1 | Mount `GET /v1/devices/{id}/health` in `api/router.rs` | [x] |
| 2 | `api/devices/create.rs` + `api/devices/delete.rs`: audit `device.create` / `device.delete` per SCOPE.md §7.2 | [x] |
| 3 | `api/tunnels/create.rs` + `api/tunnels/delete.rs`: audit `tunnel.create` / `tunnel.delete` per SCOPE.md §7.2 | [x] |
| 4 | New audit rows put entity ids in `detail` rather than the FK columns (rationale below) | [x] |
| 5 | `clients/hackline-ts/src/client.ts` + `http-client.ts`: `health()` typed as `{ status: "ok" }` | [x] |
| 6 | `clients/hackline-ts/src/types.ts`: `DeviceHealth` aligned to openapi (`last_seen_at: number\|null`, `rtt_ms: number\|null`) | [x] |
| 7 | `ui/hackline-ui/src/lib/utils.ts`: `relTime` accepts `number\|string\|null\|undefined`; numbers treated as unix epoch seconds (canonical wire shape) | [x] |
| 8 | `ui/hackline-ui/src/modules/devices/DeviceDetailPage.tsx`: read canonical `last_seen_at` / `rtt_ms` fields | [x] |
| 9 | Upgrade `clients/hackline-ts/test/health.test.ts`: assert `{ status: "ok" }` directly (no cast) | [x] |
| 10 | Upgrade `clients/hackline-ts/test/devices.test.ts`: assert the openapi `DeviceHealth` shape for an offline device (`{online:false, last_seen_at:null, rtt_ms:null}`) | [x] |
| 11 | Upgrade `clients/hackline-ts/test/audit.test.ts`: assert `device.create`, `tunnel.create`, *and* `cmd.send` audit rows in the same test | [x] |
| 12 | Verify: `cargo check --workspace` (only pre-existing `hackline-agent` warning), `cargo test --workspace` green, `pnpm -C clients/hackline-ts build` clean, `pnpm -C ui/hackline-ui typecheck` + `build` clean, `make test-client` green twice in a row | [x] |

## Design

**Why the new audit rows put entity ids in `detail` instead of the
FK columns.** The audit table's FKs to `devices(id)` and `tunnels(id)`
are plain `REFERENCES` (no `ON DELETE SET NULL`). With
`PRAGMA foreign_keys = ON` (gateway db/pool.rs) any audit row that
points at a still-live id pins that id forever — the next
`DELETE /v1/devices/:id` fails with a FK violation because there's
an audit row referring to it. The pre-existing `cmd.send` audit
already had this latent bug (see existing `api/cmd/send.rs` writing
`Some(device_id)`); the only reason the goal-15 tests didn't notice
is that `deleteDeviceQuiet` swallows errors, leaving stale rows
behind.

For the new audit rows the right answer is:

- `device.create` — pass `None` for `device_id`; carry the new
  device id in `detail.device_id`. The join back is one JSON
  extract instead of one FK lookup, which matters not at all for
  audit-trail use.
- `device.delete` — pass `None` for `device_id` (the device row is
  *gone* by the time the audit insert runs anyway, so a FK would
  dangle); carry id + zid in `detail`.
- `tunnel.create` — same shape as `device.create`.
- `tunnel.delete` — same shape as `device.delete`.

The cleaner long-term fix is a V002 migration that recreates the
audit table with `ON DELETE SET NULL` on the FKs (sqlite needs the
table-recreate dance — `ALTER TABLE` can't change FK actions).
That's a deliberate non-goal here so we don't touch landed
migrations.

**Why `online` flips on a 60 s threshold.** The bridge keepalive
documented in SCOPE.md §6 fires every 30 s; `60` is one full beat
plus a missed-beat of slack. Keeps the value boolean — the operator
who needs the actual recency reads `last_seen_at`.

**Why `rtt_ms` is always `null` today.** The openapi shape reserves
the field for a synchronous Zenoh `liveliness::Get` probe latency.
That probe isn't wired in this handler — the existing
`liveliness.rs` is a *subscriber* that bumps `last_seen_at` from
the device-side declaration, not a query. Implementing the probe
is a worthwhile follow-up but isn't required to make the route
return a stable, documented shape today. The handler comment names
this so the next agent doesn't think `rtt_ms = null` is a TODO.

**Why `relTime` was extended rather than the wire shape changed.**
The canonical timestamps on the REST surface are unix epoch seconds
(`DOCS/openapi.yaml` `Device.last_seen_at`, `DeviceHealth.last_seen_at`).
The TS `Device.last_seen_ts: string|null` field name is wrong on
two counts (`_ts` vs `_at`, `string` vs `number`) — but fixing it
ripples through every consumer. Out of scope here; flagged as a
follow-up. `relTime` accepting both shapes lets the UI line render
the `DeviceHealth.last_seen_at` correctly today without that ripple.

**What I did *not* refactor (deliberate non-goals).** The pre-existing
`cmd.send` audit row still uses `Some(device_id)` and so still
latently blocks device deletes. The `Device.last_seen_ts` typo on
the TS client + UI usage is still wrong on the wire. Both deserve
their own goals.

## Outcome

- `pnpm -C clients/hackline-ts build` clean.
- `pnpm -C ui/hackline-ui typecheck` + `build` clean (bundle still
  ~258 KB / 78 KB gz; no regression).
- `cargo check --workspace` clean except the pre-existing
  `hackline-agent` `PortDenied` dead-code warning (allowed by the
  repo CLAUDE.md rule 3).
- `cargo test --workspace` all green.
- `make test-client` green twice in a row from a cold tempdir.
- Dev stack on `:8080` / `:1430` still 200 / 200.
- `health.test.ts` no longer needs the `as unknown as` escape hatch.
- `devices.test.ts` asserts the openapi `DeviceHealth` shape for an
  offline device.
- `audit.test.ts` asserts `device.create`, `tunnel.create`, and
  `cmd.send` rows in one round-trip — three of the four
  newly-implemented audit actions, plus the pre-existing one.

Files added: none.

Files modified:

- `crates/hackline-gateway/src/api/devices/health.rs` — real
  handler (was a one-line docstring).
- `crates/hackline-gateway/src/api/router.rs` — mount `health` route.
- `crates/hackline-gateway/src/api/devices/create.rs` — audit
  `device.create`.
- `crates/hackline-gateway/src/api/devices/delete.rs` — audit
  `device.delete`.
- `crates/hackline-gateway/src/api/tunnels/create.rs` — audit
  `tunnel.create`.
- `crates/hackline-gateway/src/api/tunnels/delete.rs` — audit
  `tunnel.delete`.
- `clients/hackline-ts/src/client.ts` — `health()` typed return.
- `clients/hackline-ts/src/http-client.ts` — `health()` typed return.
- `clients/hackline-ts/src/types.ts` — `DeviceHealth` shape.
- `ui/hackline-ui/src/lib/utils.ts` — `relTime` accepts unix-secs.
- `ui/hackline-ui/src/modules/devices/DeviceDetailPage.tsx` —
  canonical field names.
- `clients/hackline-ts/test/health.test.ts` — direct assertion.
- `clients/hackline-ts/test/devices.test.ts` — real shape
  assertion replacing the 404 lock-in.
- `clients/hackline-ts/test/audit.test.ts` — broader assertion
  covering `device.create` + `tunnel.create` + `cmd.send`.

## What I deferred and why

- **`Device.last_seen_ts` → `last_seen_at` rename** in
  `clients/hackline-ts/src/types.ts` and the `Device` UI sites.
  The wire field is `last_seen_at` (openapi); the TS type calls it
  `last_seen_ts: string|null`. Fixing it is a wider sweep through
  `DevicesPage.tsx` and consumers. Worth a focused goal so the
  diff stays grepable.
- **V002 migration to add `ON DELETE SET NULL` on `audit.device_id`
  / `audit.tunnel_id`.** Would let audit rows naturally reference
  the entity FK instead of stuffing ids in `detail`. SQLite needs
  the table-recreate dance, which is sizeable enough that it
  deserves its own session note.
- **Synchronous `liveliness::Get` probe** so `DeviceHealth.rtt_ms`
  is non-null. Needs new gateway code on the Zenoh side, not just
  a handler tweak.

## What's next (goal 17 candidates)

- **Reconcile `Device.last_seen_at` shape on the TS client + UI.**
  Same kind of mechanical sweep as this goal.
- **V002 audit FK migration** — close the latent FK pinning
  documented in §Design.
- **SSE integration test in `@hackline/client`** — was deferred
  from goal 15 pending the `wire.ts::Event` vs
  `types.ts::GatewayEvent` reconciliation, which is still open.
- **Synchronous liveliness probe** so `rtt_ms` is non-null.
- **Wire `make test-client` into CI.**
