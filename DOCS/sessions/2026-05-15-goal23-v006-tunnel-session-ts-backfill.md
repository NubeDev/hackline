# 2026-05-15 — Goal 23: V006 backfill — `tunnel.session` audit `ts` ms→s

Goal 22 stopped new `tunnel.session` rows from carrying ms-shaped
`ts` / `ts_close` values, but pre-fix rows already in any deployed
audit table still hold ms. The goal-20 REST projection promises
seconds (openapi §AuditEntry.at), so a pre-fix row renders ~55,000
years in the future under the UI's `relTime`. V006 is the
forward-only backfill: scoped to `action = 'tunnel.session'`,
gated on a threshold that is unambiguously not seconds today.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Confirm next migration number is V006 (V001..V005 exist; numbers are dense and never reused per `migrations/README.md`). | [x] |
| 1 | New `migrations/V006__audit_tunnel_session_ts_seconds.sql`: `UPDATE audit SET ts = ts/1000 WHERE action='tunnel.session' AND ts > 10_000_000_000;` and the same for `ts_close` (NULL-guarded). Header comment names the heuristic and the year-2286 rationale. | [x] |
| 2 | Register V006 in `src/db/migrations.rs` const list. | [x] |
| 3 | Lock-in test in `db::audit::tests`: pre-seed an audit row with ms `ts`+`ts_close`, a `tunnel.session` row already in seconds, and a non-tunnel row with a deliberately large seconds value. Re-apply V006. Assert the legacy row was halved, the modern row round-trips, the non-tunnel row is untouched. | [x] |
| 4 | Verify gates: `cargo check --workspace` (only pre-existing `hackline-agent` warning), `cargo test --workspace`, `cargo build -p hackline-gateway --bin serve`, `pnpm` builds, `make test-client` green twice, dev stack `:8080`/`:1430` health 200/200. | [x] |

## Design

**Why a threshold instead of "all `tunnel.session` rows".** A
seconds-shaped epoch crosses 10_000_000_000 in year 2286.
Anything above that today must be ms. Anything at or below it is
either a seconds value already (modern, post-goal-22) or a ms
value old enough to plausibly *also* be a seconds value
(pre-1970 in ms is < 0; first 10^10 ms is < 1970-04-26 in
seconds). In practice the production DB only contains either
post-1970 ms (always > 10^10) or post-goal-22 s (always < 10^10
for the next 260 years), so the partition is exact. Guarding on
the threshold also makes the migration safe to re-run by hand
during recovery without double-dividing a row that was already
fixed.

**Why scope to `action = 'tunnel.session'`.** No other audit
action has ever been stamped in ms. Restricting the UPDATE to
that action means a future bug that stamps a different action in
ms cannot be silently "fixed" by V006 running on next boot — it
would surface as a UI date issue and get its own targeted goal.
Defensive narrowing.

**Why test V006 by re-executing the SQL.** The migration runner
has already applied V006 by the time `fresh_db()` returns, and
applying the same migration version twice is normally rejected
by the `_migrations` table. Re-executing the raw SQL via
`execute_batch` exercises the actual UPDATE statements without
fighting the runner's bookkeeping. The migration is idempotent
under the threshold guard, so this is a faithful test of what
runs in prod on first-boot after this version ships.

**Three-row test fixture.** One row exercises the rewrite, one
guards against false positives on already-fixed rows (regression
catch if someone widens the threshold), one guards against the
WHERE clause growing past `tunnel.session` (regression catch if
someone drops the action filter "to be safe"). Each row has a
specific defensive job; all three are needed.

**Operator concerns.** The dev gateway on `:8080` is still
running the pre-goal-22 binary (operator owns dev-stack
lifecycle). When the operator next restarts that gateway, V006
runs against the dev DB and fixes any `tunnel.session` rows that
accumulated under the old code path. Brand-new DBs have no rows
to backfill and the migration is a no-op.

## Outcome

- `cargo check --workspace` clean except the pre-existing
  `hackline-agent PortDenied` warning (CLAUDE.md rule 3).
- `cargo test --workspace` green; new
  `db::audit::tests::v006_backfill_converts_ms_rows_only` passes
  alongside the goal-22 lock-in (was 1 lib test, now 2).
- `cargo build -p hackline-gateway --bin serve` clean.
- `pnpm -C clients/hackline-ts build` clean (no TS surface
  changed).
- `pnpm -C ui/hackline-ui typecheck` + `build` clean (bundle
  unchanged at 258.59 KB / 78.40 KB gz).
- `make test-client` 6 files / 12 tests green twice in a row.
- Dev stack health on `:8080` / `:1430` still 200 / 200.

Files added:

- `crates/hackline-gateway/migrations/V006__audit_tunnel_session_ts_seconds.sql`

Files modified:

- `crates/hackline-gateway/src/db/migrations.rs` — register V006
  in the const list.
- `crates/hackline-gateway/src/db/audit.rs` — V006 lock-in test.

Working tree intentionally dirty for operator review.

## What I deferred and why

Same as goal 22 minus the V006 backfill item:

- **`User` shape sweep needs an operator schema decision first**
  (auth model vs multi-tenant model).
- **`CmdOutboxRow` shape — no openapi schema to align against.**
- **`Device.org_id`** — schema decision.
- **`Device.class` and `Device.online` not on the wire.**
- **Audit-FK migration** — close the latent FK pinning so the
  goal-20 `subject` projection can drop its `device:N` /
  `tunnel:N` fallback. SQLite needs the table-recreate dance.
- **Synchronous `liveliness::Get` probe** — `DeviceHealth.rtt_ms`.
- **SSE integration test in `@hackline/client`** (pending the
  goal-15 `wire.ts::Event` vs `types.ts::GatewayEvent`
  reconciliation).
- **Wire `make test-client` into CI.**

## What's next (goal 24 candidates)

- **Audit-FK migration** — append-only Vnnn migration adding
  `ON DELETE SET NULL` to `audit.user_id`, `audit.device_id`,
  `audit.tunnel_id` via the SQLite recreate-table dance.
  Unlocks dropping the goal-20 audit `subject` fallback.
- **Synchronous liveliness probe** so `DeviceHealth.rtt_ms` is
  non-null.
- **Wire `make test-client` into CI.**
- **SSE integration test in `@hackline/client`** (depends on
  goal-15 reconciliation, listed for visibility).
- **Operator decisions** (`User`, `CmdOutboxRow`, `Device.org_id`,
  `Device.class`/`online`) — not picked autonomously.
