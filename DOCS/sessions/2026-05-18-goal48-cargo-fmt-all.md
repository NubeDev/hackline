# 2026-05-18 — Goal 48: apply `cargo fmt --all`

The `cargo fmt --check` question has been the last deferred CI gate
since goal 36. `ci.yml`'s header comment has carried "codebase is
hand-formatted" for ten goals; every subsequent gate session has
listed fmt as the only remaining item. Resolving it once removes
the recurring question and brings hackline in line with stable
Rust ecosystem norms.

Three options were considered against the current state of the
tree (54 files, ~1460 diff lines on stable rustfmt 1.95):

- (a) Accept rustfmt. One-time churn, permanent enforceable gate,
  matches every other Rust workspace future contributors and tools
  will encounter.
- (b) Author a `rustfmt.toml` that preserves the hand-style. The
  diff is dominated by collapsed one-liners (`fn foo() -> X { "y".into() }`,
  compact struct literals, short `if/else`); the rustfmt options
  that would preserve these (`fn_single_line`, `struct_lit_single_line`,
  `single_line_if_else_max_width` past trivial widths) are all
  unstable. Stable rustfmt cannot reproduce the hand-style.
- (c) Permanent opt-out with a durable rationale comment. Leaves
  the codebase as the perpetual outlier; every future agent reads
  the comment and re-asks the question.

(a) wins on long-term cost: one painful commit, then the gate is
mechanical forever and rustfmt drift is impossible. (b) is blocked
on unstable rustfmt. (c) keeps the cost recurring.

This tick is **only** the formatting commit. Goal 49 wires the
gate into CI. Splitting the two keeps the formatting blast radius
isolated from the workflow change (and keeps the commit reviewable
as "100% rustfmt output, nothing else").

## Plan

| # | Step | Status |
|---|---|---|
| 1 | Inventory: capture file list and diff volume from `cargo fmt -- --check --files-with-diff` and `cargo fmt -- --check`. | [x] |
| 2 | Confirm the diff is pure style (no semantic changes a rustfmt could conceivably introduce). | [x] |
| 3 | Run `cargo fmt --all`. | [x] |
| 4 | `cargo check --workspace --all-targets` — no semantic regressions. | [x] |
| 5 | `cargo test --workspace` — green. | [x] |
| 6 | `cargo clippy --workspace --all-targets -- -D warnings` — still green (rustfmt occasionally trips lints that fire only on certain layouts). | [x] |
| 7 | Stage formatted files explicitly (`git add` per path, never `-A`; CLAUDE.md rule and the goal-40 incident). | [x] |
| 8 | Commit (no push, CLAUDE.md rule 9). | [x] |

## Outcome

- 54 files reformatted across `hackline-agent`, `hackline-cli`,
  `hackline-client`, `hackline-core`, `hackline-gateway`, and
  `hackline-proto`. Diff is the expected stable-rustfmt expansion
  of hand-compacted one-liners (single-line fns, compact struct
  literals, short `if/else`, collapsed `.await?` chains). No
  semantic changes.
- `cargo check --workspace --all-targets`, `cargo test --workspace`,
  and `cargo clippy --workspace --all-targets -- -D warnings` are
  all green post-fmt.
- The `ci.yml` header still says "not in scope: cargo fmt --check".
  Goal 49 removes that comment and adds the workflow step.

## What's next

- Goal 49 — add `cargo fmt --all -- --check` to `ci.yml`'s `gates`
  job and remove the deferral comment.
- After goal 49, the only remaining open items are external
  upstream waits (`lz4_flex` / `paste` / `rustls-pemfile` advisory
  bumps tracked in `.cargo/audit.toml` and `deny.toml`) and the
  stray `DOCS/sessions/2026-05-17-goal40-cargo-audit-ci.md` (unknown
  provenance, needs operator review against
  `2026-05-17-goal41-cargo-audit.md` which is the canonical record).
