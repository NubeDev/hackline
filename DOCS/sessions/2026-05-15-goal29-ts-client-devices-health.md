# 2026-05-15 — Goal 29: TS client method for `GET /v1/devices/health`

Goal 28 landed the collection-level health fan-out on the
gateway. The TS client doesn't speak it yet; without a typed
method, every UI consumer would either hand-roll `fetch` (losing
the package's bearer-token plumbing and `ApiError` shape) or
fall back to N parallel `getDeviceHealth(id)` calls (which
defeats the point of the fan-out endpoint).

This goal adds `getDevicesHealth(): Promise<DeviceHealthEntry[]>`
to `ApiClient` + `HttpApiClient`, with the matching wire types
in `types.ts` and a real-gateway integration test pinning the
shape.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Confirm the surface: `ApiClient` already has `getDeviceHealth(id)`; `types.ts::DeviceHealth` is the per-id shape; `test/devices.test.ts` already pins the per-id shape against a freshly-created offline device — direct template for the new test. | [x] |
| 1 | Add `DeviceHealthEntry` (per-row, with `device_id`) to `types.ts`. The page-envelope is unwrapped at the client boundary: callers receive `DeviceHealthEntry[]` to match the existing `listDevices` / `listTunnels` ergonomics, even though the wire is `{ items }`. | [x] |
| 2 | Add `getDevicesHealth(): Promise<DeviceHealthEntry[]>` to the `ApiClient` interface in `client.ts`. | [x] |
| 3 | Implement in `http-client.ts`: `GET /v1/devices/health`, unwrap `.items`. Order matters in the file (alphabetical-ish under `// ---- devices ----`) — slot it next to `getDeviceHealth`. | [x] |
| 4 | Extend `test/devices.test.ts`: a real-gateway test that creates two offline devices, calls `getDevicesHealth()`, and asserts the response includes both (by `device_id`) with `online: false`, `last_seen_at: null`, `rtt_ms: null`. | [x] |
| 5 | Verify gates: `pnpm -C clients/hackline-ts build`, `pnpm -C ui/hackline-ui typecheck`+`build`, `make test-client` green twice, `cargo check --workspace`, `cargo test --workspace`. | [x] |

## Design

**Why unwrap `{ items }` at the client boundary.** Every other
collection method on `ApiClient` returns the array directly
(`listDevices(): Promise<Device[]>`). Returning the raw envelope
here would force callers to write `(await c.getDevicesHealth()).items`
just for this one method — friction with no payoff, because
this endpoint is documented as non-paginated. If pagination is
added later, that's a breaking-shape change either way; the TS
package can update at the same time.

**Why a separate `DeviceHealthEntry` type, not reuse
`DeviceHealth`.** The per-id shape (`DeviceHealth`) has no
`device_id` field — it's keyed by the URL. The list shape needs
the id inside each row to be useful for joins. Renaming
`DeviceHealth` to add an optional `device_id` would lie about
the per-id endpoint's contract; two named types match the two
openapi schemas (`DeviceHealth` vs `DeviceHealthEntry`).

**Why a real-gateway test, not a unit test.** The package's
test policy (see `index.ts` and goal 14) is no-mock: tests run
against a loopback gateway booted by `globalSetup.ts`. The
existing `getDeviceHealth` test creates a device and asserts
the offline shape, which exercises the real handler end-to-end.
The new test follows the same pattern with two devices, which
is enough to prove the collection endpoint *and* the client's
envelope unwrap.

**Why two devices, not one.** A one-device test would pass even
if the implementation accidentally returned only the first
entry, or used a `find()` instead of returning all. Two
devices is the minimum that proves "this is a list, not a
short-circuit".

**Why filter the response by `device_id` rather than asserting
length.** The test creates devices with unique zids per run
(`uniqueZid()`) but the test gateway is reused across tests, so
the response will include leftover devices from earlier tests
that ran in the same vitest invocation. Asserting "the two ids
I just created are present with the offline shape" is
both sufficient and stable under test-suite reordering.

## Outcome

- `pnpm -C clients/hackline-ts build` clean.
- `pnpm -C ui/hackline-ui typecheck` + `build` clean. UI bundle
  grew from 258.59 KB → 258.67 KB (gz 78.40 → 78.42); the UI
  bundles `@hackline/client` so the new method ships in the
  bundle even before any UI consumer wires it up. Eight bytes
  gzipped is the cost of advertising the typed surface.
- `make test-client` 6 files / 13 tests green twice in a row
  (was 12 — the new collection-health test is the +1, exercises
  the real handler end-to-end against two freshly-created
  offline devices).
- `cargo check --workspace` clean (only tolerated
  `hackline-agent PortDenied` warning).
- `cargo test --workspace` green; gateway lib unit tests still
  33 (no Rust surface changed in this goal).

Files modified:

- `clients/hackline-ts/src/types.ts` — add
  `DeviceHealthEntry`.
- `clients/hackline-ts/src/client.ts` — add
  `getDevicesHealth()` to `ApiClient`.
- `clients/hackline-ts/src/http-client.ts` — implement
  against `GET /v1/devices/health`, unwrap `.items`.
- `clients/hackline-ts/test/devices.test.ts` — new
  two-device integration test.

Files added: none.

Working tree intentionally dirty for operator review.

## What I deferred and why

- **UI consumer.** Wiring this into `DevicesPage` to render the
  online dot and RTT badge per row is the natural follow-up,
  but UI work has its own design surface (loading state,
  refresh cadence, badge thresholds) and deserves its own
  goal.
- **Auto-generated TS from openapi.** Goal 11 set up the wire
  types as hand-written; the codegen migration is a separate
  larger goal called out in `SCOPE.md`.

## What's next (goal 30 candidates)

- **`DevicesPage` wires the new method** — show online dot +
  RTT badge per row.
- **Stand up a first GitHub Actions workflow** for
  `make test-client` + Rust + UI gates.
- **SSE integration test in `@hackline/client`** (depends on
  goal-15 reconciliation).
- **Operator decisions** (`User`, `CmdOutboxRow`, etc.) —
  not picked autonomously.
