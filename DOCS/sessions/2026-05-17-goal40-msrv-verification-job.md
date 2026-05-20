# 2026-05-17 — Goal 40: MSRV verification job in CI

Goal 39 set the contract: `Cargo.toml`'s `rust-version` is a
downstream promise, and the main `gates` job runs against current
stable. Nothing currently *checks* the MSRV promise — a PR could add
syntax that needs a newer rustc and the gate would still pass on
stable, silently breaking downstream consumers pinned at the
declared floor. That's exactly what happened: goal 39 set the floor
to 1.85 based on the `cpufeatures` edition2024 requirement, but the
lockfile actually contains deps (`time 0.3.47`, `serde_with 3.20.0`)
that declare `rust-version = "1.88"`. Without a verification job
nothing caught that.

This tick adds a second CI job, `msrv`, that runs `cargo +1.88
check --workspace --all-targets` against the now-honest declared
MSRV, and bumps the floor itself from 1.85 to 1.88 so the gate has
something true to defend. Two deliberate scope cuts on the job:

- **`check`, not `clippy`.** Clippy lints drift between rustc
  releases (the goal-39 discovery — `uninlined_format_args` differs
  between 1.88 and 1.95). The MSRV job's job is "does this code
  *compile* on the floor we promise"; lint policy belongs to the
  stable gate.
- **`check`, not `test`.** Tests pull in additional dev-dep build
  graphs and runtime behaviour; the MSRV claim covers compilation,
  not test-runtime semantics.

The job is its own GitHub Actions job (parallel to `gates`), not a
matrix on `gates`, so the failure attribution stays obvious — "MSRV
broke" is a different signal from "stable broke" — and it keeps its
own rust-cache key.

## Plan

| # | Step | Status |
|---|---|---|
| 1 | Determine the actual lockfile-implied MSRV (`cargo +1.85 check` reports highest dep `rust-version`). | [x] |
| 2 | Bump workspace `Cargo.toml`'s `rust-version` from `"1.85"` to `"1.88"`; update `DOCS/CODEBASE-ANALYSIS.md` to match. | [x] |
| 3 | Add an `msrv` job to `.github/workflows/ci.yml`: `dtolnay/rust-toolchain@1.88`, `Swatinem/rust-cache@v2` with key `msrv`, then `cargo check --workspace --all-targets`. No pnpm or test steps. | [x] |
| 4 | Verify locally: `cargo +1.88 check --workspace --all-targets` exits 0. | [x] |
| 5 | `cargo +stable clippy --workspace --all-targets -- -D warnings` still 0 (no source changed). | [x] |
| 6 | Commit (no push, CLAUDE.md rule 9). | [ ] |

## Outcome

- Real MSRV floor identified as **1.88**, not 1.85 — goal 39's pick
  satisfied edition2024 in `cpufeatures` but missed `time 0.3.47`
  and `serde_with 3.20.0` which declare `rust-version = "1.88"`.
  `Cargo.toml`'s `rust-version` bumped accordingly, and
  `DOCS/CODEBASE-ANALYSIS.md` rewritten to match.
- New `msrv` job in `.github/workflows/ci.yml` runs in parallel with
  `gates`, installs `dtolnay/rust-toolchain@1.88`, uses a dedicated
  `Swatinem/rust-cache@v2` key (`msrv`) so it doesn't trample the
  stable cache, and runs `cargo check --workspace --all-targets`.
  The job comment captures the design constraints: `check` (not
  clippy, not test), separate job (not matrix), separate cache key.
- Verified locally:
  - `cargo +1.88 check --workspace --all-targets`: ok.
  - `cargo +stable clippy --workspace --all-targets -- -D warnings`:
    0 (no source changed, so the goal-38 baseline holds).

## What's next (goal 41 candidates)

- **`cargo fmt --check`** — the still-deferred rustfmt-vs-hand-style
  decision.
- **`cargo deny` / `cargo audit` in CI** — supply-chain gates.
- **Lockfile-aware MSRV tooling** (`cargo msrv verify`) — would have
  caught goal 39's 1.85 miss directly, instead of needing this tick
  to discover it via the new job. Out of scope here because it adds
  a third-party tool install to CI.
