# 2026-05-14 — Goal 7: Multi-tenant orgs

## Plan

| # | Step | Status |
|---|---|---|
| 1 | V005 migration: `orgs` table seeded with `default` (id=1), `org_id` non-null FK on `users` and `devices` backfilled to id=1, indexes on both org_id columns | [x] |
| 2 | `db::orgs` repository: `insert`, `list`, `get`, `get_by_slug`, slug validation matching the migration's CHECK; `DEFAULT_ORG_ID` / `DEFAULT_ORG_SLUG` consts so non-org-aware sites have a single source of truth | [x] |
| 3 | `User` carries `org_id`; `AuthedUser` exposes it through the existing extractor; every per-tenant DB function grew an `_in_org` variant (`devices::{get,list,delete}_in_org`, `users::{list,delete}_in_org`, `tunnels::{list,delete}_in_org`) | [x] |
| 4 | Every REST handler that scopes by device or user filters by `caller.org_id` first (`api/devices/*`, `api/users/*`, `api/tunnels/*`, `api/cmd/*`, `api/api_call/call`, `api/audit/list`, `api/events/*`, `api/logs/*`); cross-org rows surface as `GatewayError::NotFound` → 404, never 403 | [x] |
| 5 | Keyexpr prefix becomes `hackline/<org_slug>/<zid>/...` across both planes; `hackline-proto::keyexpr` rewrites every builder to take `org`; fan-in subscriptions become `hackline/*/*/msg/{event,log,cmd-ack}/**`; `parse_msg_cmd_ack_keyexpr` returns `(org, zid, cmd_id)` | [x] |
| 6 | `ClientSession::from_session(session, org, zid)` and `from_session_auto(session, org)` — SDK rejects publish outside its org's prefix; `hackline-agent` reads `org_slug` from config and threads it through `connect`; `hackline-core::bridge` queryable + spike example updated | [x] |
| 7 | Gateway-side fan-in (`msg_fanin`, `events_bus`, `cmd_delivery`) parses the org out of incoming keyexprs, resolves the device by `(org_slug, zid)`, and stamps the `org_id` on every persisted event / log / cmd-ack | [x] |
| 8 | Gateway-side publish (`cmd_delivery::drain_pending`, `api_call::call`, `tunnel::tcp_listener`, `tunnel::http_router`) resolves each device's org slug via the new `devices::get_with_org_slug` (single SQL hop joining `orgs`) and composes the org-aware keyexpr | [x] |
| 9 | `TunnelWithZid` carries `org_slug`; the manager spawn helper, `POST /v1/tunnels`, and the active-tcp listing all source it from the existing `JOIN orgs` instead of a second hop | [x] |
| 10 | REST surface: `POST /v1/orgs` (owner-only), `GET /v1/orgs` (owner-only), `GET /v1/orgs/me` (any caller); claim flow `POST /v1/claim` accepts an optional `org` slug, allocating it inside the same transaction and stamping the owner | [x] |
| 11 | CLI: `hackline org create | list | inspect`; `hackline login --org <slug>` carries the slug into the claim request; `Client` config caches the slug for display only (server still authoritative off the bearer) | [x] |
| 12 | Cross-org isolation integration test: two orgs, one device + one user + one tunnel each, every `_in_org` DB entry point that handlers call refuses cross-org access (`tests/org_isolation.rs`); also covers `audit::list_recent` cross-org filter | [x] |
| 13 | SCOPE.md §13 Phase 4: replace the two-bullet sketch with the full spec (orgs table shape, claim flow, REST surface, keyexpr prefix, the 404-not-403 leak-the-minimum rule, out-of-scope items deferred to Phase 5) | [x] |
| 14 | `cargo check --workspace` clean; `cargo test --workspace` green | [x] |

## Outcome

End-to-end Phase 4 demoable.

`cargo check --workspace` — clean; the same two pre-existing dead-code
warnings in `hackline-agent` (`label`, `PortDenied`) remain, no new
warnings.

`cargo test --workspace` — every prior suite plus the new
isolation suite is green:

```
hackline-proto: keyexpr parse + render, msg round-trips
hackline-client: topic_validation
hackline-gateway unit: auth::scope, http_router::host_header_*, events::stream::globs
gateway tests/cmd_plane.rs   (cmd_round_trip, api_round_trip)
gateway tests/message_plane.rs (event_round_trip, log_round_trip)
gateway tests/org_isolation.rs (cross_org_isolation,
                                cross_org_users_isolated,
                                cross_org_tunnels_and_audit_isolated)
```

Manual demo (single gateway, two orgs):

```bash
# Terminal 1 — gateway
cargo run -p hackline-gateway --bin serve -- gateway.toml

# Terminal 2 — owner claims the seeded `default` org and an `acme` org
hackline login                                          # default org
DEFAULT_TOK=$(cat ~/.config/hackline/token)
hackline reset-claim                                    # demo only
hackline login --org acme                               # creates acme
ACME_TOK=$(cat ~/.config/hackline/token)

hackline org list                                       # both orgs
hackline org inspect                                    # the caller's own

# Two devices, one per org
HACKLINE_TOKEN=$DEFAULT_TOK hackline device add --zid aa11 --label default-dev
HACKLINE_TOKEN=$ACME_TOK    hackline device add --zid bb22 --label acme-dev

# Cross-org reads return 404 (not 403)
DEFAULT_DEV_ID=$(HACKLINE_TOKEN=$DEFAULT_TOK hackline device list --json |
                 jq '.[0].id')
ACME_DEV_ID=$(HACKLINE_TOKEN=$ACME_TOK hackline device list --json |
              jq '.[0].id')

curl -s -o /dev/null -w "%{http_code}\n" \
     -H "Authorization: Bearer $DEFAULT_TOK" \
     http://127.0.0.1:8080/v1/devices/$ACME_DEV_ID
# => 404

curl -s -o /dev/null -w "%{http_code}\n" \
     -H "Authorization: Bearer $ACME_TOK" \
     http://127.0.0.1:8080/v1/devices/$DEFAULT_DEV_ID
# => 404

# Each owner only sees their own devices
HACKLINE_TOKEN=$DEFAULT_TOK hackline device list   # one row, aa11
HACKLINE_TOKEN=$ACME_TOK    hackline device list   # one row, bb22

# Keyexprs are namespaced per-org; the agent for `acme` publishes
# under hackline/acme/bb22/... and the default org's gateway
# subscriber on hackline/default/*/msg/event/** never sees it
# (verified by /v1/events scoped to each org).
```

