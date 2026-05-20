# 2026-05-15 — Goal 22: normalise `tunnel.session` audit `ts` to seconds

Goal 20 surfaced a latent inconsistency the audit projection now
makes visible: every `audit.ts` value is unix epoch seconds *except*
`tunnel.session` rows, which the bridge code stamps in milliseconds.
Once the goal-20 projection ships and the dev gateway restarts, any
`tunnel.session` row in the audit log renders ~55,000 years in the
future under `relTime` (the UI assumes seconds per the goal-20
contract). Openapi §AuditEntry.at is canonically seconds.

The fix is to convert ms → s at the storage boundary
(`db::audit::insert_tunnel_session_open` and
`db::audit::finalize_tunnel_session`) so the `audit.ts` column
carries one unit for every row. Bridge call sites still pass ms
because the same value feeds duration_ms metrics; the boundary
function divides by 1000 once.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Verify the inconsistency: `crates/hackline-gateway/src/tunnel/{http_router,tcp_listener}.rs` call `audit::insert_tunnel_session_open(..., ts_open_ms = now_ms())` and `audit::finalize_tunnel_session(..., ts_close_ms = now_ms())`; both store ms straight into `audit.ts` / `audit.ts_close`. Every other audit insert uses `unixepoch()` (seconds). | [x] |
| 1 | `crates/hackline-gateway/src/db/audit.rs`: divide `ts_open_ms` and `ts_close_ms` by 1000 before binding. Doc comments name the unit invariant + the goal-20 projection contract. | [x] |
| 2 | Add `#[cfg(test)]` lock-in: open the bridge with a known ms value, finalize with another, assert both columns hold seconds. | [x] |
| 3 | Verify: `cargo check --workspace` clean (only pre-existing `hackline-agent PortDenied` warning), `cargo test --workspace` green (the new lib test runs), `pnpm -C clients/hackline-ts build` clean, `pnpm -C ui/hackline-ui typecheck` + `build` clean, `make test-client` green twice in a row, dev stack `:8080` / `:1430` health still 200/200. | [x] |

## Design

**Why convert at the storage boundary instead of at the call sites.**
The bridge code computes `now_ms()` once and uses the same value for
two purposes: stamping the audit row, and computing `duration_ms`
when finalising the session for metrics. Forcing the call sites to
pass seconds would split the value into two variables (s and ms) for
the same instant, which is gratuitous churn. The cleaner contract is
"`audit.ts` is seconds; functions that take a ms value at the
boundary divide it once". Two functions, two divisions, one unit
invariant in the column.

**Why integer division by 1000 (not rounding).** Audit precision is
seconds-of-the-event; the sub-second remainder is metrics-grade,
not audit-grade. Truncation matches the existing `unixepoch()`
behaviour used by every other audit insert in the codebase.

**Why a unit test instead of an integration test.** The bridge code
that calls these functions is hard to exercise without a real Zenoh
session and a real listener; the value being asserted is the
transformation between the function arg and the stored column,
which is local to `db/audit.rs`. The lock-in is small and runs in
under a millisecond against an in-memory SQLite. The migration
seeds `org_id = 1` already, so the test pulls that id and creates
its own device row rather than hardcoding ids.

**What this *does not* fix.** Existing `tunnel.session` rows already
in any deployed gateway's `audit` table still have ms-stamped `ts`
values. Backfilling them would need a one-shot migration that scans
for ms-shaped values (heuristic: `ts > 10_000_000_000`, since any
seconds-shaped audit ts that large would represent ~year 2286), and
SCOPE rule 4 says migrations are append-only. A future V006 can
land that backfill in isolation; the goal here is to stop new rows
from being wrong. UI consumers that hit a pre-fix row will see the
same broken `relTime` they would have seen yesterday, which is no
worse than the status quo.

**Why the dev stack on `:8080` was not restarted.** Same reasoning
as goal 21: the operator owns dev-stack lifecycle. The change here
only affects rows newly inserted *after* the gateway restart;
existing dev-db rows still carry whatever they had.

## Outcome

- `cargo check --workspace` clean except the pre-existing
  `hackline-agent PortDenied` dead-code warning (CLAUDE.md
  rule 3).
- `cargo test --workspace` green; new
  `db::audit::tests::tunnel_session_ts_is_stored_in_seconds` test
  passes (was 0 lib tests in `hackline-gateway` before this goal,
  now 1).
- `pnpm -C clients/hackline-ts build` clean.
- `pnpm -C ui/hackline-ui typecheck` + `build` clean (bundle
  unchanged at 258.59 KB / 78.40 KB gz; no UI files touched).
- `make test-client` 6 files / 12 tests green twice in a row from
  a cold vitest spawn.
- Dev stack health on `:8080` / `:1430` still 200 / 200.

Files modified:

- `crates/hackline-gateway/src/db/audit.rs` — `ts_*_ms / 1000`
  conversion in `insert_tunnel_session_open` and
  `finalize_tunnel_session`; doc comments name the unit
  invariant; new `tests` mod with the lock-in.

Files added: none.

Working tree intentionally dirty for operator review.

## What I deferred and why

Same list as goal 21, minus the tunnel.session-ts item, plus a
new follow-up:

- **Backfill migration for existing `tunnel.session` rows with
  ms-stamped `ts`** (V006 candidate). Heuristic: `WHERE
  action = 'tunnel.session' AND ts > 10_000_000_000` then
  `UPDATE ... SET ts = ts / 1000, ts_close = ts_close / 1000`.
  Append-only per SCOPE rule 4; deserves its own session note.
- **`User` shape sweep needs an operator schema decision first** —
  server emits an auth concept (`name`/`role`/`device_scope`/
  `tunnel_scope`), openapi documents a multi-tenant concept
  (`label`/`scope`/`customer_id`/`last_used_at`). Picking a side
  is a design call.
- **`CmdOutboxRow` shape sweep has no openapi schema to align
  against.** The wire shape today is whatever
  `db::cmd_outbox::CmdRow` serialises; the TS type omits four
  required fields and renames three more. Bringing them into
  agreement needs an operator decision on what the canonical
  shape should be (extend openapi? trim TS? rename DB?), so it
  isn't a pure mechanical sweep.
- **`Device.org_id`** — schema decision (add to openapi or drop
  from server projection).
- **`Device.class` and `Device.online` are not on the wire** —
  needs a per-row health fan-out and a schema decision.
- **V002 audit-FK migration** — close the latent FK pinning
  (still pending from goal 16). Unlocks dropping the
  `device:N` / `tunnel:N` fallback in the goal-20 audit
  projection.
- **Synchronous `liveliness::Get` probe** — `DeviceHealth.rtt_ms`
  (still pending from goal 16).
- **SSE integration test in `@hackline/client`** (pending goal-15
  `wire.ts::Event` vs `types.ts::GatewayEvent` reconciliation).
- **Wire `make test-client` into CI.**

## What's next (goal 23 candidates)

- **V006 backfill migration** — convert pre-fix `tunnel.session`
  ms timestamps to seconds in any deployed audit table. Small,
  contained.
- **Operator decision on `User` shape**: which model wins (auth
  or multi-tenant) and what the openapi schema should describe.
  Spec work, then the sweep is mechanical.
- **Operator decision on `CmdOutboxRow` shape**: what belongs on
  the wire vs DB-internal. Spec work, then sweep.
- **V002 audit-FK migration** — close the latent FK pinning.
- **Synchronous liveliness probe** — `DeviceHealth.rtt_ms`.
- **Wire `make test-client` into CI.**
