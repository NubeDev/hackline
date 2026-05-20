# hackline

Zenoh-native remote-access service for IoT fleets. Per-device HTTP and
TCP endpoints exposed through a cloud gateway; each device runs a small
Rust agent that bridges Zenoh queries to its own loopback services.

> **Read these in order before writing code:**
>
> 1. [`SCOPE.md`](./SCOPE.md) — the load-bearing design doc.
> 2. [`DECISIONS.md`](./DECISIONS.md) — what was rejected and why.
> 3. [`HOW-TO-ADD-CODE.md`](./HOW-TO-ADD-CODE.md) — file layout rules.
> 4. [`INTEGRATION-RUBIX.md`](./INTEGRATION-RUBIX.md) — first-consumer
>    contract.

## Layout

```
crates/
  hackline-proto/     wire types, key-expression builders
  hackline-core/      TCP <-> Zenoh bridging helpers
  hackline-agent/     device-side binary
  hackline-gateway/   cloud-side library + binary
  hackline-cli/       hackline CLI
DOCS/
  ARCHITECTURE.md     picture + dependency rules
  REST-API.md         per-endpoint contracts
  KEYEXPRS.md         Zenoh key-expression catalogue
  CLI.md              CLI subcommand reference
  CONFIG.md           gateway + agent config files
  AUTH.md             claim flow + tokens
  DATABASE.md         SQLite schema + migrations
  DEVELOPMENT.md      local dev loop
  sessions/           per-session work logs
```

## Quick start

```sh
cargo check --workspace
cargo test  --workspace
```

The crates are scaffolded empty. See [`HOW-TO-ADD-CODE.md`](./HOW-TO-ADD-CODE.md)
for where new code goes.
