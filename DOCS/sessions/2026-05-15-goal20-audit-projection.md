# 2026-05-15 — Goal 20: project audit-list response to the openapi shape

The TS `AuditEntry` had drifted not just from openapi but from the
gateway's actual JSON: `e.actor` and `e.target` were rendered in
`AuditPage.tsx` as `undefined` (the server has never emitted those
fields), and `e.ts` was typed `string` while the server emits
`number`. The fix needs both ends — the server's
`db::audit::AuditEntry` carries a wider, internal shape (extras for
`tunnel.session` rows) that should not be on the wire.

This goal lands an `AuditEntryView` projection at the REST boundary
in `crates/hackline-gateway/src/api/audit/list.rs` matching the
`DOCS/openapi.yaml` §AuditEntry contract
(`{id, at, actor_user_id, action, subject, detail}`), realigns the
TS type, fixes the UI to read the new field names, and drops the
`as unknown as` cast from the audit test in favour of a real
shape lock-in.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Verify the wire today: `db::audit::AuditEntry` has 12 columns including `tunnel.session` extras (`ts_close`, `bytes_up`, `bytes_down`, `peer`, `request_id`); openapi §AuditEntry requires 6 (`id, at, actor_user_id, action, subject, detail`); TS shadow type names neither set correctly. UI consumers `e.actor` / `e.target` read undefined. | [x] |
| 1 | `crates/hackline-gateway/src/api/audit/list.rs`: add `AuditEntryView` projection + `project()` helper; `subject` derives from `tunnel_id` ‖ `device_id` ‖ `user_id` (most-specific-FK wins); `detail` parses the stored JSON string, wraps non-JSON in `{ "raw": s }` to satisfy openapi `type: object`. Handler maps each row through it. | [x] |
| 2 | `clients/hackline-ts/src/types.ts`: rewrite `AuditEntry` to the openapi shape; comment names the prior fabrication. | [x] |
| 3 | `ui/hackline-ui/src/modules/audit/AuditPage.tsx`: render `relTime(e.at)`, `e.actor_user_id` as `user:N` or `—`, `e.action`, `e.subject`, `e.detail`. Header `Target` → `Subject`. | [x] |
| 4 | `clients/hackline-ts/test/audit.test.ts`: drop the `as unknown as` cast; assert each entry conforms to the openapi shape (`typeof` checks for every field); pre-existing detail-substring searches updated for `detail` being an object (`JSON.stringify(detail).includes(needle)`). | [x] |
| 5 | Verify: `cargo check --workspace` (only pre-existing `hackline-agent PortDenied` warning), `cargo test --workspace` green, `pnpm -C clients/hackline-ts build` clean, `pnpm -C ui/hackline-ui typecheck` + `build` clean, `make test-client` green twice in a row, dev stack `:8080` / `:1430` still 200/200. | [x] |

## Design

**Why project at the REST boundary instead of changing the DB row.**
The `db::audit::AuditEntry` is also consumed internally by
liveliness / bridge code that finalises `tunnel.session` rows; that
code legitimately needs `ts_close`, `bytes_up`, `bytes_down`,
`request_id`, `peer`. Splitting the DB row would force a parallel
write path or a wider join. A view at the handler costs one
allocation per row and keeps the contract compact, which is what
the openapi schema documents anyway — internal shape ≠ wire shape.

**Why subject prefers `tunnel:N` over `device:N` over `user:N`.**
Most specific FK wins. A `tunnel.session` row carries all three;
the operator reading the audit log wants to jump to the tunnel,
not the owning device. A bare `device.create` row from goal 16's
audit work has `device_id = NULL` (FK pinning, see goal-16
Design); the projection then falls back to `user:<actor>` so the
column is never blank for a real action. Openapi requires the
field but not non-emptiness; truly anonymous rows (none of the
three FKs) project an empty string.

**Why `detail` falls back to `{ "raw": s }` on parse failure.**
Openapi marks `detail: type: object`. The DB column is text and
historically held both JSON and free-form strings (older
`auth.login` / claim rows). Wrapping non-JSON keeps the contract
intact without backfilling old data; the operator sees
`{"raw":"..."}` and knows to interpret it literally.

**Why I did not rename the page envelope from `entries` to `items`.**
Openapi §AuditPage uses `items`; every other paginated response in
this codebase (cmd outbox listing, future device cursor pages, the
TS `Page<T>` type) uses `entries`. Renaming the audit envelope
alone breaks the cross-endpoint shape; renaming all of them is a
ripple goal of its own. Documented as deferred so this projection
goal lands in isolation.

