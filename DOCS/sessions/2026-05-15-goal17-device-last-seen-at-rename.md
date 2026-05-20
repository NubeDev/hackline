# 2026-05-15 — Goal 17: rename `Device.last_seen_ts` to canonical `last_seen_at`

Goal 16 deferred the `Device` field rename because the fix ripples
through the TS client and every UI consumer; goal 16 only widened
`relTime` so the `DeviceHealth` line could render the canonical
`number | null` shape today. This goal closes that gap on the wider
`Device` type and lands a lock-in test so the lie can't sneak back.

The wire field — verified against the gateway source
(`crates/hackline-gateway/src/db/devices.rs::Device` returned by
`GET /v1/devices` and `GET /v1/devices/{id}`) and `DOCS/openapi.yaml`
§Device — is `last_seen_at: integer | null` (unix epoch seconds).
The TS client previously typed it as `last_seen_ts: string | null`,
wrong on both axes.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Verify the live gateway response shape: gateway source confirms `Device { id, org_id, zid, label, customer_id, created_at: i64, last_seen_at: Option<i64> }`; openapi §Device requires `[id, zid, label, customer_id, created_at, last_seen_at]` with `last_seen_at: integer\|null`, `created_at: integer`. Live `:8080` gateway returns 200 on `/v1/health`. | [x] |
| 1 | `clients/hackline-ts/src/types.ts`: rename `Device.last_seen_ts` → `last_seen_at`, retype `string\|null` → `number\|null`; retype `created_at: string` → `number` (openapi int64); retype `label: string\|null` → `string` (openapi required string). | [x] |
| 2 | `ui/hackline-ui/src/modules/devices/DeviceDetailPage.tsx`: read `device.last_seen_at`. | [x] |
| 3 | `ui/hackline-ui/src/modules/devices/DevicesPage.tsx`: read `d.last_seen_at`. | [x] |
| 4 | `clients/hackline-ts/test/devices.test.ts`: add a third `it` that creates a device, lists it, and asserts the wire-shape lock-in: `last_seen_ts` absent, `last_seen_at` present and is `number\|null` (never string), plus typeof checks for `id` / `zid` / `label` / `customer_id` / `created_at`. | [x] |
| 5 | Verify: `cargo check --workspace` (only pre-existing `hackline-agent PortDenied` warning), `cargo test --workspace` green, `pnpm -C clients/hackline-ts build` clean, `pnpm -C ui/hackline-ui typecheck` + `build` clean, `make test-client` green twice in a row from a cold tempdir, dev stack on `:8080` / `:1430` still 200/200. | [x] |

## Design

