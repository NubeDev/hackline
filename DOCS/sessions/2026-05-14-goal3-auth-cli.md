# 2026-05-14 — Goal 3: Auth layer + CLI completion

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Implement db/claim.rs (pending insert, consume) + db/users.rs (CRUD + token-hash lookup) | [x] |
| 1 | Implement auth/token.rs (generate 32-byte token, SHA-256 hash, constant-time compare) | [x] |
| 2 | Implement auth/claim.rs (ensure_pending on startup) | [x] |
| 3 | Implement auth/middleware.rs (Bearer extractor → AuthedUser) | [x] |
| 4 | Implement API: claim/status, claim/post, users CRUD, users/mint_token | [x] |
| 5 | Wire all new routes into router.rs; protect existing routes with auth middleware | [x] |
| 6 | Update bin/serve.rs to seed claim on startup | [x] |
| 7 | Add sha2, subtle, rand, clap, reqwest, dirs workspace deps | [x] |
| 8 | Wire CLI: main.rs (clap), client.rs (reqwest), config.rs (credentials cache) | [x] |
| 9 | CLI commands: login, whoami, device {add,list,remove,show}, tunnel {add,list,remove} | [x] |
| 10 | cargo check --workspace | [x] |
| 11 | E2E: gateway start → claim → login → device add → tunnel add → verify via CLI | [x] |

## Outcome

Full auth layer and CLI working end-to-end. The flow:
1. Gateway starts, seeds claim token if unclaimed, prints it to stdout
2. `hackline login --server URL --token TOKEN` claims the gateway, caches bearer token
3. All subsequent CLI commands use cached credentials with Bearer auth
4. AuthedUser extractor validates every authenticated request via SHA-256 token hash lookup

Verified:
- Claim flow works (POST /v1/claim consumes pending, returns bearer)
- Whoami shows cached credentials
- Device CRUD: add, list, show, remove
- Tunnel CRUD: add, list, remove
- User CRUD: add (with token minting), list
- `cargo check --workspace` clean (2 pre-existing dead-code warnings in hackline-agent)
- `cargo test --workspace` passes

## Design

Goal 3 completes Phase 1 by adding the auth layer and a usable CLI.

**Auth flow:**
1. Gateway startup: if `users` table is empty, insert a `claim_pending` row with a random token hash. Print the raw claim token to stdout.
2. `hackline login --server URL --token TOKEN`: POST /v1/claim with the claim token. Gateway consumes pending, inserts owner user, returns a bearer token. CLI caches credentials.
3. Subsequent CLI calls send `Authorization: Bearer <token>`. Middleware validates by hashing and comparing against `users.token_hash`.

**CLI architecture:**
- Pure HTTP client (reqwest). No hackline-core or hackline-gateway dependency.
- Credentials cached at `$XDG_CONFIG_HOME/hackline/credentials.json`.
- Human-readable table output by default; `--json` flag for machine consumption.
