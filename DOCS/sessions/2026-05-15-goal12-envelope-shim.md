# 2026-05-15 — Goal 12: Value→unknown shim for envelope wire types (Phase 5)

Picks up the deferred slice from goal 11. The four envelope types
(`MsgEnvelope`, `CmdEnvelope`, `ApiRequest`, `ApiReply`) carry
`serde_json::Value` payloads and therefore did not appear in
`wire.ts`. Goal 12 lands the smallest viable shim so they do.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Make `specta-typescript` an optional dep on `hackline-proto` (gated by the existing `specta` feature) so the `#[specta(type = ...)]` override path can name `specta_typescript::Unknown` without dragging the crate into default builds | [x] |
| 1 | Derive `specta::Type` on `MsgEnvelope`, `CmdEnvelope`, `ApiRequest`, `ApiReply`. Mark each `payload: serde_json::Value` field `#[cfg_attr(feature = "specta", specta(type = specta_typescript::Unknown))]` so it renders as TS `unknown` instead of recursing through specta's Value impl | [x] |
| 2 | Register the four envelope types in `collect()` in both `tests/specta_snapshot.rs` and `examples/wire_ts.rs`; update the example banner to drop the "envelopes are deferred" disclaimer | [x] |
| 3 | Regenerate `tests/wire.ts.snap` and `clients/hackline-ts/src/wire.ts` (one with `SPECTA_UPDATE=1`, one with `cargo run --example wire_ts`) | [x] |
| 4 | `cargo check -p hackline-proto` (no features), `--features specta`, `cargo test --workspace`, `cargo test -p hackline-proto --features specta` — all clean | [x] |

## Design

**Why `#[specta(type = …)]` and not a wrapper newtype.** The cleanest
fix at the type-system level would be a `JsonValue(serde_json::Value)`
newtype with a manual `Type` impl. But that touches every call site
that currently constructs an envelope (`MsgEnvelope::new_event`,
`ApiReply::json`, the `headers` map, the `cmd_round_trip` test, every
gateway and agent code path that builds payloads). The
`#[specta(type = X)]` override is exactly the escape hatch the specta
authors built for this case: it changes only the schema, not the Rust
type, so wire bytes and call sites stay byte-identical.

**Why `specta_typescript::Unknown` and not `define("unknown")`.**
`define` returns a `Reference`, not a `DataType`, and the `type =`
attribute expects a type path that itself implements `Type`.
`specta_typescript::Unknown<T = ()>` has a `Type` impl that emits
`Reference::Opaque(opaque::Unknown)` which the TS exporter renders as
`unknown`. It is the documented public API for "this field is opaque
to the TS contract."

**Optional dep, not workspace-wide.** `specta-typescript` already
exists as a dev-dep on `hackline-proto` for the snapshot/example
targets. Promoting it to an optional regular dep gated by the
`specta` feature keeps default builds (mobile-safe spine, agent,
gateway) free of both `specta` and `specta-typescript`. Only crates
that explicitly opt into the `specta` feature pay for either.

**Snapshot scope.** The same registration set that the example uses
goes into `tests/specta_snapshot.rs` so drift in either direction
(adding a Value-bearing type without the override, or changing the
override target) trips the snapshot test.

**Out of scope.**

- The npm package scaffold itself (`clients/hackline-ts/package.json`,
  tsconfig, `transport.ts` over Zenoh-WS) is the next goal. This goal
  finishes the contract surface.
- A bigint-tolerant export. Same reasoning as goal 11: every i64 we
  ship is unix-millis, well inside `Number.MAX_SAFE_INTEGER`.
- A `wire-ts-check` mani task. Hackline still has no entry in the
  workspace `mani.yaml`; that lands when codeless-workspace grows a
  multi-project task layer.

## Outcome

The wire-contract surface is complete. Every public hackline-proto
type now appears in `wire.ts`. The four envelope types render with
their `payload` field as `unknown`, which is the correct TS shape
for an arbitrary-JSON blob: consumers must narrow before reading,
and the contract no longer pretends to know payload schema.

Key properties preserved:

- Default builds (mobile-safe spine, agent, gateway, CLI) still see
  no `specta` or `specta-typescript` in their dependency graph. The
  override and the dep are both gated behind the existing `specta`
  feature.
- Wire bytes unchanged. `Value` is still `Value` in Rust; the
  `#[specta(type = ...)]` attribute affects schema generation only,
  not serde.
- Snapshot test still authoritative. Drift in either direction —
  a new Value-bearing field added without the override, or the
  override target changed — fails the test with a clear
  `SPECTA_UPDATE=1` regenerate hint.

Verified:
- `cargo check -p hackline-proto` — clean.
- `cargo check -p hackline-proto --features specta` — clean.
- `cargo test --workspace` — all suites pass.
- `cargo test -p hackline-proto --features specta` — 8 tests pass
  (7 existing unit + the snapshot).
- Generated `clients/hackline-ts/src/wire.ts` byte-identical to the
  snapshot body modulo banner.

## What's next

- `clients/hackline-ts/` npm scaffold: `package.json`, `tsconfig.json`,
  `transport.ts` over Zenoh-WS. The wire surface is now stable enough
  to start writing the typed client around it.
- Postgres backend behind a SQL repository trait (SCOPE.md Phase 5).
- Hackline entry in the workspace `mani.yaml` so a `wire-ts-check`
  task can guard drift in CI alongside the codeless equivalent.
