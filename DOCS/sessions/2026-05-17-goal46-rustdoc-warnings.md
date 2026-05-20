# goal 46 — rustdoc warnings as errors in CI

## context

clippy, audit, deny, and machete now gate the Rust code, lockfile,
and dependency graph. nothing yet catches *documentation* rot:
broken intra-doc links (`[foo::bar]` where `bar` was renamed),
invalid disambiguators, malformed HTML in doc strings, missing
crate-level docs that rustdoc would normally warn about. these
accumulate silently because `cargo build` and `cargo test` don't
invoke rustdoc.

## changes

- new CI step `cargo doc` between `cargo clippy` and `cargo audit`:
  - `--workspace --no-deps` so only first-party crates are built.
  - `--document-private-items` so the gate covers the same surface
    humans see via `cargo doc --open` on a single crate.
  - `RUSTDOCFLAGS="-D warnings"` promotes every rustdoc warning
    (broken links, unresolved references, malformed HTML, etc.)
    to a hard failure.
- header comment updated to list the new gate.

no source edits required — the tree already builds clean docs at
`-D warnings`. verified locally:

    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps \
        --document-private-items

## verification

- local rustdoc build at `-D warnings`: 0 warnings, 0 errors.
- (other gates unchanged; this commit only touches the workflow
  and its header comment.)

## what's next

- `cargo fmt --check` — still deferred (needs user input).
- weekly cron of audit/deny so new RUSTSEC entries against an
  unchanged lockfile surface within 7 days instead of "whenever
  someone pushes next".
- track external deps for the three open advisory ignores
  (lz4_flex, paste, rustls-pemfile).
