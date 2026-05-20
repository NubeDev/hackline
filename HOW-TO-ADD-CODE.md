# HOW TO ADD CODE — hackline

Single entry point for any coding session in this repo. Read once at
the start of the session; the rules below are enforced in review.

---

## RULE ZERO — one responsibility per file

| Limit | Value |
|---|---|
| Max lines per file | **400** |
| Max lines per function | **50** |
| Max public items per module | **~10** |
| Max nesting depth | **4** |

When a file approaches **300 lines**, stop and ask: *what are the two or
more responsibilities living here?* Split before you hit 400, not after.

### Why

An AI loads context in fixed-size chunks. When `api/devices/get.rs` is
50 lines, the AI loads it and has 100 % of the relevant code for that
endpoint in context. When `devices.rs` is 800 lines covering list +
get + create + patch + delete + info + health, the AI burns context
window, risks editing the wrong handler, and produces a diff that
touches unrelated routes.

### Concrete shape

REST routes — one file per `(resource, verb)`:

```
api/
  devices/
    mod.rs        re-exports + router wiring
    list.rs       GET    /v1/devices
    create.rs     POST   /v1/devices
    get.rs        GET    /v1/devices/:id
    patch.rs      PATCH  /v1/devices/:id
    delete.rs     DELETE /v1/devices/:id
    info.rs       GET    /v1/devices/:id/info
    health.rs     GET    /v1/devices/:id/health
```

CLI subcommands — one file per leaf command:

```
cmd/
  device/
    mod.rs        clap subcommand wiring
    list.rs       hackline device list
    add.rs        hackline device add
    show.rs       hackline device show ID
    remove.rs     hackline device remove ID
```

Database — one file per table:

```
db/
  mod.rs          pool + transaction helpers
  users.rs
  devices.rs
  tunnels.rs
  audit.rs
  claim.rs
```

### Naming

| Never | Always |
|---|---|
| `utils.rs` | name the concept: `keyexpr.rs`, `token_hash.rs` |
| `helpers.rs` | name the concept: `peer_addr.rs`, `byte_copy.rs` |
| `common.rs` | move shared types to `hackline-proto`; name them |
| `misc.rs` | trash drawer — never |

---

## Crate dependency direction (load-bearing)

See [`SCOPE.md` §4 R1–R4](./SCOPE.md). Summary:

```
hackline-proto   ← pure types, no tokio, no zenoh, no fs
hackline-core    ← bridging helpers, depends on proto + tokio + zenoh
hackline-agent   ← bin; depends on proto + core
hackline-gateway ← lib + bin; depends on proto + core
hackline-cli     ← bin; depends on proto + reqwest (NOT core, NOT gateway)
```

Hard rules:

- **R1.** `hackline-proto` is pure types. No `tokio`, no `zenoh`, no
  filesystem.
- **R2.** `hackline-agent` and `hackline-gateway` do not depend on
  each other. Anything they share lives in `hackline-core`.
- **R3.** Only `*-cli`, `*-agent`, and `*-gateway` `main.rs` install a
  logging subscriber, parse argv, or call `std::process::exit`.
  Library code returns `Result<_,_>`.
- **R4.** SQLite lives **only** in `hackline-gateway`. The agent has no
  persistent state of its own.

If you're about to violate one of these, stop. Either you're in the
wrong crate, or `hackline-proto` / `hackline-core` needs widening.

---

## Where does my code go?

Walk top-to-bottom. Stop at the first match.

### Q1 — am I changing a wire-level type?

*Examples: a field on `ConnectRequest`, a new SSE event variant, a new
key-expression segment.*

→ **`crates/hackline-proto/src/`**, in the file that owns the concept
(`connect.rs`, `event.rs`, `keyexpr.rs`). If the concept is new, add a
new file.

Then update [`DOCS/KEYEXPRS.md`](./DOCS/KEYEXPRS.md) or the relevant
DOC in the same PR.

### Q2 — am I writing TCP↔Zenoh bridging code?

→ **`crates/hackline-core/src/bridge.rs`** or a sibling. This is the
only place `tokio::io::copy_bidirectional` over a Zenoh stream lives.

### Q3 — am I adding a REST endpoint?

→ **`crates/hackline-gateway/src/api/<resource>/<verb>.rs`**.

Handlers are thin: extract → call domain function → map to DTO →
return. If you're typing a loop, a multi-step predicate, or SQL
inside a handler, that logic belongs in `db/` or a new domain module.
The 20-line ceiling on handlers is the canonical smoke test.

### Q4 — am I adding a CLI subcommand?

→ **`crates/hackline-cli/src/cmd/<group>/<verb>.rs`**.

CLI is a thin client over `reqwest`. No business logic; if a command
needs a capability the REST surface doesn't expose, add the REST
endpoint first.

### Q5 — am I adding a SQL table or migration?

→ **`crates/hackline-gateway/migrations/V###__<name>.sql`** plus the
matching repository file in **`crates/hackline-gateway/src/db/`**.
One repository file per table. Update [`DOCS/DATABASE.md`](./DOCS/DATABASE.md).

### Q6 — am I adding agent-side Zenoh queryable handling?

→ **`crates/hackline-agent/src/`** — one file per queryable concept
(`info.rs`, `connect.rs`).

### Q7 — am I adding documentation?

- Architecture / contracts → **`DOCS/`**.
- Session work log → **`DOCS/sessions/<date>-<topic>.md`**.
- ADR-style decision → **`DECISIONS.md`** (one section, "overturn this
  if" clause).

### Still unsure?

Read [`SCOPE.md`](./SCOPE.md) end to end, then ask.

---

## Comment rules

1. **Doc-comment every public item.** What it is, when/why to use it,
   defaults, edge cases.
2. **Explain why, not what.** `// increment counter` above
   `counter += 1` is noise. An invariant or a rejected alternative is
   signal.
3. **No session-progress markers.** No `// STAGE-1 complete`, no
   `// FIXED:`, no `// previously this was X`. Comments describe the
   code as it is now, not its history. Track in-flight work in the
   session note.
4. **No emojis. No ASCII banners. No decoration.**
5. **`TODO` / `FIXME` always carry an owner or ticket.**
   `// TODO(alice): ...` or `// TODO(HACK-123): ...`. Never bare.
6. **Stale comment is worse than no comment.** Update with the same
   diff that changes the code.

---

## Test rules

- Test lives with the code. Same PR.
- One test file per source file: `src/api/devices/get.rs` →
  `tests/api/devices/get.rs` (or an inline `#[cfg(test)] mod tests`
  for unit tests; integration tests go under `tests/`).
- Loopback Zenoh router for any test that needs an end-to-end query.
  Don't mock the transport; mocking it just hides bugs.

---

## Workflow

Local commands (run from this directory):

```sh
cargo check --workspace
cargo test  --workspace
cargo fmt   --all
cargo clippy --workspace --all-targets -- -D warnings
```

There is no `make`, no `mani`, no orchestration. Everything is
`cargo`. If you find yourself wanting orchestration, stop — this
repo is small enough that scripts only add noise.

---

## Commit etiquette

- Conventional commits. `feat(gateway): …`, `fix(agent): …`,
  `docs: …`, `chore: …`.
- Commit only when the user asks. Never amend; always a new commit.
- One logical change per commit; the commit body explains the *why*.
