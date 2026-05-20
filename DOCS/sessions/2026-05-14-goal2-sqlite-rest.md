# 2026-05-14 — Goal 2: SQLite persistence + REST API

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Add rusqlite, r2d2, r2d2_sqlite, axum, tower-http workspace deps | [x] |
| 1 | V001__init.sql migration (devices, tunnels, audit, users, claim tables) | [x] |
| 2 | db/pool.rs — r2d2 pool with WAL + FK pragmas | [x] |
| 3 | db/migrations.rs — idempotent migration runner | [x] |
| 4 | db/devices.rs — Device CRUD (insert/list/get/delete) | [x] |
| 5 | db/tunnels.rs — Tunnel CRUD + list_active_tcp join | [x] |
| 6 | API handlers: health, devices CRUD, tunnels CRUD | [x] |
| 7 | api/router.rs — axum Router wiring | [x] |
| 8 | AppState, GatewayError IntoResponse | [x] |
| 9 | tunnel/manager.rs — load active tunnels from DB, spawn listeners | [x] |
| 10 | bin/serve.rs — DB pool + migrations + REST server + tunnel manager | [x] |
| 11 | cargo check + test | [x] |
| 12 | E2E: REST CRUD via curl | [x] |
| 13 | E2E: DB-driven TCP bridge (create via REST, restart, netcat echo) | [x] |

## Outcome

Full SQLite persistence and REST API working. The gateway:
1. Opens an r2d2 connection pool with WAL mode, FK enforcement, 5s busy timeout
2. Runs migrations on startup (tracked in `_migrations` table)
3. Serves REST API on the configured address (axum 0.8)
4. Tunnel manager reads active TCP tunnels from DB at startup, spawns TCP listener per tunnel

Verified:
- All REST endpoints return correct JSON (health, devices CRUD, tunnels CRUD)
- TCP bridge works through DB-created tunnels (create device+tunnel via REST, restart gateway, netcat echo succeeds)
- `cargo check --workspace` passes (2 dead-code warnings, both expected)
- `cargo test --workspace` passes (3 proto tests)

Known limitations:
- Tunnel manager loads at startup only — tunnels created via REST after boot need a gateway restart
- `r2d2: database is locked` / `disk I/O error` at startup is non-fatal (pool init vs migration race)
