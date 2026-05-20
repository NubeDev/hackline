# 2026-05-15 — Goal 25: audit `subject` falls back to `detail.{tunnel,device}_id`

After V007 (goal 24) the audit FKs are `ON DELETE SET NULL`, so a
post-delete audit row reliably has `tunnel_id = NULL` /
`device_id = NULL`. The handlers in
`api/{tunnels,devices}/delete.rs` already insert with FK = NULL
and stuff the orphaned id into `detail` JSON. The goal-20
projection in `api/audit/list.rs` then emits an empty `subject`
for those rows, even though the originating entity id is right
there in the row's own `detail`. Result: `tunnel.delete` /
`device.delete` rows render with no clickable subject in the UI.

Goal 25 teaches `project()` to fall back to `detail.tunnel_id` /
`detail.device_id` when the FK column is NULL, recovering the
subject without changing the wire shape or the DB.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Confirm the gap: `api/{tunnels,devices}/delete.rs` write `detail = {"tunnel_id": id}` / `{"device_id": id}` and pass FK = NULL; current `project()` only reads the FK columns. | [x] |
| 1 | Refactor `project()` so `detail` is parsed first; the `subject` ladder reads FK → `detail.tunnel_id` → `detail.device_id` → user FK → empty string. Add `detail_id()` helper for the lookup. | [x] |
| 2 | Comment the `else if` ordering so it does not get reordered: tunnel FK > device FK > tunnel-from-detail > device-from-detail > user. The "from-detail" steps are *only* hit on parent-deleted rows; FK-bearing rows still hit the FK branches first. | [x] |
| 3 | Lock-in tests in `api::audit::list::tests` — 7 cases covering: tunnel FK wins over device FK; device FK wins when tunnel NULL; recovered tunnel from `detail` after delete; recovered device from `detail` after delete; user-only when no other signal; empty subject when nothing recoverable; non-JSON `detail` is wrapped *and* the user fallback still fires. | [x] |
| 4 | Verify gates: `cargo check --workspace` (only pre-existing `hackline-agent` warning), `cargo test --workspace`, `cargo build -p hackline-gateway --bin serve`, `pnpm` builds, `make test-client` green twice, dev stack `:8080`/`:1430` health 200/200. | [x] |

## Design

**Why the fallback ladder is FK first, detail second, user last.**
The FK columns are the canonical answer when present — they
survive parent renames and never lie about which row was the
operand. The `detail.<id>` fields only exist on rows where the
parent has been deleted, and only because the delete handler was
told to put them there; they are scoped to two action codes
(`tunnel.delete`, `device.delete`). Reading them ahead of the FK
would risk a future bug where `detail.tunnel_id` is something
other than "the tunnel this row references" (it's a free-form
JSON field). User comes last because `actor_user_id` is the
*who*, not the *what* — every authenticated row has it, so
promoting it ahead of detail-recovery would shadow the actual
subject for every `tunnel.delete` row.

**Why a tiny `detail_id()` helper instead of `as_i64()` inline
twice.** The lookup has two call sites and the conversion has
one specific failure mode (the JSON value isn't a number). One
named function makes the contract explicit and lets a future
fix (e.g. accepting string ids if `detail` is ever produced by a
client that boxes ids as strings) land in one place. CLAUDE.md
rule against over-abstracting — but two call sites with
identical logic and a real type-conversion concern clears the
bar.

**Why parse `detail` once, before the subject ladder.** The
previous code parsed `detail` at the bottom and built `subject`
from FKs at the top, with no shared dependency. Now the subject
ladder needs the parsed value; parsing once avoids both
duplicate work and the alternative refactor where the ladder
takes a borrowed `&Option<String>` and re-parses on cache miss.

**Why no openapi change.** The `subject` field's wire contract
is unchanged: still `"<resource>:<id>"` or empty string, still
matching the openapi schema's `pattern`-free `string` type. Only
*more* rows now get a non-empty value than before. UI consumers
that already render `subject` keep working; rows that previously
rendered empty now render the recovered subject.

**Why a cfg-test mod, not an integration test.** The
transformation under test is pure: `AuditEntry → AuditEntryView`.
No connection, no axum router, no request/response cycle. A
unit test on `project()` is the smallest faithful exercise and
runs in microseconds. The full HTTP round-trip is already
covered by `clients/hackline-ts/test/audit.test.ts`.

## Outcome

- `cargo check --workspace` clean except the pre-existing
  `hackline-agent PortDenied` warning (CLAUDE.md rule 3).
- `cargo test --workspace` green; 7 new
  `api::audit::list::tests::*` cases pass alongside the
  goal-22/23/24 lib tests (was 3 lib tests in
  `hackline-gateway`, now 10).
- `cargo build -p hackline-gateway --bin serve` clean.
- `pnpm -C clients/hackline-ts build` clean (no TS surface
  changed).
- `pnpm -C ui/hackline-ui typecheck` + `build` clean (bundle
  unchanged at 258.59 KB / 78.40 KB gz).
- `make test-client` 6 files / 12 tests green twice in a row.
- Dev stack health on `:8080` / `:1430` still 200 / 200.

Files modified:

- `crates/hackline-gateway/src/api/audit/list.rs` — `project()`
  parses `detail` first; subject ladder gains two
  detail-recovery branches between the FK and user steps; new
  `detail_id()` helper; new `#[cfg(test)] mod tests` with 7
  cases.

Files added: none.

Working tree intentionally dirty for operator review.

## What I deferred and why

Same as goal 24 minus the audit-subject-from-detail item:

- **`User` shape sweep needs an operator schema decision first**
  (auth model vs multi-tenant model).
- **`CmdOutboxRow` shape — no openapi schema to align against.**
- **`Device.org_id`** — schema decision.
- **`Device.class` and `Device.online` not on the wire.**
- **Synchronous `liveliness::Get` probe** —
  `DeviceHealth.rtt_ms`. Touches Zenoh runtime; not as small as
  this goal.
- **SSE integration test in `@hackline/client`** (pending
  goal-15 `wire.ts::Event` vs `types.ts::GatewayEvent`
  reconciliation).
- **Wire `make test-client` into CI.**
- **Drop the "id-in-detail" workaround in
  `api/{tunnels,devices}/delete.rs`** — would mean inserting
  with the FK populated and relying on V007 to clear it. Race-
  prone (the projection would briefly show a `subject` whose
  parent is gone) and the current pattern is more honest about
  what the delete event represents. Left as-is.

## What's next (goal 26 candidates)

- **Synchronous `liveliness::Get` probe** so
  `DeviceHealth.rtt_ms` is non-null on
  `GET /v1/devices/:id/health`.
- **Wire `make test-client` into CI.**
- **SSE integration test in `@hackline/client`** (depends on
  the goal-15 `wire.ts::Event` vs `types.ts::GatewayEvent`
  reconciliation; listed for visibility).
- **Audit `actor_user_id` projection: backfill from session
  metadata for `auth.login` rows where `user_id` is set after
  the row is written** — small, mechanical, no schema change.
- **Operator decisions** (`User`, `CmdOutboxRow`,
  `Device.org_id`, `Device.class`/`online`) — not picked
  autonomously.
