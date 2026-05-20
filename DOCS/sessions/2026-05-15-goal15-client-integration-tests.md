# 2026-05-15 — Goal 15: loopback integration tests for `@hackline/client`

Goal 14 deleted `MockApiClient` and named the no-mock policy out
loud. This goal makes that policy enforceable: the
`@hackline/client` package now ships a vitest suite that drives the
real `hackline-gateway` `serve` binary against an ephemeral SQLite
DB and a loopback Zenoh listener — no mocks, no fakes, no fixtures,
no `msw`/`nock`.

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Add `vitest` + `@types/node` devDependencies; add `test` / `test:watch` scripts; `vitest.config.ts` with `pool: "forks"` (singleFork) and the global setup wired in | [x] |
| 1 | `test/globalSetup.ts`: build `target/debug/serve` if missing, allocate two free 127.0.0.1 ports (REST + Zenoh), tempdir for `gateway.toml` + `gateway.db` + log, spawn the binary, scrape the claim token from the log, POST `/v1/claim`, publish `{ baseUrl, token, tempDir, logPath }` via `project.provide` | [x] |
| 2 | `test/helpers.ts`: `harness()`, `freshClient()`, `uniqueZid()`, best-effort `deleteDeviceQuiet` / `deleteTunnelQuiet` cleanup | [x] |
| 3 | `test/health.test.ts`: `/v1/health` + `/v1/claim/status` smoke | [x] |
| 4 | `test/devices.test.ts`: create + list + get + delete round-trip; lock in current 404 behaviour of the unimplemented `getDeviceHealth` route | [x] |
| 5 | `test/users.test.ts`: owner mints a scoped user, the minted token can `listDevices`, deleting the user flips the next call to 401 | [x] |
| 6 | `test/tunnels.test.ts`: create (`kind="http"`) + list + delete round-trip with no live agent | [x] |
| 7 | `test/audit.test.ts`: drive `sendCmd` (one of the actually-audited actions per `cmd/send.rs`), assert `listAudit().entries` is non-empty and contains a `cmd.send` row referencing the `cmd_id` we just minted | [x] |
| 8 | `test/errors.test.ts`: bad bearer → `ApiError(401)`, missing record → `ApiError(404)` | [x] |
| 9 | `make test-client` Makefile target | [x] |
| 10 | README "Testing" section: harness model + no-mock reminder + `make test-client` | [x] |
| 11 | Verify: `pnpm install` clean, `pnpm build` clean, `pnpm test` green, `pnpm test` twice in a row green, dev stack on `:8080` / `:1430` untouched | [x] |
| 12 | Grep proof: only the no-mock-policy comment in `globalSetup.ts` matches `mock|stub|fake|fixture|msw|nock` | [x] |

## Design

**Why a single shared gateway per run, not per file.** `pool: "forks"`
with `singleFork: true` means every test runs in the same worker, so
one gateway is fine and the per-test cost is just an HTTP call. Per-file
gateways would multiply the build/spawn cost by 6 with no isolation
benefit at this surface size — every test already cleans the resources
it created in `afterEach`. If the suite ever grows a test that needs a
clean DB it can opt out via its own `globalSetup` file or a child
`describe` that spawns a one-off binary; that's not load-bearing today.

**Why scrape the claim token from the gateway log file.** `serve.rs`
prints the `CLAIM TOKEN: …` line on stdout *before* it starts the REST
listener, and the child's stdout is owned by node for the process
lifetime. Piping the child's stdout through to a node string buffer is
fine in principle but adds backpressure failure modes that have nothing
to do with what we're testing. Redirecting both fds into a log file in
the tempdir gives us a stable filesystem oracle to poll, plus a tail to
surface on failure.

**Why the harness picks free ports rather than hard-coding any.** The
operator's dev stack runs on `:8080` (gateway) and `:1430` (UI vite).
The test gateway must not conflict with either; binding to
`127.0.0.1:0` and reading back the assigned port via a
`net.createServer` round-trip is the simplest way to guarantee that
without inventing a port-allocation library.

**Why `vitest` 2.1, not 3.x.** vitest 3 dropped support for
`globalSetup` returning a teardown function in favour of a different
shape. 2.x is what the codeless-ui side standardised on; matching that
keeps two npm packages in this repo on the same major.

**Where the tests had to bend to current gateway shape.**
Three observed gaps; each is a comment in the test that references the
gateway file so the day the gateway changes shape, the test fails
loudly:

