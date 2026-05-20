# hackline-agent

Device-side binary. Declares one Zenoh queryable per whitelisted local
port; on each query, opens a `127.0.0.1:port` TCP connection and
hands bridging to `hackline-core`.

No SQLite, no REST, no auth code of its own — Zenoh ACLs are the only
gate. (SCOPE R4.)

## Files

| File | Owns |
|---|---|
| `main.rs` | argv, logging subscriber, run loop |
| `config.rs` | TOML config loader, port whitelist |
| `info.rs` | `hackline/<zid>/info` queryable handler |
| `connect.rs` | `hackline/<zid>/tcp/<port>/connect` queryable handler |
| `liveliness.rs` | `hackline/<zid>/health` liveliness token |
| `error.rs` | agent-level error type |

The split mirrors `DOCS/KEYEXPRS.md`: one source file per
key-expression family.
