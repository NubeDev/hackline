# 2026-05-17 — Goal 40: add `cargo audit` to CI

Picks the supply-chain half of goal 39's "what's next" list. `cargo
audit` checks the workspace's resolved dependency graph against the
RustSec advisory database — a low-friction, high-signal addition
that requires no policy debates (unlike `cargo deny`'s config file,
which negotiates licence allowlists and source registries up front).

Scope: one new step in the existing `gates` job, using
`rustsec/audit-check@v2` (the maintained GitHub Action wrapper).
Action choice rationale: it already handles caching and emits
annotations on the diff, avoiding a hand-rolled `cargo install
cargo-audit` + run dance every CI invocation. If the project later
wants `cargo deny` for licence/source policies, that's a separate
goal — `audit` is the strict subset that catches known CVEs and
yanked crates, and the two coexist cleanly.

Out of scope:

- `cargo deny` — needs a `deny.toml` and licence-policy decisions.
- Periodic scheduled runs (cron) — the gate runs on every push/PR;
  if a new advisory drops between pushes it gets caught on the next
  one. A weekly cron is a follow-up.
- Auto-PRs to upgrade vulnerable deps (Dependabot/Renovate) — out
  of scope; this gate only fails the build, doesn't fix anything.

## Plan

| # | Step | Status |
|---|---|---|
| 1 | Run `cargo audit` locally (install if missing) against the current `Cargo.lock` to see whether the baseline is clean. If any advisory fires, decide per-advisory: real fix (bump dep) vs. documented `ignore` (with rationale). | [ ] |
| 2 | If any advisories are intentionally ignored, add `.cargo/audit.toml` with the ignore list + one-line rationale per entry. Otherwise skip. | [ ] |
| 3 | Add a `cargo audit` step to `gates` in `.github/workflows/ci.yml`, using `rustsec/audit-check@v2`, after `cargo clippy`. | [ ] |
| 4 | Update the leading comment in `ci.yml`: drop `cargo audit` from the "not in scope" line, leave `cargo deny` there. | [ ] |
| 5 | Commit (no push, CLAUDE.md rule 9). | [ ] |

## Outcome

_pending_
