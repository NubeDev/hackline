# 2026-05-18 — Goal 50: drop the stale "tolerated warnings" carve-out

CLAUDE.md rule 3 has carried "Two pre-existing dead-code warnings
in `hackline-agent` are tolerated; no *new* warnings" since the
file was first written. Every goal-3 through goal-7 session doc
reaffirmed it. The carve-out is now **stale**: a full
`cargo clean && cargo build --workspace --all-targets` produces
zero warnings, and `cargo clippy --workspace --all-targets --
-D warnings` has been a CI gate since goal 38 (any reintroduction
fails CI immediately).

Keeping the carve-out in CLAUDE.md is actively harmful:

- It tells future agents (and humans) the gate is "warnings + 2
  exceptions" when the actual gate is "zero warnings".
- It invites someone to add a third "tolerated" warning by
  analogy, which the CI gate would then immediately reject —
  wasted work.
- It violates hard rule 1 (no drift between code and docs).

This tick is documentation-only: update CLAUDE.md to reflect the
real state and explicitly close the loophole. No source changes,
no workflow changes.

Out of scope: editing historical session docs (`goal3..goal7`)
and `.codeless/jobs/*/SCOPE.md` snapshots — those are frozen
records of what was true at the time and must not be rewritten.

## Plan

| # | Step | Status |
|---|---|---|
| 1 | Confirm zero warnings: `cargo clean && cargo build --workspace --all-targets`, grep for `warning` / `dead_code` / `unused`. | [x] |
| 2 | Update CLAUDE.md rule 3 to drop the carve-out and reference the goal-38 clippy CI gate as the enforcement mechanism. | [x] |
| 3 | Sanity: re-run `cargo fmt -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` to make sure nothing regressed (no code touched, but cheap to verify). | [x] |
| 4 | Commit (no push, CLAUDE.md rule 9). | [x] |

## Outcome

- Verified `cargo build --workspace --all-targets` from a clean
  state produces no warnings, no dead-code messages, no unused
  imports. The carve-out was describing a state of the world that
  no longer exists.
- CLAUDE.md rule 3 now reads "zero warnings", points at the goal
  38 clippy CI gate as the enforcement mechanism, and explicitly
  tells future agents not to revive the "tolerated" excuse.
- Historical session docs (goal 3, 4, 5, 6, 7) and the
  `.codeless/jobs/*/SCOPE.md` snapshots still mention the
  carve-out — left intentionally. They are point-in-time records
  of what was true when they were written; rewriting them would
  itself violate the "no drift" rule by retconning history.

## What's next

CI hardening (goals 36–49) and documentation alignment (goal 50)
are complete. The remaining roadmap is genuine feature work
that needs operator direction — see SCOPE.md §13 Phase 5:

- Postgres backend behind the SQL repository trait (conditional
  on scale per SCOPE.md).
- Rust→TS codegen for `hackline-proto` + `@hackline/client` npm
  package on Zenoh-WS.
- Wildcard certs for per-org subdomains — externally blocked on
  `instant-acme` DNS-01 support.

Plus the external advisory ignores in `.cargo/audit.toml` /
`deny.toml` (`lz4_flex`, `paste`, `rustls-pemfile`), revisit
when upstream cuts releases.