**Why the test now scans `JSON.stringify(detail)` for substrings.**
The pre-existing assertions hunted for `zid`, the public hostname,
and `cmd_id` inside `e.detail` when `detail` was a string. After
projection `detail` is an object; the same intent ("did this row
mention X?") becomes a substring match against the serialised form.
The check is loose enough to survive future detail-shape changes
(e.g. nesting under `device_id` / `tunnel_id`) without churn.

**Why the subject equality assertion was relaxed.** I initially
wrote `expect(deviceCreate?.subject).toBe(\`device:${dev.id}\`)`,
which failed because goal 16 deliberately sets `device_id = None`
on the new audit rows (the FK would otherwise pin a still-live id
and block its own delete; see goal-16 Design). The projection then
returns `user:<actor>` as the most-specific subject for that row.
The assertion now checks subject is non-empty, which is the actual
invariant the projection guarantees today.

## Outcome

- `cargo check --workspace` clean except the pre-existing
  `hackline-agent PortDenied` dead-code warning (CLAUDE.md rule 3).
- `cargo test --workspace` all green.
- `pnpm -C clients/hackline-ts build` clean.
- `pnpm -C ui/hackline-ui typecheck` clean.
- `pnpm -C ui/hackline-ui build` clean (bundle 258.57 KB / 78.39 KB
  gz; +50 B from the four-line UI rewrite).
- `make test-client` 6 files / 12 tests green twice in a row from a
  cold vitest spawn (test count unchanged from goal 19; the audit
  test gained shape assertions but not a new `it`).
- Dev stack on `:8080` / `:1430` still 200 / 200.

Files modified:

- `crates/hackline-gateway/src/api/audit/list.rs` — `AuditEntryView`
  projection + `project()` helper; handler maps through it.
- `clients/hackline-ts/src/types.ts` — `AuditEntry` rewritten to the
  openapi shape; doc comment naming the prior fabrication.
- `ui/hackline-ui/src/modules/audit/AuditPage.tsx` — read
  `at`/`actor_user_id`/`subject` instead of `ts`/`actor`/`target`;
  Subject column header.
- `clients/hackline-ts/test/audit.test.ts` — drop `as unknown as`;
  add per-entry `typeof` shape assertions; substring helper for
  `detail` as object.

Files added: none.

Working tree intentionally dirty for operator review.

## What I deferred and why

- **Page envelope rename `entries` → `items` per openapi.** Touches
  every paginated endpoint (audit, cmd outbox listing) and the
  `Page<T>` TS type. Ripple goal.
- **`User` shape is widely divergent** — different field names
  (`name`/`role`/`device_scope`/`tunnel_scope` vs openapi
  `label`/`scope`/`customer_id`/`last_used_at`) and types. Touches
  `UsersPage`. The biggest of the remaining drifts.
- **`CmdOutboxRow` shape is divergent** — TS calls timestamps
  `string`, omits `id`/`content_type`/`payload`/`attempts`, renames
  `ack_at`/`ack_result`/`ack_detail` to `acked_at`/`result`/`detail`.
  Whole-object retype + rename family.
- **`Device.org_id`** is on the wire but absent from the TS type;
  schema decision (add to openapi or drop from server projection).
- **`Device.class` and `Device.online` are not on the wire.** Needs
  a per-row health fan-out and a schema decision.
- **`tunnel.session` audit rows store `ts` in milliseconds** while
  every other row stores seconds; the projection passes through
  whatever the DB has. Heterogeneous units in one column is a
  latent bug — its own goal.
- **V002 audit-FK migration** — close the latent FK pinning
  (still pending from goal 16).
- **Synchronous `liveliness::Get` probe** — `DeviceHealth.rtt_ms`
  (still pending from goal 16).
- **SSE integration test in `@hackline/client`** (pending goal-15
  `wire.ts::Event` vs `types.ts::GatewayEvent` reconciliation).
- **Wire `make test-client` into CI.**

## What's next (goal 21 candidates)

- **`User` shape sweep** — biggest remaining type-only drift.
  Touches `UsersPage` (rename `u.name` → `u.label`, `u.role` →
  `u.scope`, etc.).
- **`CmdOutboxRow` shape sweep** — same kind of mechanical
  retype + rename family.
- **Page envelope rename `entries` → `items`** — small but
  cross-cutting; breaks every list call site at once.
- **`tunnel.session` ts unit normalisation** — pick seconds, fix
  `insert_tunnel_session_open` to store seconds, audit-page UI
  renders correctly for both old and new rows.
- **V002 audit-FK migration** — close the latent FK pinning.
- **Synchronous liveliness probe** — `DeviceHealth.rtt_ms`.
