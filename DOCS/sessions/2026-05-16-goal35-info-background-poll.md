# 2026-05-16 — Goal 35: background-poll the info endpoint on `DeviceDetailPage`

Goals 32-34 made `getDeviceInfo` real and surfaced its failures.
Today it still fires once on mount. The case it doesn't cover:
an operator sits on a device's detail page while the agent
restarts (rolling upgrade, config push, etc.). They see the
pre-restart `version` and an `uptime` that's now wrong, with no
indication a refresh is needed.

A long-cadence poll (much slower than health's 5 s) closes that
gap. Info is generated synchronously from in-memory state on
the agent (zero I/O), so the Zenoh cost is negligible. 30 s is
slow enough that an idle detail page costs ~2 queries/min and
fast enough that a rolling upgrade visibly converges.

## Plan

| # | Step | Status |
|---|---|---|
| 1 | Add `INFO_POLL_MS = 30_000` constant alongside `HEALTH_POLL_MS`. Hoist the info-fetch into a `pollInfo()` closure that sets `info` / `infoErr` exactly as the one-shot does today (so success after failure clears `infoErr`, and vice versa). Drop the one-shot inline call. | [x] |
| 2 | Drive the first tick from the async `(async () => { … })()` block (matches `pollHealth()` first-tick pattern), and schedule subsequent ticks via `setInterval`. Clean up in the existing teardown alongside the health interval. | [x] |
| 3 | Gates: `pnpm -C ui/hackline-ui typecheck` + `build`, `pnpm -C clients/hackline-ts build`, `make test-client` green twice. (No Rust / TS-client changes.) | [x] |

## Design

**Why 30 s, not 5 s like health.** Health is the only signal
that says "the device is alive *right now*", and stale-online
data is actively wrong (the SCOPE.md liveliness window is
60 s). Info is identity + policy + uptime — a 30 s lag on
version is operator-trivial because they're looking at a card
that already reads "uptime: 0 min" when a restart happens.

**Why not start with one cadence and split later.** Same call
sites today, but they answer different questions and have
different acceptable lag. Mixing them at 5 s doubles the
Zenoh query rate per detail page without operator benefit;
splitting them at 30 s costs one extra constant.

**Why setInterval, not setTimeout-chain.** Matches the
existing `HEALTH_POLL_MS` pattern; the `cancelled` flag
already protects against overlapping responses arriving
after unmount. setInterval-vs-chain only matters when the
request can outlive the interval, which the `infoErr` path
guarantees doesn't happen (errors resolve before the timeout
on the gateway side: 1 s + 250 ms buffer < 30 s).

**Why clear `infoErr` on success and `info` on
failure.** The card derives its content with `info ? … :
infoErr ? … : pending`; without clearing the stale field
the card could show old info + a fresh error simultaneously,
which lies about the current state. Symmetric clearing makes
the displayed state a true snapshot of the last poll.

## Outcome

- `pnpm -C ui/hackline-ui typecheck` + `build`: clean
  (bundle 259.37 KB / 78.67 KB gz — marginally larger).
- `pnpm -C clients/hackline-ts build`: clean.
- `make test-client`: 6 files / 13 tests green twice.

## What's next (goal 36 candidates)

- **First GitHub Actions workflow** for Rust + UI + client
  gates (note: this checkout can't push, so the workflow
  goes in but isn't exercised from here).
- **Operator decisions** (`User`, `CmdOutboxRow`).
- **Configurable `INFO_POLL_MS`** via the settings page if
  operators want it faster or slower per their topology.