1. `health()` returns `{status: "ok"}` not `{ok: true}` — the typed
   return on `HttpApiClient.health` is a known mismatch (goal-14
   follow-up). The test asserts what the binary actually emits.
2. `getDeviceHealth` route is not mounted today
   (`api/devices/health.rs` is one line of docstring); the test asserts
   the 404 so the route landing flips it red.
3. `device.create` / `tunnel.create` do not write audit rows; the
   audited write actions are `cmd.send` / `cmd.cancel` / `api.call`
   (`api/cmd/send.rs`). The audit test drives `sendCmd` rather than
   asserting against a non-existent `device.create` audit row.

**Why no SSE test.** The `wire.ts::Event` vs `types.ts::GatewayEvent`
reconciliation from goal 14's follow-ups is a prerequisite — testing
the streamed shape today would lock in whichever side of that
mismatch happens to win when the test is written. Out of scope.

## Outcome

- 6 test files, 9 tests, ~600 ms cold-start runtime (gateway spawn
  dominates; tests themselves are ~110 ms).
- `pnpm install` clean.
- `pnpm build` clean (no regression).
- `pnpm test` green twice in a row from a cold state — tempdirs and
  ports are not leaked between runs.
- Dev stack on `:8080` / `:1430` stayed up throughout (`curl` returns
  200 from both at the end of the run).
- `make test-client` runs the same suite from the workspace root.
- Grep proof:

  ```
  $ grep -irE 'mock|stub|fake|fixture|msw|nock' clients/hackline-ts/test/
  ./globalSetup.ts:// vitest's `provide` channel. Per the no-mock policy (goal 14)
  ./globalSetup.ts:// no mocks, no stubs, no fixtures — the binary the tests drive is
  ```

  Both matches are inside the comment that *names* the policy. No test
  code introduces a mock/fixture seam.

Files added:

- `clients/hackline-ts/vitest.config.ts`
- `clients/hackline-ts/test/globalSetup.ts`
- `clients/hackline-ts/test/helpers.ts`
- `clients/hackline-ts/test/health.test.ts`
- `clients/hackline-ts/test/devices.test.ts`
- `clients/hackline-ts/test/users.test.ts`
- `clients/hackline-ts/test/tunnels.test.ts`
- `clients/hackline-ts/test/audit.test.ts`
- `clients/hackline-ts/test/errors.test.ts`

Files modified:

- `clients/hackline-ts/package.json` — added `vitest` + `@types/node`
  devDependencies, `test` / `test:watch` scripts.
- `clients/hackline-ts/README.md` — Testing section.
- `Makefile` — `test-client` target + `.PHONY` listing.
- `pnpm-lock.yaml` — refreshed by `pnpm install` after the
  package.json change.

## What I deferred and why

- **`getDeviceHealth` happy-path assertion.** The route is not
  mounted; I asserted the 404 instead of inventing a shape. Upgrading
  the test is a one-line change once the handler lands.
- **`getDeviceInfo` test.** The handler issues a Zenoh query against
  the device; without a live agent the call hangs to deadline. Out of
  scope per the prompt's "no agent in the loop" guardrail.
- **`sendCmd` ack/expiry path.** Same reason — needs an agent. The
  audit test only asserts the synchronous side (enqueue + audit row).
- **CI wiring.** Per prompt, separate task.
- **Per-file gateways.** Single shared instance is enough at this
  surface size; revisit if isolation pressure shows up.

## What's next (goal 16 / 17 candidates)

- **Reconcile the REST shape mismatches the test comments name.**
  `health()` typed return, `getDeviceHealth` route landing,
  `device.create` / `tunnel.create` audit rows. Each is a small,
  self-contained gateway change that flips an existing test
  assertion green or red on landing.
- **SSE integration test.** Once `wire.ts::Event` vs
  `types.ts::GatewayEvent` is reconciled (goal-14 follow-up), assert
  end-to-end event delivery for `device.online` / `tunnel.opened`
  via the existing `subscribeEvents` surface.
- **Wire `make test-client` into CI.** Workspace `cargo build` cache
  hit on the binary makes this cheap; just needs a job step.
- **Agent-in-the-loop tunnel test.** Drive bytes through a `tcp`
  tunnel against a hackline-agent running in the same harness;
  catches regressions the REST-only tunnel test cannot see.
