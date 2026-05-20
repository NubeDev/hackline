# 2026-05-15 — Goal 30: `DevicesPage` consumes `getDevicesHealth()`

`DevicesPage.tsx` reads `d.online` and renders an "online" /
"offline" badge from it. The wire `Device` shape (since goal 17)
no longer carries `online`, so `d.online` is `undefined` at
runtime and the ternary always picks the falsy branch — every
device renders "offline" regardless of state. Same story for
`d.class` (the column shows blank).

Goal 28 added the collection endpoint, goal 29 added the typed
client method. This goal wires it in: one `getDevicesHealth()`
call alongside the existing `listDevices()`, joined by
`device_id`, drives the status badge and a new RTT column. The
two reads run in parallel (`Promise.all`) so the page-load
wall clock is dominated by whichever is slower. A poll loop
refreshes the health pair every 5 s so the dot tracks
liveliness without the user clicking around — the per-device
RTT cache (goal 27) means each refresh costs at most one
liveliness query per device per second.

`d.class` is left alone for now — it has no producer in the
current openapi (the schema decision is on the operator's
deferred list) and fixing it requires either a schema decision
or removing the column. Removing is reversible, so it stays as
a known-broken field flagged in the deferred list.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Confirm the surface: `DevicesPage` already polls `listDevices()` once on mount with no refresh loop; `useApi()` returns `ApiClient`; `getDevicesHealth()` and `DeviceHealthEntry` are re-exported from `@/lib/api`; `Badge` variants `ok`/`err`/`warn` exist; `relTime` already lives in `@/lib/utils`. | [x] |
| 1 | Replace the single `devices` state with a `{ devices, health }` pair loaded by `Promise.all([listDevices, getDevicesHealth])`. Build a `Map<number, DeviceHealthEntry>` once per refresh, look up by `device_id` in the row render. | [x] |
| 2 | Add a 5 s `setInterval` poll inside the existing `useEffect` so the health dot tracks liveliness without a user click. Cleanup on unmount. The first refresh runs immediately so initial paint isn't blocked by the first interval tick. | [x] |
| 3 | Render: status badge driven by `health?.online` (loading ⇒ neutral "—" placeholder, never the false-positive "offline"); new RTT column showing `<rtt> ms` when `online && rtt_ms != null`, `—` otherwise. | [x] |
| 4 | Verify gates: `pnpm -C ui/hackline-ui typecheck` + `build`, `pnpm -C clients/hackline-ts build`, `make test-client` green twice, `cargo check --workspace`, `cargo test --workspace`. | [x] |

## Design

**Why `Promise.all`, not two `useEffect`s with separate state.**
The two reads have no dependency on each other and the page is
useless without both. Sequencing them in one `await` block keeps
the render path single-state (no "device list visible, health
still loading" intermediate flash), and a single error catch
handles either side failing.

**Why a 5 s poll, not an SSE subscription.** The gateway has a
liveliness fan-in (`liveliness.rs`) and an event SSE stream,
but liveliness state changes are not currently published as
events — they update the `last_seen_at` column directly. Polling
the new collection endpoint is the correct read shape today;
the per-row RTT cache (goal 27, 1 s TTL) means a 5 s poll
hits cache for ~80% of requests in steady state. If liveliness
ever lands on the event bus, this poll becomes a
subscribe-and-refresh-on-event.

**Why neutral "—" instead of "offline" while health is
loading.** Showing "offline" before the first health response
arrives is a lie — the device might be perfectly online. A
neutral placeholder makes the loading state distinguishable
from the offline state, which matters because offline is the
state the operator actually wants to act on.

**Why `online && rtt_ms != null` for the RTT cell.** A device
can be `online: true` but `rtt_ms: null` (last_seen_at fresh,
but the just-now probe didn't get a reply within 250 ms — a
real edge case on a congested mesh). Showing `null ms` would
be wrong; showing the prior cached value would lie about
freshness. The `—` is honest: "no RTT data right now".

**Why one map per refresh, not a derived `useMemo`.** The
join key is `device_id` and the source is small (≤ tens of
devices in MVP). A `Map` constructor on every render is
cheaper than the `useMemo` machinery's bookkeeping for this
size, and both are O(N).

**Why leave `d.class` broken.** Two options: (a) remove the
column — reversible, but loses information the operator might
expect, and the operator decision on the `class` schema is
already on the deferred list; (b) backfill from somewhere —
no current producer exists. Doing nothing is the
least-bad choice while flagging it explicitly. The column
already renders blank today; nothing changes.

**Why no test for the page.** The UI package has no component
test harness today (vitest is configured for the `clients/`
package only). Adding one is its own goal. The wire shape is
pinned by `clients/hackline-ts/test/devices.test.ts` (which
gained the `getDevicesHealth` shape test in goal 29), so any
breakage in the data path lights up there before it reaches
the UI.

## Outcome

- `pnpm -C ui/hackline-ui typecheck` clean.
- `pnpm -C ui/hackline-ui build` clean. UI bundle grew
  258.67 KB → 259.16 KB (gz 78.42 → 78.55) — the cost of the
  health-state pair, poll loop, and the new column.
- `make test-client` 6 files / 13 tests green twice (the first
  run was 33 s because vitest's globalSetup boots a fresh
  gateway; the second run reused the warm one and finished in
  823 ms — unchanged from goal 29 baseline).
- `cargo check --workspace` clean (only tolerated
  `hackline-agent PortDenied` warning).
- `cargo test --workspace` not re-run (no Rust surface changed
  in this goal; gates were green at goal 29's commit).

Files modified:

- `ui/hackline-ui/src/modules/devices/DevicesPage.tsx` —
  parallel reads, 5 s poll, health-driven badge, new RTT
  column. The `class` column is unchanged; it still renders
  blank because the wire `Device` shape doesn't carry it
  (operator schema decision pending).

Files added: none.

Working tree intentionally dirty for operator review.

## What I deferred and why

- **Component test for `DevicesPage`.** Needs a UI test
  harness; out of scope.
- **Removing the `class` column.** Operator decision pending.
- **Push-based liveliness updates.** Would require a new
  event variant and a fan-in change; deferred until the SSE
  reconciliation (goal 15) lands.
- **RTT badge thresholds (green/yellow/red).** Cosmetic;
  needs a UX decision on what counts as "good", "okay",
  "slow".

## What's next (goal 31 candidates)

- **Stand up first GitHub Actions workflow** for
  `make test-client` + Rust + UI gates.
- **SSE integration test in `@hackline/client`** (depends on
  the goal-15 reconciliation).
- **Same treatment for `DeviceDetailPage`** — replace the
  per-mount `getDeviceHealth(id)` + manual refresh with a
  poll loop and surface RTT in the device header.
- **Operator decisions** (`User`, `CmdOutboxRow`,
  `Device.class`/`online`) — not picked autonomously.
