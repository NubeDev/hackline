# 2026-05-17 — Goal 42: `cargo deny` in CI

Goal 41 landed `cargo audit` for known CVEs. `cargo deny` is the
remaining supply-chain follow-up: licence allowlist + source-registry
policy + duplicate-version policy + advisory layering on top. It
catches things `cargo audit` does not: a transitive dep switching to
a copyleft licence, a crate being pulled from a non-crates.io
registry, a vendored dep with a forbidden licence.

Configuration choices made up front, since `cargo deny` requires a
real policy:

- **Licences allowed** — the permissive set the lockfile already
  uses: Apache-2.0, MIT, BSD-2-Clause, BSD-3-Clause, ISC, BSL-1.0,
  Zlib, Unicode-3.0, Unlicense, plus the weak-copyleft set already
  present (MPL-2.0 — file-level, fine; EPL-2.0 — pulled in via
  zenoh, weak copyleft requiring source disclosure of modifications,
  acceptable for a library dependency we don't modify).
- **LGPL-2.1-or-later** is **not** broadly allowed. Two crates carry
  it (`r-efi` 5.3.0 and 6.0.0), both EFI-target-only bindings never
  linked into our Linux/macOS binaries. They land as explicit
  per-crate `exceptions` with a rationale, not a blanket licence
  allowance — the moment a non-EFI crate adopts LGPL it should fail
  the gate.
- **Sources** — default (crates.io only). No git deps in the
  lockfile, no vendored registries.
- **Bans** — `multiple-versions = "warn"` (informational; the tree
  has duplicates from zenoh/reqwest and chasing them all is a
  separate dependency-hygiene goal). `wildcards = "deny"`.
- **Advisories** — mirror the `RUSTSEC-2026-0041` ignore from
  `.cargo/audit.toml` (lz4_flex pinned by zenoh-transport 1.9.0).
  Cargo-deny does not read cargo-audit's config, so the ignore is
  duplicated; both rationales point at the same upstream tracker.

Action used: `EmbarkStudios/cargo-deny-action@v2` — official, caches
its own state, runs the four checks (`advisories`, `bans`, `licenses`,
`sources`) as a single step.

## Plan

| # | Step | Status |
|---|---|---|
| 1 | Survey licences in the lockfile (`cargo deny list -l license`); identify any non-permissive entries. | [x] |
| 2 | Write `deny.toml` with the licence allowlist, the `r-efi` LGPL exception, sources = crates.io only, bans (wildcards=deny + allow-wildcard-paths, multiple-versions=warn), and the advisory ignores (lz4_flex + paste). | [x] |
| 3 | Declare workspace crates as `publish = false` (canonical truth — these aren't on crates.io) so cargo-deny's `allow-wildcard-paths` recognises the workspace-internal `*` deps as private. | [x] |
| 4 | Local verification: `cargo deny check` exits 0, all four checks ok. | [x] |
| 5 | `cargo check --workspace` still green after the Cargo.toml changes. | [x] |
| 6 | Add a `cargo deny` step to `gates` in `.github/workflows/ci.yml` after `cargo audit`, using `EmbarkStudios/cargo-deny-action@v2`. | [x] |
| 7 | Update the leading comment in `ci.yml` to drop `cargo deny` from the "not in scope" list. | [x] |
| 8 | Commit (no push, CLAUDE.md rule 9). | [ ] |

## Outcome

- `deny.toml` lands with a principled minimal policy: the permissive
  licences the lockfile already uses (Apache-2.0, MIT, BSD-2/3,
  ISC, BSL-1.0, Zlib, Unicode-3.0, Unlicense), the two weak-copyleft
  ones already present (MPL-2.0, EPL-2.0), and a per-crate
  exception for `r-efi` (LGPL-2.1-or-later, EFI-target-only). Sources
  restricted to crates.io. Wildcards denied on registry deps, allowed
  on workspace path deps. Multiple-versions kept as warn (the tree
  has zenoh/reqwest-flavoured duplicates; chasing them is a separate
  dependency-hygiene goal).
- Advisory ignores in `deny.toml` mirror `.cargo/audit.toml`:
  `RUSTSEC-2026-0041` (lz4_flex pinned by zenoh-transport),
  `RUSTSEC-2024-0436` (paste unmaintained, transitive via sqlx-macros).
  `RUSTSEC-2025-0134` was dropped during validation — cargo-deny
  reports "advisory was not encountered" against the resolved
  rustls-pemfile, so listing it would only generate a stale-ignore
  warning.
- All seven workspace crates now declare `publish = false` (via
  `[workspace.package] publish = false` + `publish.workspace = true`
  in each member). This is the truth: nothing in this tree is for
  crates.io. cargo-deny needs it to honour `allow-wildcard-paths`
  for the workspace-internal `*` deps.
- `.github/workflows/ci.yml`: new `cargo deny` step after `cargo
  audit` using `EmbarkStudios/cargo-deny-action@v2` with
  `command: check`. Leading comment updated; clippy/audit/deny all
  removed from the "not in scope" list — only `cargo fmt --check`
  remains there.
- Verified locally:
  - `cargo deny check`: `advisories ok, bans ok, licenses ok, sources ok`.
  - `cargo check --workspace`: ok (the `publish.workspace = true`
    additions are inert at build time).

## What's next (goal 43 candidates)

- **`cargo fmt --check`** — the last remaining CI gate from the
  goal-36 "not in scope" list. Still gated on the
  rustfmt-vs-hand-style decision (whether to accept the auto-format
  diff or maintain a custom rustfmt.toml).
- **Dependency-hygiene pass** — investigate the `multiple-versions`
  warnings cargo-deny emits (hashbrown ×4, rand ×3, etc.) and
  decide which are worth deduping with `cargo update --precise`.
- **Track upstream `zenoh-transport` for an `lz4_flex` bump** so the
  RUSTSEC-2026-0041 ignore can be removed (and downgrade
  RUSTSEC-2024-0436 once sqlx migrates off paste).
