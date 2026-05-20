# 2026-05-15 — Goal 18: reconcile `Tunnel` TS shape with openapi

Goal 17 deferred this as the smallest of the remaining wire-shape
mismatches. The `clients/hackline-ts` `Tunnel` interface had three
lies vs `DOCS/openapi.yaml` §Tunnel and the gateway row in
`crates/hackline-gateway/src/db/tunnels.rs::Tunnel`:

1. `created_at: string` — wire is `integer` (unix epoch seconds, int64).
2. `enabled: boolean` was missing — openapi marks it required, the
   gateway has always serialised it.
3. `TunnelKind` included `"ssh"` — openapi enum is `[tcp, http]`,
   no Rust producer or TS consumer ever referenced `"ssh"`.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Verify the wire: gateway `Tunnel` struct exposes `id, device_id, kind, local_port, public_hostname, public_port, enabled, created_at: i64` (all `Serialize`). Live `:8080` returns 200. | [x] |
| 1 | `clients/hackline-ts/src/types.ts`: `Tunnel.created_at: string` → `number`; add required `enabled: boolean`; `TunnelKind` drop `"ssh"`. | [x] |
| 2 | `clients/hackline-ts/test/tunnels.test.ts`: add a wire-shape lock-in test that creates a tunnel, lists it, and asserts `created_at` is `number`, `enabled` is `boolean`, `kind ∈ {http,tcp}`, plus typeof checks for the other documented fields. | [x] |
| 3 | Verify: `cargo check --workspace` (only pre-existing `hackline-agent PortDenied` warning), `cargo test --workspace` green, `pnpm -C clients/hackline-ts build` clean, `pnpm -C ui/hackline-ui typecheck` + `build` clean, `make test-client` green twice in a row, dev stack `:8080` / `:1430` still 200/200. | [x] |

## Design

**Why removing `"ssh"` from `TunnelKind` counts as openapi-mandated,
not a feature deletion.** The openapi enum is the contract; an
extra TS variant that no producer ever emits is a documentation
bug, not a feature. No call site (Rust, TS, or UI) constructs or
matches on `"ssh"`. The brief permits "rename or retype only — no
field additions/removals beyond what openapi mandates" — narrowing
the union to match the schema *is* the openapi-mandated change.

**Why the existing UI (TunnelsPage, DeviceDetailPage) needed no
edits.** Both consumers read `t.created_at` only through `relTime`,
which goal 16 widened to accept `number | string | null | undefined`.
Neither reads `t.enabled` yet — adding the field is a pure type
strengthening; no JSX touches required.

**Why the lock-in uses `Record<string, unknown>` rather than
`toEqual`.** Same rationale as goal 17: the server may grow new
optional fields, so we assert the goal-18 invariants point-by-point
(`created_at: number`, `enabled: boolean`, `kind ∈ enum`) instead
of pinning an exact snapshot that would force a test edit on every
forward-compatible addition.

**Why this goal does not also rename `TunnelWithZid` consumers.**
The `tunnels.list_active_tcp` projection is gateway-internal; not
exposed on the REST surface, not on openapi, no TS shadow type. It
stays.

## Outcome

- `cargo check --workspace` clean except the pre-existing
  `hackline-agent PortDenied` dead-code warning (CLAUDE.md rule 3).
- `cargo test --workspace` all green.
- `pnpm -C clients/hackline-ts build` clean.
- `pnpm -C ui/hackline-ui typecheck` + `build` clean (bundle
  unchanged at 258.52 KB / 78.37 KB gz; no UI logic moved).
- `make test-client` 6 files / 11 tests green twice in a row from a
  cold vitest spawn (11 tests this goal vs 10 after goal 17 — the
  new tunnel shape lock-in adds one).
- Dev stack on `:8080` / `:1430` still 200 / 200.

Files modified:

- `clients/hackline-ts/src/types.ts` — `Tunnel` retype + `enabled`,
  `TunnelKind` narrowed; comment naming the prior lies.
- `clients/hackline-ts/test/tunnels.test.ts` — new
  `Tunnel wire shape: created_at is number, enabled is boolean`
  lock-in test.

Files added: none.

Working tree intentionally dirty for operator review.

## What I deferred and why

The drift surfaces remaining from goal 17's deferred list, untouched
here because each is wider than a same-shape rename + retype:

- **`Device.class` and `Device.online` are not on the wire.** UI
  consumers read both. Removing them needs a per-row health fetch
  (for `online`) and a schema decision (for `class`).
- **`Device.org_id` is on the wire but absent from the TS type.**
  Either openapi adds it or the server drops it; not a TS-only fix.
- **`User` shape is widely divergent** — different field *names*
  (`name`/`role`/`device_scope`/`tunnel_scope` vs openapi
  `label`/`scope`) and types. Touches `UsersPage`.
- **`AuditEntry` shape is divergent** — TS `ts`/`actor`/`target`
  vs openapi `at`/`actor_user_id`/`subject`. Touches `AuditPage`.
- **`MintedToken.expires_at: string | null`** should be
  `integer | null`.
- **V002 audit-FK migration** (still pending from goal 16).
- **Synchronous `liveliness::Get` probe** so `DeviceHealth.rtt_ms`
  is non-null (still pending from goal 16).
- **SSE integration test in `@hackline/client`** (pending goal-15
  `wire.ts::Event` vs `types.ts::GatewayEvent` reconciliation).
- **Wire `make test-client` into CI.**

## What's next (goal 19 candidates)

- **`Tunnel.created_at` retype** — done here.
- **`User` shape sweep** — biggest of the remaining type-only
  drifts. The `name`/`role` rename is invasive enough to warrant
  its own goal, plus the auth UI (`UsersPage`) needs label tweaks.
- **`AuditEntry` shape sweep** — same kind of mechanical rename
  as goal 17/18 but on the audit object; touches `AuditPage`.
- **`MintedToken.expires_at` retype** — small, mechanical.
- **Replace `Device.online` and `Device.class` with a real source**
  — needs a per-device health fan-out or list-with-health endpoint.
- **V002 audit-FK migration** — close the latent FK pinning
  documented in goal 16.
