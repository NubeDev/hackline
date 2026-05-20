# 2026-05-18 — Goal 49: `cargo fmt --check` as a CI gate

Goal 48 applied `cargo fmt --all` across the workspace and recorded
the option-(a) decision. This tick wires the matching CI gate so
formatting drift is mechanical to catch and impossible to merge.

Placement: first cargo step in the `gates` job. Rustfmt is the
cheapest gate to fail on (no compile required) and the easiest to
diagnose; running it before `cargo test` / `cargo clippy` keeps
later gates' logs free of whitespace-only noise when someone
forgets to format. Also gives a fail-fast signal in the common
"forgot to run fmt" case.

Header comment in `ci.yml` is updated to (a) drop the "not in
scope" stanza and (b) add `cargo fmt` to the per-gate summary in
the opening line, matching the style established by goal 38
(clippy) and goal 42 (deny).

## Plan

| # | Step | Status |
|---|---|---|
| 1 | Verify goal 48 left the tree fmt-clean: `cargo fmt -- --check` exits 0. | [x] |
| 2 | Add `cargo fmt --all -- --check` step to `.github/workflows/ci.yml` `gates` job, ahead of `cargo test`. | [x] |
| 3 | Update the leading comment block in `ci.yml`: drop "Not in scope" stanza, add fmt to the gate summary, add this session doc to the see-also list. | [x] |
| 4 | Sanity-check: re-run `cargo fmt -- --check` locally; confirm green. | [x] |
| 5 | Commit (no push, CLAUDE.md rule 9). | [x] |

## Outcome

- `.github/workflows/ci.yml` now runs `cargo fmt --all -- --check`
  as the first cargo step in the `gates` job. Uses the stable
  toolchain installed by `dtolnay/rust-toolchain@stable` higher
  up in the same job (same toolchain as goal-48 was authored
  against, so the rustfmt output is identical).
- Header comment block reflects the new gate set; the "Not in
  scope: cargo fmt --check" line is gone.
- Local `cargo fmt -- --check` is green; the gate will pass on
  the next CI run.

## What's next

The hackline CI hardening sequence (goals 36–49) is complete. No
deferred local gates remain. Remaining queue (all external or
operator review, not in-tree work):

- **`lz4_flex` / `paste` / `rustls-pemfile` advisory bumps.**
  Tracked in `.cargo/audit.toml` and `deny.toml` with upstream
  links; each ignore has a one-line rationale. Re-evaluate when
  upstream cuts a release.
- **Stray `DOCS/sessions/2026-05-17-goal40-cargo-audit-ci.md`.**
  Unknown provenance — appeared in the working tree without a
  commit and was deliberately left unstaged across goals 48 and
  49. The canonical record of the audit-CI work is
  `2026-05-17-goal41-cargo-audit.md`. Needs operator review to
  decide whether to delete, merge into goal 41, or keep as a
  separate historical artefact.
