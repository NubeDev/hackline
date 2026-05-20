# 2026-05-16 — Goal 34: surface info-endpoint failures in `DeviceDetailPage` Agent info card

Today the Agent info card swallows every failure (`.catch(() =>
{})`) and shows "live query pending…" forever. After goal 32 the
gateway returns distinct statuses — 504 (agent timeout), 503
(agent unreachable), 502 (decode failure) — and `ApiError`
already carries `status: number`. Mapping those to operator-
visible copy turns the card from "I don't know" into "the agent
isn't answering" / "the agent answered with garbage" / etc.

## Plan

| # | Step | Status |
|---|---|---|
| 1 | Track an `infoErr: ApiError \| null` alongside `info` in `DeviceDetailPage`. On failure, set it from the caught error (only when it's an `ApiError`, otherwise rethrow to the outer error boundary so we don't silently hide bugs). | [x] |
| 2 | Render: if `info` present → existing rows; else if `infoErr` present → muted-foreground line keyed on status (504 → "agent did not reply within 1 s", 503 → "no agent listening for this device", 502 → "agent reply could not be decoded", other → generic "agent info unavailable (HTTP n)"). Else → "live query pending…". | [x] |
| 3 | Gates: `pnpm -C ui/hackline-ui typecheck` + `build`, `pnpm -C clients/hackline-ts build`, `make test-client` green twice. (No Rust changes.) | [x] |

## Design

**Why an `instanceof ApiError` check, not duck-typing on
`.status`.** `ApiError` is re-exported from `@/lib/api` and is
already used by `ApiError` consumers elsewhere; the
`instanceof` check makes it impossible to mistake an arbitrary
`{ status }`-shaped value (e.g. a fetch `Response` accidentally
caught) for a structured API error.

**Why rethrow non-`ApiError` failures rather than swallow them.**
If the error isn't a server response (e.g. network failure,
abort, programming error from a future refactor), the existing
`.catch(() => {})` hides it from the dev console. Surfacing
non-API failures through `setError` routes them to the page-
level `ErrorBox`, matching how `getDevice` / `listTunnels`
already behave. The Agent info card is best-effort *for
server-told failures*; client-side bugs are not best-effort.

**Why hard-coded copy keyed on numeric status, not a lookup
table.** Three statuses, one fallback, one site. A lookup
table would be more code for the same output. If a future
endpoint reuses the same error mapping, factoring out is
trivial; doing it pre-emptively is the over-engineering the
file rules warn against.

**Why no retry button.** The info call already re-runs every
time the user navigates back to the page; a manual retry adds
a control with no path that the natural workflow doesn't
cover. If `HEALTH_POLL_MS`-style background polling for info
becomes desirable, that's a separate decision (extra Zenoh
load per device per cadence × every detail-page viewer).

## Outcome

- `pnpm -C ui/hackline-ui typecheck` + `build`: clean
  (bundle 259.29 KB / 78.65 KB gz — marginally larger,
  +12 lines of UI logic).
- `pnpm -C clients/hackline-ts build`: clean.
- `make test-client`: 6 files / 13 tests green twice.

## What's next (goal 35 candidates)

- **First GitHub Actions workflow** for Rust + UI + client
  gates.
- **Operator decisions** (`User`, `CmdOutboxRow`).
- **Background-poll the info endpoint** at a slower cadence
  than health (e.g. 30 s) so version changes during a
  rolling upgrade show up without a navigation away/back.
