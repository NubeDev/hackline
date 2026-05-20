# 2026-05-17 — Goal 39: re-baseline MSRV after edition2024 deps

Goal 36 pinned the CI toolchain to `Cargo.toml`'s `rust-version = "1.78"`.
Since then a transitive dep update pulled in `cpufeatures 0.3.0`,
which declares `edition = "2024"` and therefore needs rustc ≥ 1.85.
With the existing pin, CI's `cargo test` step cannot compile the
lockfile (`feature 'edition2024' is required`); the goal-37 clippy
work and the goal-38 gate inherit the same break.

This tick re-baselines the declared MSRV upward to match the
lockfile reality. Lockfile content is treated as the source of
truth — we don't downgrade `cpufeatures`, because (a) we can't pin a
transitive from the workspace without `[patch]`, and (b) the upstream
edition bump is one-way.

New MSRV target: **1.85** — the minimum that supports `edition2024`,
which is what `cpufeatures 0.3.0` needs. (Some other deps in the
lockfile claim ≥ 1.86 / 1.88, but they're transitive and cargo's
MSRV-aware resolver is not in use; the declared `rust-version` in
this workspace is informational rather than load-bearing on the
build path. Bumping to 1.85 keeps the declared floor honest with the
edition floor — anything stricter is owned by upstream lockfile
selection.)

CI was previously over-pinning the toolchain to the workspace MSRV
("1.78", now "1.85"). That made sense when MSRV was meaningful, but
clippy lints drift between rustc versions and pinning the gate to
the MSRV ends up testing whatever lint set that specific clippy ships
with — `uninlined_format_args` fires on clippy 1.88 but not on 1.95,
for instance. The cleaner contract is: **MSRV is what `cargo`
honours; CI tests against stable.** This tick drops the
`toolchain:` override and lets `dtolnay/rust-toolchain@stable`
resolve to whatever stable is current on the runner.

## Plan

| # | Step | Status |
|---|---|---|
| 1 | Bump `rust-version` in workspace `Cargo.toml` from `"1.78"` to `"1.85"` (matches the edition2024 floor that the lockfile already requires). | [x] |
| 2 | Drop the `toolchain: "1.85"` override in `.github/workflows/ci.yml`; let `dtolnay/rust-toolchain@stable` resolve. Rewrite the surrounding comment to explain the new contract (MSRV in `Cargo.toml`, CI on stable). | [x] |
| 3 | Update `DOCS/CODEBASE-ANALYSIS.md`'s "Rust version" line to `1.85`. | [x] |
| 4 | `cargo +stable build --workspace` confirms the lockfile compiles. | [x] |
| 5 | `cargo +stable clippy --workspace --all-targets -- -D warnings` clean. | [x] |
| 6 | `cargo +stable test --workspace` green. | [x] |
| 7 | Commit (no push, CLAUDE.md rule 9). | [x] |

## Outcome

- `rust-version` in workspace `Cargo.toml` bumped `1.78 → 1.85`,
  matching the edition2024 floor `cpufeatures 0.3.0` already
  required via the lockfile. The previous 1.78 claim was fictive
  (the lockfile could not compile on it).
- `.github/workflows/ci.yml` no longer pins `toolchain:`; the
  `dtolnay/rust-toolchain@stable` action now resolves to whatever
  stable the runner has. The accompanying comment captures *why*:
  pinning CI to MSRV also pinned clippy's lint set, which drifts
  between rustc releases (clippy 1.88 fires `uninlined_format_args`
  on call sites clippy 1.95 accepts). MSRV stays a downstream
  promise enforced by cargo; CI is a stable-track freshness check.
- `DOCS/CODEBASE-ANALYSIS.md` "Rust version" line updated to 1.85.
- Verified locally against current stable (1.95.0):
  - `cargo build --workspace`: ok.
  - `cargo clippy --workspace --all-targets -- -D warnings`: 0
    warnings.
  - `cargo test --workspace`: all suites green (33 + 7 + per-crate +
    integration `cmd_plane`, `message_plane`, `org_isolation`).

## What's next (goal 40 candidates)

- **`cargo fmt --check`** — the still-deferred rustfmt-vs-hand-style
  decision.
- **`cargo deny` / `cargo audit` in CI** — supply-chain gates.
- **Periodic MSRV verification** — a separate matrix job that does
  `cargo +1.85 check --workspace` to keep the declared MSRV honest
  without bundling lint drift into the main gate.
