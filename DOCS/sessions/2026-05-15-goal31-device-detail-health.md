# 2026-05-15 — Goal 31: `DeviceDetailPage` health polling + dead `device.online` cleanup

`DeviceDetailPage.tsx` reads `device.online` for the header
badge and as a fallback in the Health card row. Same dead field
as `DevicesPage` (goal 30): `online` isn't on the wire `Device`
since goal 17, so the header always renders "offline" and the
Row falls through to `String(undefined)`.

The page already calls `getDeviceHealth(id)` once on mount,
which is the right shape for an interactive page. But there's
no refresh loop, so the badge freezes on the first response
and stays stale until the user navigates away and back. Same
fix as goal 30: a 5 s poll, separated from the
once-per-mount data (`getDevice`, `listTunnels`, `getDeviceInfo`)
so the poll is health-only and cheap.

While here: replace `device.online` (dead) with the
`health.online` signal in the header badge, with the same
"loading ⇒ neutral —" semantics as the list page. Drop the
`?? device.online` fallback inside the Health card row for
the same reason.

`device.class === "linux"` / `"constrained"` is also broken
(no producer for `class` either) — left alone, see deferred
list. The agent-info branch already renders "live query
pending…" indefinitely when `class` is undefined, which is
the existing-bug status quo; fixing it needs a schema
decision.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Confirm the surface: `DeviceDetailPage` already has `health` state but no refresh; the existing `useEffect` is structured as one async block that resolves device + tunnels then fires-and-forgets health + info; `DeviceHealth` is re-exported from `@/lib/api`. | [x] |
| 1 | Refactor the effect: keep the once-on-mount load for `device`/`tunnels`/`info`; add a separate `setInterval` (5 s) that re-fetches `getDeviceHealth(id)` only. Both honour the `cancelled` flag. First health fetch fires immediately, then on the interval. | [x] |
| 2 | Header badge: drive from `health?.online`. Loading state renders a neutral `outline` badge with `—`, matching `DevicesPage`. Drop the `device.online` read. | [x] |
| 3 | Health card row: drop the `?? device.online` fallback in the `online` row (read directly from `health`, render `—` when health is null); same for `last_seen_at` row (read directly from `health`). The `device.last_seen_at` field *is* on the wire and would work, but mixing two sources for fields that are both meant to come from the same probe creates a UI lie where the dot says one thing and the last-seen says another mid-refresh. Pin everything to `health` so the card is atomically consistent. | [x] |
| 4 | Verify gates: `pnpm -C ui/hackline-ui typecheck` + `build`, `make test-client` green twice, `cargo check --workspace`. | [x] |

## Design

**Why pin the entire Health card to `health`, not mix sources.**
The card has four rows: `online`, `last seen`, `rtt`, `class`.
The first three all come from the same probe — they're
internally consistent because they describe one moment in
time. Mixing in `device.last_seen_at` as a fallback creates a
visual race: after a refresh, `online` updates but
`last_seen_at` lags behind by one tick. Showing `—` for ~250 ms
during the first probe is honest; showing wrong-but-stale data
forever is not. `class` stays from `device` because it's a
property of the row, not a probe result.

**Why 5 s, same as `DevicesPage`.** Consistency. The user
switching between the list and the detail page expects the
freshness to feel the same. The per-row RTT cache (goal 27,
1 s TTL) means each refresh is at most one Zenoh query per
the device being viewed; on the detail page that's a single
device, so cost is negligible.

**Why two `useEffect`s would have been wrong.** The poll
depends on `id` and the cleanup needs the same `cancelled`
flag. Splitting into two effects would mean two cancellation
flags and two interval setups; the existing single-effect
shape is the right one to extend.

**Why no `device.last_seen_at` fallback even on first paint.**
The trade-off is "show stale DB value for ~250 ms vs show `—`
for ~250 ms". The DB value can be hours old; the placeholder
honestly says "loading". On a fresh navigation the page
renders the device card (label, zid, tunnels) immediately and
fills in the health card when the probe returns — the
asymmetry is fine because the health card is documented as
"current liveliness", not "last known state".

**Why not use SSE.** Same reasoning as goal 30: liveliness
state isn't on the event bus today. Polling is the right
shape until that lands.

**Why no test.** UI package has no component-test harness
(vitest is configured for `clients/hackline-ts/` only). The
data path is pinned by the `getDeviceHealth` and
`getDevicesHealth` tests in `clients/hackline-ts/test/`.

## Outcome

- `pnpm -C ui/hackline-ui typecheck` clean.
- `pnpm -C ui/hackline-ui build` clean. UI bundle 259.16 KB →
  259.31 KB (gz 78.55 → 78.57); +0.15 KB raw / +0.02 KB gz
  for the poll loop and the new conditional render.
- `make test-client` 6 files / 13 tests green twice.
- `cargo check --workspace` clean (only tolerated
  `hackline-agent PortDenied` warning).

Files modified:

- `ui/hackline-ui/src/modules/devices/DeviceDetailPage.tsx` —
  health poll, header badge wired to `health.online` with
  neutral loading state, Health card pinned entirely to
  `health` (no `device.online`/`device.last_seen_at`
  fallbacks). The `class` row is unchanged — still sourced
  from `device` because it's a row property, and the
  underlying "`class` not on the wire" issue is the
  operator-decision deferred item.

Files added: none.

Working tree intentionally dirty for operator review.

## What I deferred and why

- **`device.class` consumer.** `class` has no producer on the
  wire either. The agent-info "live query pending…" branch is
  the existing-bug status quo; the right fix is a schema
  decision (already on the operator's deferred list).
- **Push-based liveliness.** Needs an event-bus variant; out
  of scope.

## What's next (goal 32 candidates)

- **Stand up first GitHub Actions workflow** for
  `make test-client` + Rust + UI gates.
- **SSE integration test in `@hackline/client`** (depends on
  goal-15 reconciliation).
- **Operator decisions** (`User`, `CmdOutboxRow`,
  `Device.class`/`online`) — not picked autonomously.
- **Hover tooltip with full RTT history sparkline** — needs
  a server-side history surface that doesn't exist yet.
