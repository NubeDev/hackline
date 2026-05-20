# 2026-05-15 — Goal 11: Rust→TS codegen for hackline-proto (Phase 5)

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Add `specta` + `specta-typescript` to workspace deps; gate behind a `specta` feature on `hackline-proto` so `serde`-only consumers don't pay for it | [x] |
| 1 | Derive `specta::Type` (under the feature) on the public wire types whose fields are statically typed: `Zid`, `LogLevel`, `CmdResult`, `CmdAck`, `ConnectRequest`, `ConnectAck`, `Event`, `AgentInfo` | [x] |
| 2 | Add `tests/specta_snapshot.rs`: collects each type, exports TypeScript, compares to `tests/wire.ts.snap`. `SPECTA_UPDATE=1` regenerates | [x] |
| 3 | Add `examples/wire_ts.rs`: emits the same TypeScript with a generated-by banner to the default path `clients/hackline-ts/src/wire.ts` | [x] |
| 4 | First-run snapshot capture and committed `wire.ts` | [x] |
| 5 | `cargo check -p hackline-proto` (no features) and `--features specta`; `cargo test --workspace` | [x] |

## Design

**Why specta and not ts-rs.** Specta is what the rest of the workspace
already uses (`codeless-types`, `codeless-rpc`). Same toolchain across
projects means one mental model and one set of escape hatches when a
type doesn't render the way we want. ts-rs would also work but would
add a second pattern.

**Feature-gated, not unconditional.** `hackline-proto` is on the
mobile-safe spine — anything compiled into the device-side `agent`
binary, the desktop CLI, and (later) the iOS shell. Pulling
`specta` and its derive macro into every one of those builds for a
codegen-only purpose is wasted compile time and binary size. The
`specta` feature is opt-in; tests, the example binary, and the npm
package build all enable it. Default consumers see no change.

**Why one snapshot in `hackline-proto` and not split across crates.**
The codeless workspace splits its snapshot in two (`codeless-types`
+ `codeless-rpc`) because of a dev-dependency cycle. Hackline has no
such cycle — every wire type lives in `hackline-proto`. One
snapshot file. If `hackline-gateway` later grows REST-shaped types
that need to ship to the npm package (`/v1/devices` JSON shapes,
SSE event payloads), they earn a second snapshot in their own crate,
and the example binary stitches both into one `.ts` the same way
`wire_ts.rs` does in codeless.

**Output path: `clients/hackline-ts/src/wire.ts`.** The npm package
itself (Phase 5 second half) lands at `clients/hackline-ts/`. The
generated `wire.ts` is committed so consumers don't need a Rust
toolchain to type-check; the snapshot test guards against drift the
same way `mani run wire-ts-check` does in codeless. The actual npm
scaffolding (package.json, the Zenoh-WS transport client) is
deferred to goal 12 — this goal is just the contract surface.

**Snapshot-only validation in v1.** No `wire-ts-check` mani task yet
because the workspace `mani.yaml` only knows about the codeless
project. When hackline gets its own mani entry (or codeless-workspace
grows a multi-project task layer), a second `wire-ts-check` rule
will live there.

## Outcome

The Rust→TS wire bridge ships for the connection-lifecycle and
event surfaces. With `--features specta` enabled:

- `cargo test -p hackline-proto --features specta --test specta_snapshot`
  enforces the wire shape against `tests/wire.ts.snap`. Drift is a
  test failure with a clear `SPECTA_UPDATE=1` regenerate hint.
- `cargo run -p hackline-proto --features specta --example wire_ts`
  emits `clients/hackline-ts/src/wire.ts`, the file the future
  `@hackline/client` npm package imports. Both files are
  byte-identical except for the leading banner.

Default builds (agent, gateway, CLI, mobile-safe spine) see no
change: the `specta` dep stays optional, the derives are gated by
`#[cfg_attr(feature = "specta", derive(specta::Type))]`, and the
snapshot test + example carry `required-features = ["specta"]` so a
plain `cargo test --workspace` doesn't try to build them.

### Scope cut: payload-bearing envelope types deferred

`MsgEnvelope`, `CmdEnvelope`, `ApiRequest`, `ApiReply` all carry an
opaque `serde_json::Value` payload. Specta's `serde_json` feature
impls `Type for Value` as a structural enum that recurses through
`Vec<Value>`; the typescript exporter then expands the type inline
until the stack overflows. The clean fix is a `Value -> unknown`
shim with a manual `Type` impl, but `specta-typescript` 0.0.10 has
no `unknown`/`any` primitive, so a real fix needs either an
intermediate "named-and-opaque" type pattern or a custom emitter
hook. That work belongs alongside the npm-client wiring (next goal)
rather than dragging this PR sideways.

The four envelope types still derive Serde unchanged — zero wire
behaviour difference. They simply don't appear in `wire.ts` yet.

Verified:
- `cargo check -p hackline-proto` — clean
- `cargo check -p hackline-proto --features specta` — clean
- `cargo test -p hackline-proto --features specta` — 8 tests pass
  (7 existing unit + 1 snapshot)
- `cargo test --workspace` — all suites pass; only the two
  pre-existing `hackline-agent` dead-code warnings.

## What's next

- `Value -> unknown` shim so `MsgEnvelope` / `CmdEnvelope` /
  `ApiRequest` / `ApiReply` join the snapshot.
- `clients/hackline-ts/` npm scaffold (`package.json`, tsconfig,
  `transport.ts` over Zenoh-WS) using the generated `wire.ts`.
- Postgres backend behind a SQL repository trait (SCOPE.md
  Phase 5).
