# 2026-05-15 — Goal 13: extract `@hackline/client` npm package (Phase 5)

The Rust→TS wire bridge from goals 11+12 produced a stable
`wire.ts`. Goal 13 lifts the existing TypeScript client out of
`hackline-ui` into the standalone npm package the SCOPE has been
calling for, so future consumers (rubix UI, support tools, third-party
embedders) depend on `@hackline/client` rather than copy-pasting from
the admin UI.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Scaffold `hackline/clients/hackline-ts/` as a real package: `package.json` (`@hackline/client`, ESM, types), `tsconfig.json`, `README.md`, `.gitignore`, plus a `pnpm-workspace.yaml` at the hackline root listing both this package and `ui/hackline-ui` | [x] |
| 1 | Move the framework-agnostic client files (`client.ts`, `http-client.ts`, `mock-client.ts`, `types.ts`, `token.ts`) from `ui/hackline-ui/src/lib/api/` into `clients/hackline-ts/src/`. The generated `wire.ts` already lives there; export it on a `./wire` subpath | [x] |
| 2 | Update `ui/hackline-ui/package.json` to depend on `@hackline/client` via `workspace:*`. Replace the moved files with a thin shim under `lib/api/` that re-exports from the package and keeps the React-only `provider.tsx` (which stays UI-side because it imports React) | [x] |
| 3 | `pnpm install`, `pnpm -r typecheck`, `pnpm -r build` from the hackline root — UI still builds, no behavior change | [x] |

## Design

**Why extract now.** The wire-types contract is stable enough
(goals 11+12) that a published client surface no longer risks
churning consumers every week. The UI is the de-facto reference
implementation of the client; keeping it as the only source of
truth means a second consumer would have to copy or vendor it.
Pulling it out as `@hackline/client` collapses both problems into
"depend on the package."

**Why the package lives at `clients/hackline-ts/`.** SCOPE.md §3.3
already names that path. The Rust SDK is `hackline-client`; the TS
SDK is `clients/hackline-ts/` exporting `@hackline/client`. The
naming gap (`hackline-client` vs `@hackline/client`) is intentional:
the Rust crate name follows cargo conventions; the npm scope
follows npm conventions.

**Why the React provider stays UI-side.** The package is
framework-agnostic — pure interfaces, types, and a fetch/EventSource
implementation. Moving `provider.tsx` into the package would force a
React peer dependency on every consumer, including future Vue/Svelte
embedders. The provider is one tiny file; leaving it in the UI keeps
the package's footprint minimal.

**Why `workspace:*` and not a published version yet.** v0.1 — the
package has no semver story, no changelog, no npm-publish CI. A
pnpm workspace link is exactly the right intermediate state: the
UI builds against the local source, future external consumers can
either workspace-link or wait for the first publish.

**Why a thin shim instead of a hard rewrite of every UI import.**
Changing every `from "@/lib/api"` to `from "@hackline/client"`
across the UI is mechanical churn that would clutter the diff and
force a retest of every page for an edit that should be invisible.
Keep `lib/api/index.ts` as a one-line re-export from the package;
the UI's import paths and behavior stay byte-identical, the
package becomes the source of truth, and a future cleanup can
inline the imports without entangling the extraction.

**Wire types: subpath export, not auto-merged.** The hand-written
`types.ts` and the generated `wire.ts` overlap in concept but not in
shape (the gateway's SSE event shape is hand-written, the wire's
`Event` enum is the Zenoh-side shape; they will be reconciled when
the gateway grows specta-derived types). Until then, both are
exported but on distinct subpaths: `@hackline/client` (REST surface)
and `@hackline/client/wire` (Zenoh-side wire). Consumers see two
clearly named surfaces; the eventual reconciliation collapses them
without breaking imports.

**Out of scope.**

- Reconciling `types.ts::GatewayEvent` with `wire.ts::Event`. That
  needs the gateway to emit specta types for its REST surface; goal
  for after the Postgres backend lands.
- Zenoh-WS transport. That's the real "transport.ts" from the
  goal-12 followup list — much bigger, lands once the gateway grows
  a Zenoh-WS bridge endpoint and we have a story for browser-side
  Zenoh sessions.
- npm publish CI. v0.1 the package is workspace-only.

## Outcome

The TS client is now a standalone npm package consumed by
hackline-ui via a pnpm workspace link. The UI's call sites stayed
byte-identical — every `from "@/lib/api"` import still works — because
the shim re-exports the package surface unchanged. The package itself
carries no React dependency; the React-only `ApiProvider` / `useApi`
stayed in the UI.

Layout that landed:

```
hackline/
  pnpm-workspace.yaml          # packages: clients/*, ui/*
  clients/hackline-ts/         # @hackline/client v0.1.0
    package.json               # ESM, exports "." + "./wire"
    tsconfig.json              # emits dist/
    README.md
    .gitignore                 # dist/, node_modules/, *.tsbuildinfo
    src/
      index.ts                 # public surface
      client.ts                # ApiClient interface, ApiError
      http-client.ts           # HttpApiClient (REST + SSE)
      mock-client.ts           # MockApiClient (fixtures)
      types.ts                 # REST request/response shapes
      token.ts                 # bearer-token + base-url storage
      wire.ts                  # Rust-generated wire types
  ui/hackline-ui/
    package.json               # adds "@hackline/client": "workspace:*"
    src/lib/api/
      index.ts                 # thin re-export shim
      provider.tsx             # React-only, imports from @hackline/client
```

Verified:
- `pnpm install` clean (132 packages, all reused from store).
- `pnpm -C clients/hackline-ts build` — emits `dist/` with `.js` +
  `.d.ts` + sourcemaps for every source file.
- `pnpm -C ui/hackline-ui typecheck` — zero errors.
- `pnpm -C ui/hackline-ui build` — succeeds, bundle size unchanged
  (~262 KB JS / ~80 KB gzipped).

## What's next

- Reconcile `types.ts::GatewayEvent` with `wire.ts::Event` once the
  gateway emits specta-derived REST types. Drops the dual-surface
  caveat from the README and lets consumers import a single set of
  event types.
- Zenoh-WS transport (`transport.ts`) over the gateway's future
  WS-bridge endpoint. The package now has the right home for it.
- npm publish CI when the API is stable enough for an external
  consumer (rubix UI is the obvious first one).
