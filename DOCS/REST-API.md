# REST API

Authoritative reference for the hackline gateway HTTP surface. The
router lives in
[`crates/hackline-gateway/src/api/router.rs`](../crates/hackline-gateway/src/api/router.rs);
each handler file is named `<verb>.rs` under its resource folder so
this doc and the source map 1:1. Status of every endpoint is recorded
in the [Implementation status](#implementation-status) table at the
bottom.

## Conventions

- **Base URL.** `http(s)://<gateway-host>:<port>`. All routes are
  prefixed `/v1`. The version is part of the URL because the wire
  contract is what changes across releases — Cargo versions of the
  daemon do not.
- **Auth.** `Authorization: Bearer <token>` on every endpoint *except*
  `GET /v1/health`, `GET /v1/claim/status` and `POST /v1/claim`. The
  token model is in [`AUTH.md`](./AUTH.md).
- **Content type.** Requests and responses are `application/json; charset=utf-8`
  unless explicitly noted (SSE endpoints emit `text/event-stream`).
- **Timestamps.** All timestamps are integer seconds since the Unix
  epoch (`unixepoch()` in SQLite). UTC.
- **IDs.** Numeric primary keys (`i64`) for `devices`, `tunnels`,
  `users`. ZIDs are 16-hex-character Zenoh identifiers.
- **Errors.** Any non-2xx response has shape `{ "error": "<message>" }`.
  Status codes mapped by [`GatewayError`](../crates/hackline-gateway/src/error.rs):

  | Status | Variant | When |
  |---|---|---|
  | 400 | `BadRequest` | Validation failure (bad ZID, unknown patch field, missing required field). |
  | 401 | (middleware) | Missing or invalid bearer token. |
  | 403 | (middleware) | Token scope does not cover the resource. |
  | 404 | `NotFound` | Row does not exist (or caller cannot see it — same status, not 403, by design). |
  | 500 | `Config`/`Db`/`Pool`/`Io`/`Bridge`/`Zenoh`/`Proto` | Internal failure. The body is the literal string `"internal error"`; details are in the gateway log only. |

- **Pagination.** Cursor-based on `/v1/audit`. Offset pagination on
  the audit table is a footgun and is not supported.
- **Concurrency.** All writes are serialised through a single
  `r2d2::Pool<rusqlite::Connection>` backed by SQLite in WAL mode.

---

## Health

### `GET /v1/health`

Unauthenticated liveness probe. Used by Caddy/Docker/Kubernetes.

Response `200`:

```json
{ "status": "ok" }
```

---

## Claim (first-boot)

The claim flow turns a freshly-installed gateway (with a one-time
pending row in `claim_pending`) into an owned instance. See
[`AUTH.md`](./AUTH.md) for the threat model.

### `GET /v1/claim/status`

Unauthenticated. Tells a UI whether the gateway is ready to be claimed.

Response `200`:

```json
{
  "claimed": false,
  "can_claim": true
}
```

`claimed` is `true` once any `owner`-scoped user row exists.
`can_claim` is `true` iff a non-expired `claim_pending` row is in the
table.

### `POST /v1/claim`

Unauthenticated. Atomic consume-pending + insert-owner. The
`DELETE FROM claim_pending` and the `INSERT INTO users` happen in the
same SQL transaction — there is no window in which a stolen
`claim_token` can be reused.

Request:

```json
{
  "claim_token": "<one-time-token printed at install>",
  "label": "first-laptop"
}
```

Response `201`:

```json
{
  "user_id": 1,
  "token": "<raw-bearer-token-shown-once>",
  "scope": "owner"
}
```

Failure modes: `400` if `claim_token` does not match, `409` if the
gateway is already claimed.

---

## Devices

A *device* is a registered agent instance, addressed by its ZID. The
`devices` table is the index that the SSE event stream, the tunnel
manager, and the audit log all key on.

### `GET /v1/devices`

List devices visible to the caller. Owner sees all rows; a scoped user
sees only rows whose `customer_id` matches their assignment.

Response `200`:

```json
[
  {
    "id": 1,
    "zid": "a1b2c3d4e5f60718",
    "label": "edge-1",
    "customer_id": null,
    "created_at": 1715683200,
    "last_seen_at": 1715690400
  }
]
```

Empty list is `200 []`, not `404`.

### `POST /v1/devices`

Register a new device by ZID. The ZID itself is validated by
[`hackline_proto::zid`](../crates/hackline-proto/src/zid.rs); duplicates
are rejected by the table's `UNIQUE` constraint and surface as
`500 internal error` today (see [Implementation status](#implementation-status)).

Request:

```json
{
  "zid": "a1b2c3d4e5f60718",
  "label": "edge-1"
}
```

Response `201`: same shape as `GET /v1/devices/:id`.

### `GET /v1/devices/:id`

Fetch one device row.

Response `200`: same as a list element above. `404` if the id is
unknown.

### `PATCH /v1/devices/:id`

Mutate a device. Only `label` and `customer_id` are accepted; any other
field is rejected at the deserializer with `400`.

Request:

```json
{
  "label": "edge-1-renamed",
  "customer_id": 42
}
```

Both fields are optional; `customer_id: null` clears the assignment.

Response `200`: the updated row.

### `DELETE /v1/devices/:id`

Deletes the device row and cascades to `tunnels` via the foreign key.
No 2-phase confirm — call carries `Authorization` and that's enough.

Response `204` on success, `404` if the id is unknown.

### `GET /v1/devices/:id/info`

Issues a live Zenoh query to `hackline/<zid>/info` and returns the
agent's [`AgentInfo`](../crates/hackline-proto/src/agent_info.rs).

Response `200`:

```json
{
  "label": "edge-1",
  "allowed_ports": [22, 80, 443]
}
```

`504` if the agent does not answer the query within the gateway's
configured `info_query_timeout`.

### `GET /v1/devices/:id/health`

Liveness summary derived from the `last_seen_at` column and a single
liveness probe over Zenoh.

Response `200`:

```json
{
  "online": true,
  "last_seen_at": 1715690400,
  "rtt_ms": 14
}
```

`rtt_ms` is `null` when `online` is `false`.

---

## Tunnels

A *tunnel* is a gateway-side public listener (TCP) or HTTP host route
that forwards bytes/requests to a registered device. The active set is
managed by [`tunnel::manager`](../crates/hackline-gateway/src/tunnel/manager.rs).

### `GET /v1/tunnels`

List all tunnels visible to the caller.

Response `200`:

```json
[
  {
    "id": 1,
    "device_id": 1,
    "kind": "tcp",
    "local_port": 22,
    "public_hostname": null,
    "public_port": 2222,
    "enabled": true,
    "created_at": 1715683200
  }
]
```

### `POST /v1/tunnels`

Open a new public listener (`kind: "tcp"`) or register an HTTP host
route (`kind: "http"`).

Request:

```json
{
  "device_id": 1,
  "kind": "tcp",
  "local_port": 22,
  "public_hostname": null,
  "public_port": 2222
}
```

Validation invariants (enforced by a `CHECK` in the migration):

- `kind = "tcp"` ⇒ `public_port` required, `public_hostname` null.
- `kind = "http"` ⇒ `public_hostname` required, `public_port` null.

Any other combination produces `500` today (the `CHECK` fires inside
SQLite); a tighter `400` mapping is on the open-questions list.

Response `201`: the row, shape as in list. The tunnel manager picks
the new row up on its next reconcile pass and binds the listener.

### `DELETE /v1/tunnels/:id`

Closes the listener (TCP) or removes the host route (HTTP) and deletes
the row.

Response `204` on success, `404` if the id is unknown.

---

## Users

Users are bearer-token holders. Tokens are stored as Argon2id hashes;
the raw token is shown exactly once at mint time. Scopes are `owner` or
`customer:<customer_id>`.

### `GET /v1/users`

Owner-only. Returns every user row without token material.

Response `200`:

```json
[
  {
    "id": 1,
    "label": "first-laptop",
    "scope": "owner",
    "customer_id": null,
    "created_at": 1715683200,
    "last_used_at": 1715690400
  }
]
```

### `POST /v1/users`

Owner-only. Mint a new scoped user. The returned token is the only
copy — the gateway stores its hash only.

Request:

```json
{
  "label": "ops-readonly",
  "scope": "customer",
  "customer_id": 42
}
```

Response `201`:

```json
{
  "user_id": 2,
  "token": "<raw-bearer-token-shown-once>"
}
```

### `DELETE /v1/users/:id`

Owner-only. Revokes the user and all their tokens. Returns `204`. The
owner row cannot be deleted via this endpoint.

### `POST /v1/users/:id/tokens`

Mint an additional token for an existing user (rotation). Owner-only.
Old tokens for that user remain valid until they are individually
revoked — this endpoint adds, it does not replace.

Response `201`:

```json
{
  "token": "<raw-bearer-token-shown-once>"
}
```

---

## Audit

### `GET /v1/audit?cursor=<id>&limit=<n>`

Cursor-paginated read of the `audit` table. `cursor` is the `id` of
the last row in the previous page (exclusive); omit it on the first
request. `limit` defaults to 100, capped at 500. Rows are returned
oldest-first within a page.

Response `200`:

```json
{
  "items": [
    {
      "id": 7,
      "at": 1715683200,
      "actor_user_id": 1,
      "action": "tunnel.create",
      "subject": "tunnel:1",
      "detail": { "device_id": 1, "kind": "tcp", "public_port": 2222 }
    }
  ],
  "next_cursor": 7
}
```

`next_cursor` is `null` when the page is the last one.

Offset pagination is intentionally not supported: the audit table is
append-only and large, and `LIMIT … OFFSET …` over it is a footgun.

---

## Events (SSE)

Both event endpoints return `text/event-stream`. The wire encoding
follows the standard SSE framing — one `data:` line per event, blank
line as record separator, no `id:` field. Reconnect is the client's
responsibility; there is no replay.

Event payloads are the variants of
[`hackline_proto::event::Event`](../crates/hackline-proto/src/event.rs),
serialised with serde's `tag = "type"`, `rename_all = "snake_case"`:

```text
data: {"type":"device_online","device_id":1}

data: {"type":"device_offline","device_id":1}

data: {"type":"tunnel_opened","tunnel_id":7}

data: {"type":"tunnel_closed","tunnel_id":7}

data: {"type":"tunnel_connection","tunnel_id":7,"request_id":"7e57…"}
```

### `GET /v1/events`

Admin-only firehose: every event for every device.

### `GET /v1/devices/:id/events`

Scoped to a single device: only events whose `device_id` or owning
tunnel's `device_id` matches `:id`.

**Caddy / reverse proxy.** The Caddyfile must include
`flush_interval -1` for `/v1/events` and `/v1/devices/*/events` —
otherwise the proxy buffers, and clients see a broken feed where
events arrive in clumps minutes apart.

---

## Implementation status

The router currently wires the rows marked **live**. The remaining
rows are documented here so the wire contract is fixed before the
handlers ship; their handler files exist as stubs with a doc comment.

| Method | Path | Status |
|---|---|---|
| GET | `/v1/health` | live |
| GET | `/v1/claim/status` | stub |
| POST | `/v1/claim` | stub |
| GET | `/v1/devices` | live |
| POST | `/v1/devices` | live |
| GET | `/v1/devices/:id` | live |
| PATCH | `/v1/devices/:id` | stub |
| DELETE | `/v1/devices/:id` | live |
| GET | `/v1/devices/:id/info` | stub |
| GET | `/v1/devices/:id/health` | stub |
| GET | `/v1/tunnels` | live |
| POST | `/v1/tunnels` | live |
| DELETE | `/v1/tunnels/:id` | live |
| GET | `/v1/users` | stub |
| POST | `/v1/users` | stub |
| DELETE | `/v1/users/:id` | stub |
| POST | `/v1/users/:id/tokens` | stub |
| GET | `/v1/audit` | stub |
| GET | `/v1/events` | stub |
| GET | `/v1/devices/:id/events` | stub |

Changes to the wire shape of any **live** row require a `/v2` prefix.
The **stub** rows may still move — but only until they ship.
