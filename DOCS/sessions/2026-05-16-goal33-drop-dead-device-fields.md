# 2026-05-16 — Goal 33: drop dead `Device.class` / `Device.online` from the wire-facing TS surface

The TS `Device` type carries `class: DeviceClass` and `online:
boolean` fields that have not been on the wire since goal 17.
The comment above the interface in `clients/hackline-ts/src/
types.ts` already flags this as a deferred follow-up. Goal 32
made it actionable: `DeviceDetailPage.tsx` gated the
`getDeviceInfo` call on `d.class === "linux"`, so the freshly-
implemented endpoint never fires (class is always `undefined`).
Goals 30/31 already replaced `device.online` reads with
`health.online`; the `online` field is also pure dead weight on
the type.

## Plan

| # | Step | Status |
|---|---|---|
| 1 | Drop `DeviceClass` type alias and the `class` / `online` fields from `Device` in `clients/hackline-ts/src/types.ts`. Keep the `kind: "device.online"` event variant — that's an event discriminator string, not the dead field. | [x] |
| 2 | `DeviceDetailPage.tsx`: drop the `d.class === "linux"` gate around `getDeviceInfo` so the call always fires (already `.catch(() => {})`); drop the `class` row from the Health card; drop the now-unreachable `device.class === "constrained"` branch from the Agent info card. Update the surrounding comment so it no longer references the removed gate. | [x] |
| 3 | `DevicesPage.tsx`: drop the "Class" `<th>` header and the `<td>{d.class}</td>` cell. | [x] |
| 4 | Verify gates: `pnpm -C clients/hackline-ts build`, `pnpm -C ui/hackline-ui typecheck` + `build`, `make test-client` green twice, `cargo check --workspace`. | [x] |

## Design

**Why now and not as part of goal 32.** Goal 32 was already
M-sized across four crates; folding in a TS-type / UI cleanup
would have buried the wire-shape decision under churn. With
the info endpoint actually shipping, this cleanup is the
minimum needed to make it visible in the UI for every device,
not just hypothetically-`linux` ones.

**Why drop the `constrained` branch instead of preserving it
behind a different signal.** The "constrained" label was a
device-class hint with no enforcement — a non-agent device
just times out on `getDeviceInfo` and the catch swallows it.
The card already says "live query pending…" while waiting;
that's the right UX for a device that doesn't answer info
(it might be a `constrained`-style device, an offline agent,
or a misconfigured one — the UI doesn't need to guess).

**Why keep `kind: "device.online"` event variant.** It's the
string name of a server-sent event, not a property of
`Device`. Renaming server-sent event names is a wire change
that goes through SSE reconciliation (deferred goal-15). The
event still fires; the discriminator string is independent of
the dead `Device.online` field.

**Why not also delete the "Class" UI column without
operator input.** It *is* gone — but only because the field
is unambiguously absent from the openapi `Device` schema and
from the gateway DB row. If operators wanted to bring `class`
back as a real wire field, that's an openapi + DB migration
discussion, not a UI undo. Removing the dead cell is the
honest representation of what's actually queryable today.

## Outcome

- `pnpm -C clients/hackline-ts build`: clean.
- `pnpm -C ui/hackline-ui typecheck` + `build`: clean
  (bundle 258.90 KB / 78.50 KB gz — marginally smaller).
- `make test-client`: 6 files / 13 tests green twice.
- `cargo check --workspace`: clean (only the pre-existing
  `hackline-agent::error::AgentError::PortDenied` dead-code
  warning).

## What I deferred and why

- **Surfacing "agent unreachable" distinctly from "loading"
  in the Agent info card.** Worth a follow-up; today the
  card shows "live query pending…" until either info
  resolves or the user leaves. Distinguishing 503/504/timeout
  responses gives operators actionable information but
  requires plumbing the failure shape through the `.catch`,
  which is a separate UX decision.
- **Removing `DeviceClass` references from openapi `enum`
  examples.** The openapi `Device` schema doesn't list
  `class`, so there's nothing to remove there; if the enum
  appears in docs prose, that's outside the source of
  truth this goal owns.

## What's next (goal 34 candidates)

- **Stand up first GitHub Actions workflow** for
  `make test-client` + Rust gates + UI gates.
- **Surface info-endpoint errors in `DeviceDetailPage`'s
  Agent info card** (503/504/decode → distinct copy).
- **SSE integration test in `@hackline/client`** (blocked
  on goal-15 reconciliation).
- **Operator decisions** (`User`, `CmdOutboxRow`).
