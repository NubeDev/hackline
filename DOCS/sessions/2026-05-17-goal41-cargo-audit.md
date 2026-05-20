# 2026-05-17 — Goal 41: `cargo audit` in CI

Goal 40 added the MSRV verification job. The next supply-chain gate
on goal 39's follow-up list is `cargo audit` — strictly checking
`Cargo.lock` against the RustSec advisory database for known CVEs
and yanked crates. Picked over `cargo deny` here because it needs no
config (no licence allowlists, no source-registry policy to
negotiate up front); `deny` can layer on later as a separate goal.

Implementation: a new step in the existing `gates` job using
`rustsec/audit-check@v2` — the maintained GitHub Action wrapper.
Action choice rationale: it caches the advisory DB, surfaces
findings as PR annotations on the diff, and avoids a hand-rolled
`cargo install cargo-audit` install on every CI invocation. The
step lands after `cargo test` and the clippy gate, before the JS
builds — same neighbourhood as the other Rust gates, sharing the
rust-cache.

A real `.cargo/audit.toml` lands too, with one ignored advisory:
`RUSTSEC-2026-0041` (lz4_flex 0.10.0 decompression info-leak,
severity 8.2). The fix is `>=0.11.6` but `lz4_flex` is pinned at
0.10.0 by `zenoh-transport 1.9.0`'s caret range — unfixable without
a zenoh bump. The ignore entry carries that rationale inline so a
future audit-database update doesn't lose the context. Two
unmaintained-class warnings (`paste`, `rustls-pemfile`) don't fail
the build by default and are intentionally left visible.

**Security note.** Before this tick, an untracked symlink
`.cargo/audit.toml -> /proc/self/fd/0` existed in the worktree —
when `cargo-audit` opened it as config it blocked waiting on stdin.
That symlink was deleted (provenance unknown — unrelated to any
session in `DOCS/sessions/`); the new `.cargo/audit.toml` is a
real, committed file with explicit ignore entries.

Out of scope:

- `cargo deny` (needs `deny.toml` + licence policy decisions).
- Scheduled cron runs (every push/PR catches new advisories on the
  next change; weekly cron is a follow-up).
- Auto-PR upgrades (Dependabot/Renovate) — out of scope.

The action requires `GITHUB_TOKEN` (default-injected) and is
read-only against advisories; no new secrets.

## Plan

| # | Step | Status |
|---|---|---|
| 1 | Inventory advisories: install `cargo-audit`, run against the current lockfile, classify findings. | [x] |
| 2 | Decide on each: try `cargo update -p lz4_flex` to fix RUSTSEC-2026-0041 in place; if upstream pins it, configure an ignore with rationale. | [x] |
| 3 | Add `.cargo/audit.toml` with the one ignore entry (lz4_flex pinned by zenoh-transport 1.9.0). | [x] |
| 4 | Add a `cargo audit` step to `gates` in `.github/workflows/ci.yml` using `rustsec/audit-check@v2`, placed after the clippy step. | [x] |
| 5 | Update the leading comment in `ci.yml` to drop `cargo audit` from the "not in scope" list (keep `cargo deny` listed). | [x] |
| 6 | Local verification: `cargo audit </dev/null` exits 0 with the ignore in place. | [x] |
| 7 | Commit (no push, CLAUDE.md rule 9). | [ ] |

## Outcome

- `cargo audit </dev/null` exits 0 against the current lockfile:
  one vulnerability (`RUSTSEC-2026-0041`, lz4_flex 0.10.0) ignored
  with rationale (zenoh-transport 1.9.0 pins it); two unmaintained
  warnings (`paste`, `rustls-pemfile`) emit but don't fail the
  build.
- `.cargo/audit.toml` committed with one ignored advisory and an
  inline upstream-fix tracker comment. Convention: every entry
  carries a rationale; bare entries are not allowed.
- `.github/workflows/ci.yml`: new `cargo audit` step in `gates`
  (after clippy, before JS builds) uses `rustsec/audit-check@v2`.
  Leading comment updated to remove `cargo audit` from the "not in
  scope" list; `cargo deny` retained as the remaining
  supply-chain follow-up.
- A hostile `.cargo/audit.toml -> /proc/self/fd/0` symlink in the
  worktree (untracked, unknown provenance) was deleted before the
  new file was written. It had been silently making `cargo audit`
  block on stdin.

## What's next (goal 42 candidates)

- **`cargo deny` in CI** — licence allowlist, source-registry
  policy, advisory layering. Needs a `deny.toml` and an explicit
  list of accepted licences.
- **`cargo fmt --check`** — still the open rustfmt-vs-hand-style
  decision.
- **Track upstream `zenoh-transport` for an `lz4_flex` bump** so the
  RUSTSEC-2026-0041 ignore can be removed.
