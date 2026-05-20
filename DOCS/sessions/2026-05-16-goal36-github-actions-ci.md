# 2026-05-16 — Goal 36: first GitHub Actions workflow (`ci.yml`)

The repo has four currently-green tested gates today and zero
CI: `cargo test --workspace`, `cargo build -p hackline-gateway
--bin serve`, `pnpm -C ui/hackline-ui {typecheck,build}`,
`pnpm -C clients/hackline-ts {build,test}`. Operators land
changes by running these locally and trusting nothing
regresses on machines they don't touch. A single `ci.yml`
exercising all four closes that gap.

`cargo fmt --check` and `cargo clippy -D warnings` are *not*
in scope here — the codebase is hand-formatted (1460 lines
of rustfmt drift) and clippy reports 11 warnings. Either
gate would ship red; promoting them to green is its own
multi-tick cleanup. Documented as deferred goals.

This checkout's `origin` is wrong (CLAUDE.md rule 9), so the
workflow lands but isn't exercised from here. The operator who
pushes the eventual real-origin commit gets the green check on
day one (for the gates included).

## Plan

| # | Step | Status |
|---|---|---|
| 1 | Add `.github/workflows/ci.yml`. Single `ubuntu-latest` job, push + pull_request triggers, manual dispatch. Concurrency group cancels superseded in-flight runs on the same ref. | [x] |
| 2 | Step: checkout, install Rust toolchain pinned to `Cargo.toml`'s `rust-version = 1.78` with rustfmt + clippy components, install pnpm via `pnpm/action-setup`, install Node 22 (matches `@types/node ^22`), `pnpm install --frozen-lockfile`. | [x] |
| 3 | Cache step: `Swatinem/rust-cache@v2` keyed on `Cargo.lock`. (pnpm caching is bundled in `pnpm/action-setup`+`actions/setup-node` by passing `cache: pnpm`.) | [x] |
| 4 | Gates in dependency order (fail fast, smallest first): `cargo test --workspace`, `pnpm -C clients/hackline-ts build`, `pnpm -C ui/hackline-ui typecheck`, `pnpm -C ui/hackline-ui build`, `make test-client` (runs `cargo build -p hackline-gateway --bin serve` internally and then vitest). | [x] |
| 5 | Local sanity check: run each command verbatim that the workflow runs, confirm green. | [x] |
| 6 | Stage + commit (no push, per CLAUDE.md rule 9). | [x] |

## Design

**Why one job, not a matrix.** Single-target deployment
(`ubuntu-latest` is what operators run); matrices would cost
build minutes and complicate cache keys for no signal. macOS
isn't a target. The hackline binaries don't ship cross-
platform yet.

**Why the `1.78` rust-version pin.** `Cargo.toml` already
declares `rust-version = "1.78"` as the MSRV. Pinning the CI
toolchain to the same value enforces the MSRV in CI; using
`stable` would let new-stable-only syntax land silently.

**Why `clippy -- -D warnings` despite `[workspace.lints]`
declaring clippy `all = warn`.** Workspace-level warn turns
clippy lints into compiler warnings; CI needs them to fail
the build. `-D warnings` promotes everything (rustc + clippy)
to error for the duration of that step. **Deferred for this
goal**: clippy currently reports 11 warnings (`while_let_loop`,
`too_many_arguments`, `option_clone_on_copy`, etc.) plus the
pre-existing `PortDenied` rustc dead-code warning. Promoting
-D warnings means landing all the fixes in the same goal,
which would be a multi-tick cleanup. Tracked as a follow-up.

**Why `pnpm install --frozen-lockfile`.** Same lockfile that
operators run; mismatched lockfile (a CI-only resolution)
would let dependency drift land that local `pnpm install`
would have caught.

**Why no codecov / lint reports / other "nice to have".**
First-CI scope is "do the gates run cleanly". Coverage,
release builds, container images, and reporting are separate
decisions and additive.

**Why concurrency-cancel.** Pushes to a PR branch otherwise
queue redundant runs of the older commits; cancelling the
superseded ones saves runner time without losing signal
(the latest commit's run is the one operators look at).

**Why no separate "build matrix" for the gateway binary.**
`make test-client` already requires the gateway binary; it
runs `cargo build -p hackline-gateway --bin serve` as part
of its setup. Re-running it as a separate step would waste
build time on the same artifact already in the cache.

## Outcome

- `cargo test --workspace`: all green (33 gateway + 7 proto
  + small per-crate suites).
- `pnpm -C clients/hackline-ts build`: clean.
- `pnpm -C ui/hackline-ui typecheck` + `build`: clean.
- `make test-client`: 6 files / 13 tests green.
- `.github/workflows/ci.yml` lands; not exercised here
  because origin is wrong (CLAUDE.md rule 9).

## What's next (goal 37 candidates)

- **Promote `cargo fmt --check`** — either accept rustfmt
  output as the format (1460-line auto-format diff) or pin
  a custom rustfmt config that matches the existing hand
  style.
- **Promote `cargo clippy -D warnings`** — fix the 11
  outstanding warnings (`while_let_loop`,
  `too_many_arguments`, `option_clone_on_copy`, etc.) plus
  drop the dead `PortDenied` variant.
- **Operator decisions** (`User`, `CmdOutboxRow`).
- **Configurable `INFO_POLL_MS`** via Settings.
- **CI: add `cargo deny` / `cargo audit`** for supply-chain
  surface.
