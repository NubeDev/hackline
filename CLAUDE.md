# CLAUDE.md — rules for agents working inside the hackline repo

You are working inside **hackline**, a Zenoh-native fleet service for
IoT edge devices. The design is captured in [`SCOPE.md`](./SCOPE.md)
(load-bearing — code and SCOPE must not drift) with supporting docs
under [`DOCS/`](./DOCS/). The historical record of completed work is
[`DOCS/sessions/`](./DOCS/sessions/).

Hackline started life as a POC inside the `codeless-workspace`
monorepo and was extracted to its own repository on 2026-05-20.
Older session notes may still reference codeless concepts (JOB-LOOP,
`./bin/mani`, four-shell architecture, R1–R5 rules) — **none of that
applies here**. This file is the only one that governs hackline.

## What hackline is (one paragraph)

Two planes on one Zenoh fabric: a **tunnel plane** (bytes — per-device
HTTP and TCP reachable from the cloud) and a **message plane** (typed
envelopes — events, durable commands, RPC, logs). One gateway, one
CLI, one device-side SDK, one SQLite, one auth token. Built on Zenoh
because every device already runs it; hackline is the application
layer on top.

## Where the truth lives

| Topic | File |
|---|---|
| Top-level scope, phasing, open questions | [`SCOPE.md`](./SCOPE.md) |
| Architecture overview | [`DOCS/ARCHITECTURE.md`](./DOCS/ARCHITECTURE.md) |
| REST surface (existing endpoints) | [`DOCS/REST-API.md`](./DOCS/REST-API.md), [`DOCS/openapi.yaml`](./DOCS/openapi.yaml) |
| Auth model | [`DOCS/AUTH.md`](./DOCS/AUTH.md) |
| Persistence | [`DOCS/DATABASE.md`](./DOCS/DATABASE.md) |
| CLI | [`DOCS/CLI.md`](./DOCS/CLI.md) |
| Config files | [`DOCS/CONFIG.md`](./DOCS/CONFIG.md) |
| Keyexpr conventions | [`DOCS/KEYEXPRS.md`](./DOCS/KEYEXPRS.md) |
| Codebase analysis (what's where) | [`DOCS/CODEBASE-ANALYSIS.md`](./DOCS/CODEBASE-ANALYSIS.md) |
| Past completed work | [`DOCS/sessions/`](./DOCS/sessions/) |

## Phasing (where we are)

Per `SCOPE.md` §13:

- Phase 0 — Zenoh spike — **done** (`goal0-bridge-spike`)
- Phase 1 — Tunnel plane happy path — **done** (`goal1-real-binaries`, `goal2-sqlite-rest`, `goal3-auth-cli`)
- Phase 1.5 — Message plane: events + logs — **done** (`goal4-message-plane`)
- Phase 2 — Commands + api + HTTP host-routing — **done** (`goal5-cmd-api-host-routing`)
- Phase 3 — Audit completeness + admin UI — **done** (`goal6-audit-admin-ui`)
- Phase 4 — Multi-tenant orgs — **done** (`goal7-multi-tenant-orgs`)
- Phase 5 — Deployment polish (ACME/TLS) — **done** (`goal8-acme-tls`, `goal9-tunnel-tls`); ACME renewal + Postgres + TS codegen remaining

All session docs in `DOCS/sessions/`. Next work: ACME cert renewal,
Postgres backend, or Zenoh-WS browser client.

## Hard rules

These are non-negotiable; trip one and the work halts.

1. **No drift between code and `SCOPE.md`.** If a design needs to
   change, update SCOPE.md in the same commit as the code.
2. **Each new milestone gets a session note** under
   `DOCS/sessions/YYYY-MM-DD-goalN-<slug>.md`, in the same shape as
   `2026-05-14-goal3-auth-cli.md`: a plan table (Step / Status),
   Outcome (what was verified, with curl/test commands), Design (the
   non-obvious decisions). Plan table written first, ticked as work
   lands.
3. **Workspace must build clean.** `cargo check --workspace` and
   `cargo test --workspace` green at the end of every stage. Zero
   warnings: `cargo clippy --workspace --all-targets -- -D warnings`
   is a CI gate (goal 38) and the formerly tolerated `hackline-agent`
   dead-code warnings were cleared in the intervening work (goal 50
   removed the carve-out). Don't re-introduce the "tolerated" excuse;
   fix the warning or justify a `#[allow(...)]` with a one-line
   rationale comment.
4. **Migrations are append-only.** New tables go in a fresh
   `Vnnn__*.sql` under `crates/hackline-gateway/migrations/`; never
   edit a landed migration.
5. **Auth.** New REST routes are protected by the existing
   `AuthedUser` extractor unless they belong to the unauthenticated
   set in `DOCS/AUTH.md` (health, claim/status, claim).
6. **Ring-buffer pruning** runs inside the same transaction as the
   insert, per `SCOPE.md` §7.
7. **Comments explain *why*, not *what*.** No emojis, no
   task-status comments ("added in goal 4", "TODO from phase 1.5"),
   no decorative banners, no restatements of obvious code. The test:
   would a brand-new agent reading the file with no chat history
   understand *why* this code is shaped this way? If yes, the
   comment earns its place.
8. **No drive-by refactors.** A bug fix doesn't need cleanup. A
   one-shot change doesn't need a helper.
9. **Don't push to `origin` without being asked.** Commits stay
   local until the operator requests a push.
10. **Don't `--force`, don't `--no-verify`, don't rebase published
    history.** If a hook fails, fix the cause.

## What this repo is *not*

- Not codeless. There is no JOB-LOOP here, no mani, no `./bin/mani`,
  no React UI, no four-shell architecture, no R1/R2/R3 dependency
  rules. Those belong to the parent `codeless-workspace`.
- Not a multi-tenant SaaS in v0.1 — multi-org isolation landed in
  Phase 4 (Goal 7), but the trust model is still single-operator.
- Not a generic ngrok replacement — devices must be on the Zenoh
  fabric.
- Not a long-term TSDB — `events` is a bounded ring buffer.
- Not a general broker — `cmd_outbox` and `events` have hard caps.
