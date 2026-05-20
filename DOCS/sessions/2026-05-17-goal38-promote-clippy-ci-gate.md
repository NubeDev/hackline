# 2026-05-17 — Goal 38: promote `cargo clippy -D warnings` in CI

Goal 37 drove `cargo clippy --workspace --all-targets -- -D warnings`
to zero against the pinned MSRV toolchain (1.78). The `ci.yml`
landed in goal 36 explicitly deferred the clippy gate with a comment
pointing at the (then) 11 outstanding warnings. With those gone,
promoting the gate is the single-line workflow change that goal 37
unblocked.

Scope is intentionally narrow: add one step to the existing `gates`
job, between `cargo test` and the JS builds. No matrix expansion, no
`clippy::pedantic`, no separate job — those are follow-ups, not this
tick.

One complication surfaced during the inventory step: clippy 1.95
emits one new `derivable_impls` warning on `LogLevel`'s manual
`Default` impl (added in goal 4, predates the lint). Goal 37's
zero-warning claim was on an older clippy. The mechanical fix —
`#[derive(Default)]` + `#[default]` on `Info` — is
behaviour-preserving and is the smallest change that lets the gate
land green for both CI and local `make check`-style flows.

The MSRV vs newer-deps situation (`cpufeatures 0.3.0` needs
edition2024 ≥ 1.85, but `rust-version = "1.78"`) is **not** in scope
here — that's a separate "re-baseline MSRV" goal. This tick leaves
the existing `toolchain: "1.78"` pin in `ci.yml` alone; whatever
state `cargo test` is in there, the new `cargo clippy` step inherits.

## Plan

| # | Step | Status |
|---|---|---|
| 1 | Inventory: run `cargo clippy --workspace --all-targets -- -D warnings`, confirm the only remaining warning is `derivable_impls` on `LogLevel`. | [x] |
| 2 | Fix `LogLevel`: drop the manual `impl Default`, add `#[derive(Default)]` to the enum and `#[default]` to the `Info` variant. | [x] |
| 3 | Re-run clippy; confirm zero warnings. | [x] |
| 4 | Add a `cargo clippy --workspace --all-targets -- -D warnings` step to `gates` in `.github/workflows/ci.yml`, after `cargo test`. | [x] |
| 5 | Update the leading comment in `ci.yml` to drop clippy from the "not in scope" list. | [x] |
| 6 | `cargo test --workspace` still green. | [x] |
| 7 | Commit (no push, CLAUDE.md rule 9). | [x] |

## Outcome

- `cargo clippy --workspace --all-targets -- -D warnings` exits 0 on
  the local stable toolchain (1.95). `LogLevel`'s manual `Default`
  impl was the single remaining warning; replacing it with
  `#[derive(Default)]` + `#[default] Info` is behaviour-preserving
  (same value, same `Default::default()` call site), so no test or
  wire-format change was needed.
- `cargo test --workspace` still green across every crate and
  integration suite (`cmd_plane`, `message_plane`, `org_isolation`,
  `hackline_proto`, etc.).
- `.github/workflows/ci.yml` now runs `cargo clippy --workspace
  --all-targets -- -D warnings` immediately after `cargo test`,
  inside the same `gates` job (shares the toolchain install, the
  rust-cache, and the workspace checkout — no extra runner time
  beyond compilation). The leading comment block was updated so the
  "not in scope" list no longer claims clippy is deferred.
- Toolchain pin (`toolchain: "1.78"`) was deliberately left alone;
  the cpufeatures/edition2024 mismatch is the next goal's problem.

## What's next (goal 39 candidates)

- **Re-baseline MSRV in CI** — drop the `toolchain: "1.78"` override
  (or bump `rust-version` in `Cargo.toml` to whatever cpufeatures
  0.3.0 actually requires, currently 1.85) so CI runs on a toolchain
  that can compile the lockfile.
- **`cargo fmt --check`** — still the open rustfmt-vs-hand-style
  decision.
- **`cargo deny` / `cargo audit` in CI** — supply-chain gates.
