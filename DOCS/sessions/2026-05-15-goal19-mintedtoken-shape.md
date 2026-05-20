# 2026-05-15 — Goal 19: drop fabricated `MintedToken.expires_at`

Goal 18 deferred this as the smallest of the remaining wire-shape
mismatches. The `clients/hackline-ts` `MintedToken` type carried a
`expires_at: string | null` field that the gateway has never
returned: the handler in
`crates/hackline-gateway/src/api/users/mint_token.rs` serialises
`MintTokenResponse { token: String }` only, and `DOCS/openapi.yaml`
§TokenMinted lists `[token]` as the sole field.

This is an openapi-mandated removal (the type was hallucinated),
not a feature deletion — no producer ever emits the field, and
the only UI consumer (`UsersPage.tsx`) reads `minted.token` only.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Verify the wire: `mint_token.rs` returns `{ token }` only; openapi `TokenMinted` requires `[token]`; UI `UsersPage` reads `minted.token` only. | [x] |
| 1 | `clients/hackline-ts/src/types.ts`: drop `MintedToken.expires_at`; comment names the prior fabrication. | [x] |
| 2 | `clients/hackline-ts/test/users.test.ts`: add a wire-shape lock-in test that mints a token via `POST /v1/users/:id/tokens` and asserts the response keys are exactly `["token"]` — no `expires_at`. | [x] |
| 3 | Verify: `cargo check --workspace` (only pre-existing `hackline-agent PortDenied` warning), `cargo test --workspace` green, `pnpm -C clients/hackline-ts build` clean, `pnpm -C ui/hackline-ui typecheck` + `build` clean, `make test-client` green twice in a row, dev stack `:8080` / `:1430` still 200/200. | [x] |

## Design

**Why this counts as openapi-mandated removal.** The contract is the
schema. A TS field that no producer fills and that openapi doesn't
list is documentation drift, not a feature; removing it brings the
type into compliance. This is the same reasoning used in goal 18
when narrowing `TunnelKind` from `["http","tcp","ssh"]` to
`["http","tcp"]`.

**Why the lock-in asserts an exact key set.** Unlike `Device` or
`Tunnel`, where the server may legitimately add forward-compatible
fields, `TokenMinted` is a fixed contract — the bearer token is the
only piece the caller needs and the only piece openapi documents.
`Object.keys(raw).sort()` equality is the right shape of test for
"this object is closed". If the server later grows the response
(say, to return `expires_at` for real), this test will fail loudly
and the type+test get updated together.

**Why `createUser` is still cast to `unknown` in the existing test.**
The wider `User` shape sweep (TS `name`/`role`/`device_scope`/
`tunnel_scope`/`expires_at: string|null`/`created_at: string` vs
openapi `label`/`scope`/`customer_id`/`created_at: integer`/
`last_used_at: integer|null`) is its own deferred goal — different
field *names* plus type changes, plus UI rename ripple in
`UsersPage`. Out of scope here.

## Outcome

- `cargo check --workspace` clean except the pre-existing
  `hackline-agent PortDenied` dead-code warning (CLAUDE.md rule 3).
- `cargo test --workspace` all green.
- `pnpm -C clients/hackline-ts build` clean.
- `pnpm -C ui/hackline-ui typecheck` + `build` clean (bundle
  unchanged at 258.52 KB / 78.37 KB gz; no UI logic moved).
- `make test-client` 6 files / 12 tests green twice in a row from a
  cold vitest spawn (12 tests this goal vs 11 after goal 18 — the
  new mintToken shape lock-in adds one).
- Dev stack on `:8080` / `:1430` still 200 / 200.

Files modified:

- `clients/hackline-ts/src/types.ts` — `MintedToken` drops
  `expires_at`; comment naming the prior fabrication.
- `clients/hackline-ts/test/users.test.ts` — new
  `mintToken returns { token } only — no fabricated expires_at`
  lock-in test.

Files added: none.

Working tree intentionally dirty for operator review.

## What I deferred and why

Same list as goal 18, minus `MintedToken`:

- **`Device.class` and `Device.online` are not on the wire.** UI
  consumers read both. Removing them needs a per-row health fetch
  and a schema decision.
- **`Device.org_id` is on the wire but absent from the TS type.**
  Schema decision required.
- **`User` shape is widely divergent** — different field names
  (`name`/`role`/`device_scope`/`tunnel_scope` vs openapi
  `label`/`scope`) and types. Touches `UsersPage`. The biggest of
  the remaining drifts.
- **`AuditEntry` shape is divergent** — TS `ts`/`actor`/`target`
  vs openapi `at`/`actor_user_id`/`subject`. Touches `AuditPage`.
- **V002 audit-FK migration** (still pending from goal 16).
- **Synchronous `liveliness::Get` probe** so `DeviceHealth.rtt_ms`
  is non-null (still pending from goal 16).
- **SSE integration test in `@hackline/client`** (pending goal-15
  `wire.ts::Event` vs `types.ts::GatewayEvent` reconciliation).
- **Wire `make test-client` into CI.**

## What's next (goal 20 candidates)

- **`AuditEntry` shape sweep** — mechanical rename family the same
  size as goals 17 + 18; touches `AuditPage`. Smaller than `User`.
- **`User` shape sweep** — biggest remaining type-only drift.
- **Replace `Device.online` and `Device.class` with a real source**
  — needs a per-device health fan-out or list-with-health endpoint.
- **V002 audit-FK migration** — close the latent FK pinning.
- **Synchronous liveliness probe** — `DeviceHealth.rtt_ms`.
