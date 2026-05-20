# 2026-05-15 — Goal 21: rename page envelope `entries` → `items`

`DOCS/openapi.yaml` §AuditPage names the page envelope field
`items` and types the cursor `integer | null`. The codebase had
drifted on both axes: every paginated handler (audit, events, cmd
outbox, logs) emitted `{entries, next_cursor}`, and the TS
`Page<T>` typed `next_cursor: string | null`. Goal 20's session
note logged this as the smallest cross-cutting follow-up; this
goal lands it.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Verify scope: four server handlers serialise `entries` (`api/audit/list.rs`, `api/events/list.rs`, `api/cmd/list.rs`, `api/logs/list.rs`); two UI pages read `.entries` (`AuditPage`, `CmdOutboxPage`); one TS test reads `.entries` (`audit.test.ts`); single TS `Page<T>` shape. Audit's `next_cursor` is `Option<String>` server-side, every other endpoint is `Option<i64>`, openapi is `integer\|null`. | [x] |
| 1 | `crates/hackline-gateway/src/api/audit/list.rs`: rename field `entries` → `items`; retype `next_cursor: Option<String>` → `Option<i64>` (always `None` today, no behaviour change); update doc comment to point at openapi. | [x] |
| 2 | `crates/hackline-gateway/src/api/events/list.rs`: rename field + handler binding `entries` → `items`. | [x] |
| 3 | `crates/hackline-gateway/src/api/cmd/list.rs`: same. | [x] |
| 4 | `crates/hackline-gateway/src/api/logs/list.rs`: same. | [x] |
| 5 | `clients/hackline-ts/src/types.ts`: `Page<T>.entries` → `items`; `next_cursor: string\|null` → `number\|null`; comment names the prior drift. | [x] |
| 6 | `clients/hackline-ts/src/client.ts` + `src/http-client.ts`: `cursor?: string\|null` → `number\|null` on `listCmd` and `listAudit`; URLSearchParams writes `String(cursor)`. | [x] |
| 7 | `ui/hackline-ui/src/modules/audit/AuditPage.tsx`: read `page.items`. | [x] |
| 8 | `ui/hackline-ui/src/modules/cmd/CmdOutboxPage.tsx`: read `page.items`. | [x] |
| 9 | `clients/hackline-ts/test/audit.test.ts`: `page.entries` → `page.items` (5 sites). | [x] |
| 10 | Verify: `cargo check --workspace` clean (only pre-existing `hackline-agent PortDenied` warning), `cargo test --workspace` green, `pnpm -C clients/hackline-ts build` clean, `pnpm -C ui/hackline-ui typecheck` + `build` clean, `make test-client` green twice in a row, dev stack `:8080` / `:1430` health still 200/200. | [x] |

## Design

**Why all four handlers in one goal.** The TS `Page<T>` is a
single shared shape — renaming the field on it forces every
endpoint that returns it to flip together, otherwise the type
stops describing the wire and the lock-in tests fail in
unintended places. Doing one endpoint at a time would mean
introducing a `PageOld<T>` / `PageNew<T>` split for the duration,
which is more diff than just doing it once.

**Why retype audit's `next_cursor` from `Option<String>` to
`Option<i64>`.** The field has been `None` since the handler
landed (audit list has no server-side pagination yet) so no
client has ever observed a non-null value. Openapi documents
`integer | null`; matching it now means the future pagination
addition won't have to break-change the shape again. The other
three endpoints already used `Option<i64>` — this aligns audit
with them.

**Why also retype TS `cursor` query params on `listCmd` /
`listAudit` from `string | null` to `number | null`.** The cursor
returned by the server is now uniformly numeric; if the consumer
hands it back the round-trip should be number-typed too. The
prior `if (input.cursor) qs.set(...)` truthiness check would have
silently dropped `cursor: 0` (a real, valid id); the new
`!= null` check honours zero. Same correctness fix on both
endpoints.

**Why I did not also rename the parent struct names
(`AuditPage`, `CmdPage`, `EventsPage`, `LogsPage`).** Those are
Rust-internal names, never on the wire — only the *field* name
is part of the contract. Mass-renaming them is churn for no
external benefit.

**Why the dev stack on `:8080` was not restarted.** Goal 17's
brief told the operator to leave the dev stack untouched and
re-verify health at the end. The dev gateway is a long-running
process built before this goal; until the operator restarts it,
its `/v1/audit` will still emit the old `entries` field and the
locally-served `:1430` UI will see an empty audit table. That's
an operator action, not a goal step. Health endpoints unaffected
either way (200/200 throughout).

