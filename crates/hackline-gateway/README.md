# hackline-gateway

Cloud-side gateway. axum REST + SSE, TCP / HTTP listeners for forwarded
ports, SQLite for control state, Zenoh client for talking to agents.

Library + binaries. The library is the testable surface; the binaries
are thin entry points.

## Layout

```
src/
  lib.rs                  re-exports
  state.rs                AppState (db pool, zenoh session, events bus)
  config.rs               TOML loader
  error.rs                gateway error type

  bin/
    serve.rs              hackline-gateway serve
    reset_claim.rs        hackline-gateway reset-claim
    print_claim.rs        hackline-gateway print-claim

  auth/
    mod.rs
    token.rs              hashing + constant-time compare
    middleware.rs         axum extractor / guard
    claim.rs              first-boot claim flow
    scope.rs              device_scope / tunnel_scope check

  api/                    one file per (resource, verb)
    mod.rs
    router.rs             wires every handler into the axum Router
    health.rs             GET /v1/health
    claim/
      mod.rs
      status.rs           GET  /v1/claim/status
      post.rs             POST /v1/claim
    devices/
      mod.rs
      list.rs             GET    /v1/devices
      create.rs           POST   /v1/devices
      get.rs              GET    /v1/devices/:id
      patch.rs            PATCH  /v1/devices/:id
      delete.rs           DELETE /v1/devices/:id
      info.rs             GET    /v1/devices/:id/info
      health.rs           GET    /v1/devices/:id/health
    tunnels/
      mod.rs
      list.rs             GET    /v1/tunnels
      create.rs           POST   /v1/tunnels
      delete.rs           DELETE /v1/tunnels/:id
    users/
      mod.rs
      list.rs             GET    /v1/users
      create.rs           POST   /v1/users
      delete.rs           DELETE /v1/users/:id
      mint_token.rs       POST   /v1/users/:id/tokens
    audit/
      mod.rs
      list.rs             GET    /v1/audit
    events/
      mod.rs
      all.rs              GET /v1/events
      per_device.rs       GET /v1/devices/:id/events

  db/                     one file per table
    mod.rs                pool + spawn_blocking helpers
    pool.rs               r2d2 setup, WAL pragma
    migrations.rs         refinery runner
    users.rs
    devices.rs
    tunnels.rs
    audit.rs
    claim.rs

  tunnel/                 the data-plane half
    mod.rs
    manager.rs            opens / closes listeners from db rows
    tcp_listener.rs       per-tunnel TCP listener
    http_router.rs        Host: header → tunnel lookup
    bridge.rs             glue between a TCP socket and hackline-core

  zenoh_client.rs         the gateway's single Zenoh session
  events_bus.rs           in-process broadcast feeding SSE handlers

migrations/
  V001__init.sql
```

The router file (`api/router.rs`) is the only place that knows the
full URL surface. Every handler file is small enough that the AI
loads it whole and has the entire endpoint in context.
