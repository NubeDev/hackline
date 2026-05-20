# 2026-05-15 — Goal 24: V007 — `audit.{user,device,tunnel}_id` ON DELETE SET NULL

The original V001 schema gave each `audit` foreign key no
`ON DELETE` clause. With `PRAGMA foreign_keys = ON` enforced
per-connection in `db/pool.rs`, this means deleting any tunnel
or device that has accumulated audit history fails outright with
an FK violation. The handlers in `api/{tunnels,devices}/delete.rs`
already work around the *new* audit-row insert by passing `None`
for the FK and stuffing the to-be-orphaned id into `detail` JSON,
but the workaround does not help the *pre-existing* audit rows
whose FKs still pin the parent. V007 swaps the FK clauses to
`ON DELETE SET NULL` via the SQLite recreate-table dance.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Confirm the schema problem: `V001__init.sql` declares `audit.user_id`, `audit.device_id`, `audit.tunnel_id` as bare `REFERENCES` columns; `db/pool.rs` sets `PRAGMA foreign_keys = ON` on every connection; `api/{tunnels,devices}/delete.rs` already comment "would dangle (PRAGMA foreign_keys = ON)". | [x] |
| 1 | New `migrations/V007__audit_fks_on_delete_set_null.sql` doing the SQLite 12-step recreate dance: `PRAGMA foreign_keys = OFF` → `BEGIN` → `CREATE TABLE audit_new` with `ON DELETE SET NULL` on all three FKs → copy → drop → rename → recreate three indexes → `PRAGMA foreign_key_check` → `COMMIT` → `PRAGMA foreign_keys = ON`. | [x] |
| 2 | Register V007 in `src/db/migrations.rs` const list. | [x] |
| 3 | Lock-in test in `db::audit::tests`: insert a tunnel + an audit row referencing it, delete the tunnel, assert the audit row survives with `tunnel_id = NULL`; then delete the device and assert `device_id = NULL`. | [x] |
| 4 | Verify gates: `cargo check --workspace` (only pre-existing `hackline-agent` warning), `cargo test --workspace`, `cargo build -p hackline-gateway --bin serve`, `pnpm` builds, `make test-client` green twice, dev stack `:8080`/`:1430` health 200/200. | [x] |

## Design

**Why `ON DELETE SET NULL` and not `CASCADE`.** Audit history is
forever — that is the point of the `audit` table. Cascading a
parent delete into the audit row would silently erase the
historical trail of operations on that parent. SET NULL preserves
the row (and its `action` + `detail` payload) while letting the
parent delete succeed. Readers that want "which tunnel did this
correspond to" can recover the id from `detail` if the delete
handler stamped it (which the current handlers do).

**Why the dance, not `ALTER TABLE`.** SQLite has no
`ALTER TABLE ... ALTER CONSTRAINT`. The official documented
procedure (https://www.sqlite.org/lang_altertable.html §7) is
the 12-step recreate. The migration follows it verbatim:
disable FKs (must be outside any transaction; the runner
in `db/migrations.rs` does not wrap migrations in an outer
transaction, so this is safe), open the migration's own
transaction, create the new table, copy rows, drop, rename,
recreate indexes (which are dropped with the table), run
`PRAGMA foreign_key_check` for last-line safety, commit,
re-enable FKs.

**Why a `foreign_key_check` before commit.** The recreate is
performed with FKs off. If any pre-existing row in `audit`
already violated its FK (e.g. a tunnel_id pointing to a deleted
tunnel from before this fix), turning FKs back on later would
produce silent inconsistency on the first new operation that
hits the row. `foreign_key_check` returns one row per violation;
a non-empty result aborts the transaction so the migration fails
loudly instead of leaving the DB half-converted.

**Why no caller changes.** The delete handlers in
`api/{tunnels,devices}/delete.rs` already do the right thing for
the post-delete audit insert (FK is `None`, id is in `detail`).
Removing the workaround now would mean changing those handlers
to pass the FK id and rely on SET NULL to clear it after the
parent goes — but that race would be observable in the
projection (`subject = "tunnel:N"` for a row whose tunnel no
longer exists, until the next operation on the same connection).
The current "id-in-detail" pattern is more honest about what the
delete event actually represents. CLAUDE.md rule: no drive-by
refactors. Left untouched.

**Why explicit `PRAGMA foreign_keys = ON` in the test.** A fresh
`Connection::open_in_memory` has FKs *off* by default; the
production pool turns them on. The test toggles them on after
running migrations so the lock-in actually exercises the
ON DELETE behaviour. The migration's final `PRAGMA foreign_keys
= ON` would also do the job in this case, but coupling the test
to that internal detail would be brittle.

## Outcome

- `cargo check --workspace` clean except the pre-existing
  `hackline-agent PortDenied` warning (CLAUDE.md rule 3).
- `cargo test --workspace` green; new
  `db::audit::tests::v007_audit_fks_set_null_on_parent_delete`
  passes alongside goal-22 + goal-23 lock-ins (was 2 lib tests
  in `hackline-gateway`, now 3).
- `cargo build -p hackline-gateway --bin serve` clean.
- `pnpm -C clients/hackline-ts build` clean (no TS surface
  changed).
- `pnpm -C ui/hackline-ui typecheck` + `build` clean (bundle
  unchanged at 258.59 KB / 78.40 KB gz).
- `make test-client` 6 files / 12 tests green twice in a row.
- Dev stack health on `:8080` / `:1430` still 200 / 200.

Files added:

- `crates/hackline-gateway/migrations/V007__audit_fks_on_delete_set_null.sql`

Files modified:

- `crates/hackline-gateway/src/db/migrations.rs` — register V007
  in the const list.
- `crates/hackline-gateway/src/db/audit.rs` — V007 lock-in test.

Working tree intentionally dirty for operator review.

## What I deferred and why

Same as goal 23 minus the audit-FK item:

- **`User` shape sweep needs an operator schema decision first**
  (auth model vs multi-tenant model).
- **`CmdOutboxRow` shape — no openapi schema to align against.**
- **`Device.org_id`** — schema decision.
- **`Device.class` and `Device.online` not on the wire.**
- **Synchronous `liveliness::Get` probe** —
  `DeviceHealth.rtt_ms`.
- **SSE integration test in `@hackline/client`** (pending
  goal-15 `wire.ts::Event` vs `types.ts::GatewayEvent`
  reconciliation).
- **Wire `make test-client` into CI.**
- **Dropping the "id-in-detail" workaround in
  `api/{tunnels,devices}/delete.rs`** — see Design above. Would
  need a projection-side change to derive `subject` from
  `detail.tunnel_id` / `detail.device_id` when the FK is NULL,
  which is its own tracked change rather than a drive-by.

## What's next (goal 25 candidates)

- **Synchronous `liveliness::Get` probe** so `DeviceHealth.rtt_ms`
  is non-null on `GET /v1/devices/:id/health`.
- **Wire `make test-client` into CI.**
- **SSE integration test in `@hackline/client`** (depends on
  the goal-15 `wire.ts::Event` vs `types.ts::GatewayEvent`
  reconciliation; listed for visibility).
- **Audit `subject` projection: derive from `detail` when FK is
  NULL** — small, mechanical, builds on V007.
- **Operator decisions** (`User`, `CmdOutboxRow`,
  `Device.org_id`, `Device.class`/`online`) — not picked
  autonomously.
