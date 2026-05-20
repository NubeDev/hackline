# Architecture

The full design is in [`SCOPE.md`](../SCOPE.md). This page is a
quick map for someone landing in the repo cold.

## Picture

```
                                                  cloud (VPS)
                                            ┌─────────────────────────┐
   browser ──HTTPS──► device-N.cloud.com ──►│ Caddy (TLS, ACME)       │
                                            │  ▼                      │
                                            │ hackline-gateway (axum) │
                                            │  • REST /v1/*           │
                                            │  • SSE  /v1/events      │
                                            │  • TCP  listeners       │
                                            │  • SQLite (control DB)  │
                                            │  • Zenoh client         │
                                            └────────────▲────────────┘
                                                         │ Zenoh queries
                                            ┌────────────▼────────────┐
                                            │   Zenoh router(s)       │
                                            └────────────▲────────────┘
                                                         │
                              edge device                │
                          ┌──────────────────────────────▼──┐
                          │ hackline-agent (Rust)           │
                          │   queryable                     │
                          │     hackline/<zid>/tcp/<port>   │
                          │   bridges → 127.0.0.1:<port>    │
                          ├─────────────────────────────────┤
                          │ Gin server :8080 (React app)    │
                          │ sshd        :22                 │
                          └─────────────────────────────────┘
```

## Crates

| Crate | Role | iOS-safe | Depends on |
|---|---|---|---|
| `hackline-proto` | Wire types | yes | (nothing) |
| `hackline-core` | TCP↔Zenoh bridge | yes (no spawn) | `hackline-proto`, `tokio`, `zenoh` |
| `hackline-agent` | Device binary | no | `hackline-proto`, `hackline-core` |
| `hackline-gateway` | Cloud lib + binary | no | `hackline-proto`, `hackline-core`, axum, rusqlite |
| `hackline-cli` | CLI | yes | `hackline-proto`, reqwest |

The agent and gateway never depend on each other; everything they
share lives in `hackline-core`.

## Data plane

One TCP connection ↔ one Zenoh exchange between gateway and agent.
The exact Zenoh API shape (streaming-reply query vs. paired
`<request_id>/{up,down}` pub/sub) is being validated in Phase 1; see
[`SCOPE.md` §11.1](../SCOPE.md) and [`KEYEXPRS.md`](./KEYEXPRS.md).

## Trust boundary

- Device → fabric: Zenoh ZID + Zenoh ACL.
- Gateway → device: gateway is one privileged Zenoh principal
  authorised to query `hackline/*/**`.
- User → gateway: bearer token (claim flow → owner token → scoped
  user tokens).

See [`AUTH.md`](./AUTH.md).
