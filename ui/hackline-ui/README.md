# hackline-ui

Single React + TypeScript admin UI for the **hackline-gateway**. Talks
to the gateway's REST + SSE surface (`SCOPE.md` §5.3 / §5.4): devices,
tunnels, cmd outbox, live events, audit, users — plus a first-boot
claim screen.

Modeled on [`codeless/ui/codeless-ui/`](../../../codeless/ui/codeless-ui/):
same conventions (Vite + React 19 + TS + Tailwind v4 + shadcn-style
primitives, `@/` alias), same single-client pattern. Where codeless
has `RpcClient`, hackline has `ApiClient` — `HttpApiClient` against
the real gateway. **No mock client; no `?mock=1` mode.** See the
no-mock policy below.

## Develop

```sh
cd hackline/ui/hackline-ui
pnpm install
pnpm dev                                 # http://localhost:1430
HACKLINE_GATEWAY_URL=https://hackline.example.com pnpm dev   # remote backend
```

The UI requires a running gateway. From the hackline root:

```sh
make start    # gateway + UI in the background
```

`pnpm build` runs `tsc && vite build`.

## No-mock policy

The UI does not ship in-memory fixtures and does not have an offline
mode. Every page hits the real gateway. If the gateway is down the
UI shows an honest "cannot reach gateway" screen.

Rationale (see `DOCS/CODEBASE-ANALYSIS.md` and `DOCS/DEVELOPMENT.md`):
mocks paper over real bugs and let interface drift go unnoticed.
E2E uses a loopback Zenoh router and a real gateway process; that's
the only kind of dev/test loop in this repo.

## Boundary rules

- Components import `useApi()` only — no direct `fetch`, no
  `EventSource`. Transport is the implementation's job.
- Bearer token + base URL live in `localStorage` (`hackline-ui-token`,
  `hackline-ui-base-url`); single-tenant, one token authorises the UI.
- Hash-based routing (`#/devices`, `#/devices/:id`, …) so the gateway
  can serve the bundle from any sub-path without recompiling.

## Boot flow

1. Probe `GET /v1/health`.
   - unreachable → "cannot reach gateway" screen with retry.
   - reachable + unclaimed → claim screen (SCOPE.md §6.1).
   - reachable + claimed + no token → settings prompt.
   - reachable + claimed + token → full app.