## Design

**One default org seeded by the migration; everything backfilled to
it.** SCOPE.md §13 Phase 4 calls out a single `default` org so
existing single-tenant deployments keep working without an explicit
claim-with-org. V005 inserts the `default` row with id=1 *before*
the `ALTER TABLE ... ADD COLUMN org_id NOT NULL DEFAULT 1` runs, so
every existing user and device is implicitly stamped into it. The
`DEFAULT_ORG_ID` / `DEFAULT_ORG_SLUG` consts in `db::orgs` give
non-org-aware call sites (claim default, the spike example) a single
spelling.

**Cross-org reads return 404, never 403.** SCOPE.md left this
undecided. The handler-level shape that fell out: every
per-tenant query is `_in_org(conn, org_id, id)` returning
`GatewayError::NotFound` if the row is absent *or* in another org.
The handler maps `NotFound` to `404` via the existing
`IntoResponse` impl. Returning `403` would have leaked the
existence of cross-org ids — an attacker with one org's bearer
could enumerate device ids across the whole gateway by counting
which ids return 403 vs 404. 404 across the board means the
status code carries one bit: "no row visible to you," which is
exactly the SCOPE.md §6.2 minimum-disclosure rule applied to the
new tenant boundary.

**`org_id` lives on `User` and propagates through `AuthedUser`,
not as a separate extractor.** Adding a second extractor
(`AuthedOrg`, etc.) was tempting because the org check is logically
distinct from the user check. Rejected: the bearer already names a
user, the user already names exactly one org, so the org id is a
zero-cost field on the existing struct. A second extractor would
have re-queried the DB on every request. Handlers read
`caller.org_id` and pass it into the `_in_org` repository call — one
new line per handler, no new middleware.

**Keyexpr migrates to `hackline/<org>/<zid>/...` everywhere at once.**
SCOPE.md §13 Phase 4 mandates the prefix change for ACL reasons
(per-org Zenoh grants need a per-org subtree). Every keyexpr
builder in `hackline-proto::keyexpr` was rewritten to take `org`,
plus the gateway-side fan-in subscriptions became
`hackline/*/*/msg/{event,log,cmd-ack}/**`. The two-wildcard fan-in
keyexpr is the load-bearing piece: one less wildcard and the
gateway misses every cross-org publish; one more and it accidentally
matches keys outside `msg/`. The proto crate is the only file in
the workspace that knows the wire shape, so a future v2 prefix
change is a single-file diff.

**Claim flow inserts the org in the same transaction as the
owner.** Atomic — either the bootstrap fails entirely or the
operator gets a usable token plus a populated org row. The CLI
`hackline login --org acme` carries the slug; if absent, the owner
lands in the seeded `default` org. The response echoes the slug so
`hackline whoami` / `hackline org inspect` can render it locally,
but the server still resolves org membership from the token at
request time — the cached slug is display-only, never an auth input.

**Owner-only `POST /v1/orgs` and `GET /v1/orgs`; per-caller
`GET /v1/orgs/me`.** Three endpoints, two access tiers. The owner
can enumerate every org on the gateway because operating across
orgs (e.g. cross-tenant support work) is the v0.1 reality before
per-org owner tokens land. `/v1/orgs/me` is the per-tenant safe
read so the admin UI can render the caller's own org without
leaking the rest of the operator's customer list. Cross-org user
provisioning (mint a token for a user in a *different* org from the
caller) is deliberately not in this stage — the owner already has
that power via separate per-org bearer tokens, and adding an
`org_id` parameter to `POST /v1/users` would have widened the
auth-matrix surface in the same commit as the table changes.

**`devices::get_with_org_slug` is one SQL hop with a join, not two
hops.** Background loops (`cmd_delivery::drain_pending`,
`api_call::call`) need both the device row and the org slug to
build the right keyexpr. Looking those up separately would have
doubled the SQLite load on the cmd-pusher hot path. The new
function joins `devices` to `orgs` once and returns
`(Device, String)`. `TunnelWithZid` got the same treatment for
the tunnel listener — the `JOIN orgs` is folded into the active-tcp
listing query so the manager has the slug at spawn time, no
re-hop on accept.

**V005 forgot to register itself in `migrations::run`'s table.**
Caught by `cmd_round_trip` hanging — the test's in-memory DB never
created the `orgs` table, so `devices::get_with_org_slug`'s join
returned `no such table: orgs`, the cmd was never published, and
the device-side handler awaited a Zenoh sample that never arrived.
Fixed in this stage: `MIGRATIONS` now lists V005 alongside V001-V004.
Lesson recorded in the migrations file's doc comment — landing a
new `Vnnn__*.sql` requires a matching entry in the `MIGRATIONS`
slice, and the integration tests are the ones that catch the
omission, not unit tests.