**Why retype `created_at` and `label` in the same pass.** The goal
brief permits "rename or retype only — no field additions/removals
beyond what openapi mandates" in the same sweep. Both `created_at:
string` and `label: string|null` are pure type lies (the wire is
`integer` and required-`string` respectively); fixing them is a
zero-risk retype. `relTime` already accepts numbers (goal 16) so
`Device.created_at` consumers don't need touching even though no
caller currently reads it. The `??` fallbacks on `device.label` in
the UI become dead code under the new non-null type but compile
fine; ripping them out is a drive-by cleanup the repo's CLAUDE.md
rule 8 forbids.

**Why I did *not* drop `class` and `online` in this pass.** Both
fields are absent from the wire (the Rust `Device` row doesn't
serialise them; openapi doesn't list them) — the legacy UI reads
`device.online` and `device.class` and gets `undefined` at runtime.
The brief allows removals "openapi mandates", but a mechanical
removal breaks `DevicesPage` (online badge per row, class column)
and `DeviceDetailPage` (online badge in header, agent-info gating
on `class === "linux"` / `"constrained"`) — none of which can be
fixed by a rename. The right replacement for `online` is a per-row
`getDeviceHealth()` fan-out (or a list endpoint that returns
health), and for `class` is either an openapi schema addition or
deprecation. Each is its own goal; lumping them in here would have
expanded the diff well past "rename + retype". Documented in
deferred below so the next agent can pick it up.

**Why the lock-in test casts `found` to `Record<string, unknown>`.**
`toMatchObject` would silently pass even if the server reintroduced
`last_seen_ts` alongside `last_seen_at`. The point of the lock-in is
to catch the *wrong* property name coming back, so we step out of
the typed view and assert the raw key set: `not.toHaveProperty
("last_seen_ts")` plus `toHaveProperty("last_seen_at")` plus
`typeof !== "string"`. Three independent guards, any one of which
flags a regression that the previous "this field exists and is
nullable-string" type would have allowed.

**Why I did not assert exact equality on the whole object.** The
server returns `org_id` (real), and may grow new optional fields
(forward-compatibility per openapi). `toEqual` would force a test
update on every additive change. The shape lock-in only needs to
guarantee the goal-17 invariant: `last_seen_at` is present,
`number | null`, never string.

## Outcome

- `cargo check --workspace` clean except the pre-existing
  `hackline-agent` `PortDenied` dead-code warning (CLAUDE.md rule 3).
- `cargo test --workspace` all green.
- `pnpm -C clients/hackline-ts build` clean.
- `pnpm -C ui/hackline-ui typecheck` clean.
- `pnpm -C ui/hackline-ui build` clean (bundle unchanged at
  258.52 KB / 78.37 KB gz; no UI logic moved).
- `make test-client` 6 files / 10 tests green twice in a row from a
  cold vitest spawn (10 tests after this goal vs 9 before — the new
  shape lock-in adds one).
- Dev stack on `:8080` / `:1430` still 200 / 200.

Files modified:

- `clients/hackline-ts/src/types.ts` — `Device` rename + retype, doc
  comment naming the deferred `class` / `online` removal.
- `ui/hackline-ui/src/modules/devices/DeviceDetailPage.tsx` —
  `device.last_seen_ts` → `device.last_seen_at`.
- `ui/hackline-ui/src/modules/devices/DevicesPage.tsx` —
  `d.last_seen_ts` → `d.last_seen_at`.
- `clients/hackline-ts/test/devices.test.ts` — new
  `Device wire shape: last_seen_at is number|null, never a string`
  lock-in test.

Files added: none.

Working tree intentionally dirty for operator review (no commit, no
push, per goal brief).

## What I deferred and why

Drift surfaces I confirmed against `DOCS/openapi.yaml` while doing
the `Device` audit. Each is its own goal because each requires more
than a rename:

- **`Device.class` and `Device.online` are not on the wire.** The
  Rust `Device` row doesn't serialise either; openapi doesn't list
  either. UI consumers read both. Removing them needs a per-row
  health fetch (for `online`) and a schema decision (for `class`).
- **`Device.org_id` is on the wire but absent from the TS type.**
  The server includes it; openapi does not. Either openapi should
  add it (it's useful for multi-org scoping in the UI) or the
  server should drop it from this projection. Not a TS-only fix.
- **`Tunnel.created_at: string` should be `number`.** Server
  returns `i64`; openapi §Tunnel says `integer, format: int64`. UI
  uses `relTime(t.created_at)` which already accepts both shapes.
- **`Tunnel.enabled: boolean` is missing from the TS type.** Openapi
  marks it required; the legacy TS `Tunnel` interface omits it.
- **`User` shape is widely divergent.** TS has
  `name / role / device_scope / tunnel_scope / expires_at: string |
  null / created_at: string`; openapi §User requires
  `id / label / scope / customer_id / created_at: integer /
  last_used_at: integer | null`. Different field *names*, different
  types — this is more than a goal-17-shaped sweep.
- **`AuditEntry` shape is divergent.** TS has
  `ts: string / actor: string / action / target / detail`; openapi
  §AuditEntry requires `id / at: integer / actor_user_id: integer |
  null / action / subject / detail`. Same kind of rename + retype
  sweep as goal 17 but on a different object.
- **`MintedToken.expires_at: string | null`** — openapi consistently
  uses `integer | null` for timestamps; this is the same `_at:
  string` retype family.

None of these are goal-17 because none are pure rename + retype
inside a single object that already ships under a single
"hand-written client lies about wire shape" frame. They're better
done one object at a time so each diff stays grepable.

## What's next (goal 18 candidates)

- **Reconcile `User` shape on TS client + UI** — same kind of sweep
  as this goal but on the auth surface. UsersPage reads `u.name`,
  `u.role`, `u.expires_at`; openapi says `label`, `scope`,
  `last_used_at`. Bigger diff than goal 17 but mechanical.
- **Reconcile `Tunnel` shape** — add `enabled`, retype
  `created_at`. Smallest of the deferred objects.
- **Reconcile `AuditEntry` shape** — TS has wrong field names
  (`ts`/`actor`/`target` vs openapi `at`/`actor_user_id`/`subject`).
  Touches AuditPage rendering.
- **Replace `Device.online` and `Device.class` with a real source**
  — needs a per-device health fan-out or a list endpoint that
  returns health. Architectural, not just typing.
- **V002 audit-FK migration** (still pending from goal 16's deferred
  list).
- **Synchronous `liveliness::Get` probe** so `DeviceHealth.rtt_ms`
  is non-null (still pending from goal 16).
- **SSE integration test in `@hackline/client`** (pending goal-15
  `wire.ts::Event` vs `types.ts::GatewayEvent` reconciliation).
- **Wire `make test-client` into CI.**
