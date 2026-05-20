# 2026-05-16 — Goal 37: drive `cargo clippy --workspace` to zero warnings

Goal 36 landed `ci.yml` with `cargo test`, the two pnpm builds, and
`make test-client`. It explicitly deferred `cargo clippy -D warnings`
because the workspace currently emits 10 clippy warnings plus the
pre-existing rustc dead-code warning on `AgentError::PortDenied`.
Promoting the clippy gate in CI requires zero warnings first; this
goal drives the count to zero so a follow-up tick can flip the gate
without bundling cleanups.

The standard for "fix" here:

- **Real bugs / unused code → delete.** `PortDenied` is dead; no
  caller, no constructor, no test referencing it. Drop the variant.
- **Stylistic suggestions with a concrete fix → apply the fix.**
  `while_let_loop`, `doc_lazy_continuation`, `clone_on_copy` get
  the suggested rewrite where it preserves behaviour.
- **Smell-style lints the codebase has accepted → workspace allow
  with a one-sentence rationale.** `too_many_arguments` triggers in
  four places (audit-row inserts, listener startup, diagnostic
  state). Refactoring each into a config struct would be a
  drive-by per CLAUDE.md rule 8 — the codebase has decided that
  some entry points take many args.
- **Per-line allows where a fix would change semantics.**
  `clone_on_copy` on `tls.clone()` is correct when the `tls`
  feature is on (cheap `Arc` clone) and only a wart when it's off
  (`Option<Infallible>` is `Copy`). One `#[allow]` per call site
  with a comment beats forking the code on `cfg(feature = "tls")`.

`cargo fmt --check` is *not* in scope; it remains the separate
multi-tick rustfmt-vs-hand-style decision goal 36 deferred.

## Plan

| # | Step | Status |
|---|---|---|
| 1 | Inventory: run `cargo clippy --workspace --all-targets`, list every warning with file/line/lint name. | [x] |
| 2 | Drop `AgentError::PortDenied`. Verify nothing references it (already grepped — only the definition matches). | [x] |
| 3 | Rewrite `bridge.rs:257` `loop { match recv_async().await { Ok(s) => …, Err(_) => break } }` as `while let Ok(s) = recv_async().await { … }`. | [x] |
| 4 | Re-flow the `claim.rs` module doc so `+ insert` does not start a markdown list item that the next line fails to indent into. | [x] |
| 5 | Apply `Option<Infallible>::clone` → keep-as-is at `tunnel/manager.rs:80` and `tunnel/tcp_listener.rs:73` via `#[allow(clippy::clone_on_copy)]` with a one-line rationale (the call is correct under `feature = "tls"`; the lint only fires in the no-feature build where the type is `Option<Infallible>`). | [x] |
| 6 | Add `clippy::too_many_arguments = "allow"` to `[workspace.lints.clippy]` in `Cargo.toml` with a comment that points at the four sites and explains why a config-struct refactor would be a drive-by. | [x] |
| 7 | Add `#[allow(clippy::type_complexity)]` to the `spawn_blocking` closure return in `http_router.rs:94` with a one-line rationale (one-shot tuple, naming it would force a public type alias for a private join). | [x] |
| 8 | `cargo clippy --workspace --all-targets` clean (zero warnings). | [x] |
| 9 | `cargo test --workspace` still green. | [x] |
| 10 | Commit (no push, CLAUDE.md rule 9). | [x] |

## Outcome

- `cargo clippy --workspace --all-targets -- -D warnings` exits 0
  (zero warnings, zero errors). The CI gate is now ready to flip.
- `cargo test --workspace` still green: 33 + 7 + per-crate suites,
  same as before. No behaviour changed; the only deletions are the
  unused `PortDenied` variant and one redundant `Err(_) => break`
  arm.
- Three workspace-wide / per-line allows landed:
  - `clippy::too_many_arguments` at workspace level — the four
    sites it covered are all single-call-site wirings.
  - `clippy::clone_on_copy` on the two `tls.clone()` lines — kept
    because the call is the right cheap-Arc bump under
    `feature = "tls"`.
  - `clippy::type_complexity` on the `http_router.rs` join-tuple
    closure return — naming it would force a public alias for a
    private projection.
- `AgentError::PortDenied` deleted: only one match across the
  workspace before the change (the variant definition itself), so
  nothing downstream needed updating.

## What's next (goal 38 candidates)

- **Promote `cargo clippy -D warnings` in `ci.yml`** — the gate this
  goal unblocks. Single-line workflow change.
- **`cargo fmt --check`** — still the open rustfmt-vs-hand-style
  decision.
- **`cargo deny` / `cargo audit` in CI** — supply-chain gates.
- **Operator decisions** (`User`, `CmdOutboxRow` shapes).
- **Configurable `INFO_POLL_MS`** via Settings.