## Outcome

- `cargo check --workspace` clean except the pre-existing
  `hackline-agent PortDenied` dead-code warning (CLAUDE.md
  rule 3).
- `cargo test --workspace` all green.
- `pnpm -C clients/hackline-ts build` clean.
- `pnpm -C ui/hackline-ui typecheck` clean.
- `pnpm -C ui/hackline-ui build` clean (bundle 258.59 KB / 78.40
  KB gz; +20 B from the field-name renames).
- `make test-client` 6 files / 12 tests green twice in a row from
  a cold vitest spawn — the harness rebuilds and re-spawns
  `target/debug/serve` per run, so the new envelope is exercised
  end-to-end.
- Dev stack health on `:8080` / `:1430` still 200 / 200; the
  long-running dev gateway keeps the old wire shape until
  restart (see Design).

Files modified:

- `crates/hackline-gateway/src/api/audit/list.rs` — `items` +
  numeric cursor.
- `crates/hackline-gateway/src/api/events/list.rs` — `items`.
- `crates/hackline-gateway/src/api/cmd/list.rs` — `items`.
- `crates/hackline-gateway/src/api/logs/list.rs` — `items`.
- `clients/hackline-ts/src/types.ts` — `Page<T>` rename + retype.
- `clients/hackline-ts/src/client.ts` — `cursor: number|null`.
- `clients/hackline-ts/src/http-client.ts` — `cursor: number|null`,
  `!= null` truthiness fix.
- `ui/hackline-ui/src/modules/audit/AuditPage.tsx` — `.items`.
- `ui/hackline-ui/src/modules/cmd/CmdOutboxPage.tsx` — `.items`.
- `clients/hackline-ts/test/audit.test.ts` — `.items` (5 sites).

Files added: none.

Working tree intentionally dirty for operator review.

## What I deferred and why

Same list as goal 20, minus the page-envelope item:

- **`User` shape sweep is bigger than a rename.** Server emits
  `name / role / device_scope / tunnel_scope / expires_at` (auth
  concept); openapi requires `label / scope / customer_id /
  last_used_at` (multi-tenant concept). These describe two
  different mental models of users — `role` enum is `[owner,
  admin, operator, viewer]`, `scope` enum is `[owner, customer]`.
  Reconciling them needs a schema decision (drop `role`? drop
  `scope`? unify?), not a TS-side rename. Worth a focused goal
  with the operator picking a side first.
- **`CmdOutboxRow` shape is divergent** — TS calls timestamps
  `string`, omits `id` / `content_type` / `payload` / `attempts`,
  renames `ack_at` / `ack_result` / `ack_detail` to `acked_at` /
  `result` / `detail`. Whole-object retype + rename family.
- **`Device.org_id`** — schema decision (add to openapi or drop
  from server projection).
- **`Device.class` and `Device.online` are not on the wire** —
  needs a per-row health fan-out and a schema decision.
- **`tunnel.session` audit rows store `ts` in milliseconds** while
  point-in-time rows store seconds; the goal-20 projection passes
  through whatever the DB has. Heterogeneous units in one column.
- **V002 audit-FK migration** — close the latent FK pinning
  (still pending from goal 16).
- **Synchronous `liveliness::Get` probe** — `DeviceHealth.rtt_ms`
  (still pending from goal 16).
- **SSE integration test in `@hackline/client`** (pending goal-15
  `wire.ts::Event` vs `types.ts::GatewayEvent` reconciliation).
- **Wire `make test-client` into CI.**

## What's next (goal 22 candidates)

- **`CmdOutboxRow` shape sweep** — biggest of the remaining
  pure-mechanical reconciliations; touches `CmdOutboxPage`.
- **`User` shape decision + sweep** — needs an operator decision
  first.
- **`tunnel.session` ts unit normalisation** — pick seconds, fix
  `insert_tunnel_session_open` to store seconds. Latent
  inconsistency the goal-20 projection now exposes.
- **V002 audit-FK migration** — close the latent FK pinning;
  unlocks dropping the `device:N` / `tunnel:N` fallback in the
  goal-20 projection (subject can use the FK column directly).
- **Synchronous liveliness probe** — `DeviceHealth.rtt_ms`.
- **Wire `make test-client` into CI.**
