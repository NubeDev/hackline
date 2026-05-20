# goal 44 — cargo-machete in CI

## context

with cargo-audit (goal 41) and cargo-deny (goal 42) gating the
runtime dependency graph, the remaining unguarded surface is *unused*
`[dependencies]` entries: deps that compile and link but no source
file imports. they bloat lock resolution, slow CI, drag in transitive
advisories for nothing, and rot silently because nothing fails when
they go stale.

cargo-machete checks every Cargo.toml in the workspace and reports
deps with no matching `use` / extern reference.

## findings (initial run)

cargo-machete v0.9 flagged four genuine deps:

- `hackline-client` -> `serde` (only `serde_json` is used)
- `hackline-gateway` -> `tower-http` (never imported)
- `hackline-core` -> `futures` (never imported; gateway uses it but
  declares its own dep)
- `hackline-cli` -> `hackline-proto` (only mentioned in a doc-comment
  on `src/client.rs`, not actually imported)

each was verified by grep across the relevant `src/` tree before
removal. no false positives.

## changes

- four Cargo.toml edits dropping the unused deps.
- one doc-comment fix in `crates/hackline-cli/src/client.rs` (the
  comment claimed types came from `hackline-proto`; they don't — the
  CLI decodes JSON into locally-defined `serde` value types).
- new CI step `cargo machete` between `cargo deny` and the JS
  builds, installing the binary via `taiki-e/install-action@v2` for
  a pre-built download (avoids the ~30s `cargo install` compile).
- no `Cargo.lock` change is required by this commit: dropping unused
  `[dependencies]` entries cannot remove any locked package, because
  every flagged dep is still pulled in transitively by another
  workspace member (futures by gateway, serde by cli/gateway,
  hackline-proto by all the runtime crates, tower-http by reqwest's
  chain). machete's value here is hygiene, not graph reduction.

## verification

- `cargo check --workspace --all-targets` — ok.
- `cargo machete` — clean (no unused deps).
- `cargo test --workspace` — all suites green.
- `cargo clippy --workspace --all-targets -- -D warnings` — 0
  warnings.
- (cargo audit / deny unchanged — same lockfile.)

## what's next

- `cargo fmt --check` — still long-deferred (needs user input on
  rustfmt-vs-handstyle).
- track zenoh-transport for lz4_flex 0.10 -> 0.11.
- track sqlx-macros migrating off `paste`.
- consider `cargo deny check` advisories with `--all-features` to
  catch advisories that only surface under feature flags currently
  off in CI (gateway's `tls` feature in particular).
