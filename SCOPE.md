# hackline — SCOPE

> A Zenoh-native fleet service for IoT edge devices. Two planes on
> one fabric:
>
> - **Tunnel plane** — bytes. Per-device HTTP and TCP reachable from
>   the cloud (Studio, customer browser, support SSH).
> - **Message plane** — typed envelopes. Events (device→cloud),
>   commands (cloud→device, durable), RPC (`api/*` req/reply), logs
>   (device→cloud).
>
> One gateway, one CLI, one device-side SDK, one SQLite, one auth
> token. Built on Zenoh because every device already runs Zenoh.

This scope is **load-bearing**. If code disagrees, fix the doc or fix
the code in the same change — never let them drift.

---

## 1. Problem statement

We ship IoT devices to clients. We need:

- **Operator/support** to reach a device's local admin UI (axum/Gin
  server + React) and SSH from anywhere, without the customer opening
  ports — *tunnel plane*.
- **End-customer** to reach the same admin UI from a browser via
  `https://device-1234.cloud.example.com` after logging in — *tunnel
  plane*.
- **Cloud control plane** to receive telemetry from every device,
  push commands (install block, reboot, reload config) that survive
  the device being offline, and run typed RPC against the device for
  things that don't fit a REST surface — *message plane*.
- Per-device identity, runtime revocation, audit log — table stakes.

We already run **Zenoh** on every device. Identity, encryption, ACL,
NAT traversal are solved. What's missing is the application layer:
the byte tunnels, the message conventions, and the cloud-side
control plane (claim, login, hostname mapping, command outbox, event
storage, audit, admin UI).

This project is that layer. Nothing more.

---

## 2. Non-goals

- Not a generic ngrok replacement for the public internet — devices
  must be on our Zenoh fabric.
- Not a second overlay network. Zenoh is the overlay; we are an
  application on top of it.
- Not a multi-tenant SaaS in v0.1. One operator org, many devices,
  many users-within-the-org. Cross-org isolation is Phase 4.
- Not UDP. Not in v0.1.
- No L3 VPN. Per-port, per-host only.
- No TLS termination on the device. The device serves plain HTTP on
  `127.0.0.1`. Public TLS is the gateway's job.
- Not a long-term timeseries store. The `events` table is a bounded
  ring per device for "what just happened." Customers wanting a real
  TSDB pipe events into theirs.
- Not a general broker. The `cmd_outbox` and `events` tables have
  hard caps. Anything wanting unbounded queues uses NATS or Kafka,
  not us.

---

## 3. Architecture

### 3.1 The picture

```
                                                  cloud (our VPS)
                                            ┌─────────────────────────┐
   browser ──HTTPS──► device-N.cloud.com ──►│ Caddy (TLS, ACME)       │
                                            │  ▼                      │
                                            │ hackline-gateway (axum) │
                                            │  • REST /v1/*           │
                                            │  • SSE /v1/events       │
                                            │  • TCP listeners        │
                                            │  • SQLite               │
                                            │  • Zenoh client         │
                                            └────────────▲────────────┘
                                                         │ Zenoh
                                                         │ (mTLS / QUIC)
                                                         │
                       Tunnel plane                      │   Message plane
                       hackline/<zid>/tcp/<port>/...     │   hackline/<zid>/msg/...
                                                         │
                       All queries originate from        │   Events flow up,
                       the gateway — devices never       │   commands flow down
                       dial out, which is what lets      │   with a durable
                       them sit behind NAT.              │   outbox in SQLite.
                                                         │
                                            ┌────────────▼────────────┐
                                            │   Zenoh router(s)       │
                                            └────────▲───────▲────────┘
                                                     │       │
                          edge device (Pi)           │       │
                          ┌──────────────────────────┴───────┴─────┐
                          │ hackline-agent (own Zenoh Session)     │
                          │   tunnel queryables + pub/sub          │
                          │     hackline/<zid>/tcp/<port>/...      │
                          ├────────────────────────────────────────┤
                          │ device app A (own Zenoh Session)       │
                          │   uses hackline-client SDK             │
                          │     publishes hackline/<zid>/msg/event │
                          │     serves    hackline/<zid>/msg/api   │
                          │     subscribes hackline/<zid>/msg/cmd  │
                          ├────────────────────────────────────────┤
                          │ device app B (own Zenoh Session)       │
                          │   ...                                  │
                          ├────────────────────────────────────────┤
                          │ Gin / axum server :8080 (admin UI)     │
                          │ sshd            :22                    │
                          └────────────────────────────────────────┘
```

### 3.2 The two planes

| | Tunnel plane | Message plane |
|---|---|---|
| Carries | Raw TCP bytes (HTTP, SSH, anything) | Typed JSON envelopes |
| Shape | One Zenoh `get` (`Open`) + per-request pub/sub on side channels | Pub/sub for events/logs, queryables for `api/*` RPC, durable cmd outbox |
| Latency target | < 50 ms RTT add over the underlying fabric | < 100 ms p50 RTT for `api/*`; events are best-effort live |
| Durability | None — TCP says "connection failed", retry | Cmd outbox is durable; events buffered last-N |
| Failure mode | Tunnel drops, client retries | Pub/sub drops while offline; cmd queues up; api fails fast |
| Owned by | `hackline-agent` daemon | Device apps via `hackline-client` SDK |

The two planes share Zenoh, share the gateway process, share SQLite,
and share the bearer-token auth at the gateway edge. They do **not**
share Zenoh sessions on the device — see §3.5.

### 3.3 The four hackline products (what the user actually sees)

Hackline is **standalone**. Consumers (rubix today, others later)
depend only on `hackline-client` + `hackline-proto`. The contract is
the wire, not the SDK. Full integration rules and migration sequence
for the first consumer are in
[`INTEGRATION-RUBIX.md`](./INTEGRATION-RUBIX.md).


| Product | Where it runs | Distributed as |
|---|---|---|
| `hackline-gateway` | Cloud VPS, behind Caddy | Single static binary + systemd unit + sample Caddyfile |
| `hackline-agent` | Every device | Single static binary + systemd unit |
| `hackline` (CLI) | Operator/support workstations | Single binary; Homebrew, apt, direct download |
| `hackline-client` (SDK) | Linked into device apps (rubix-agent, etc.) | `crates.io` (Rust), later `npm` (Zenoh-WS), later `pub.dev` |

Plus one supporting library:

| Library | Purpose |
|---|---|
| `hackline-proto` | Wire types only. Pure, no tokio, no zenoh. Anything that needs the schema (TS-codegen, docs tools) depends on this. |

### 3.4 Why Zenoh, not bore/wstunnel/frp/OpenZiti/NATS

Already on every device. Already does identity, encryption, ACL, NAT
traversal, and pub/sub-with-wildcards. Adding a second overlay (or a
second message broker) would mean a second secret to rotate, a second
port to firewall, a second metric to monitor — for something Zenoh
already provides. See [`DECISIONS.md`](./DECISIONS.md) entry
`tunnel-engine — 2026-05` and `message-broker — 2026-05`.

### 3.5 Trust model

- **Device → fabric.** Zenoh ZID + Zenoh ACL. Each `Session` on the
  device (one for `hackline-agent`, one per device app using
  `hackline-client`) gets its own ZID under the same per-device cert
  chain. Sessions are permitted only to **serve / publish** keyexprs
  under `hackline/<own-zid>/**`. They must **not** be granted query
  or subscribe rights against `hackline/**` — otherwise a compromised
  app could enumerate or attack peers' loopbacks. Both `hackline-agent`
  and `hackline-client::Session::open` perform a startup self-check
  and fail closed if their ACL grants more than serve+publish on
  `hackline/<own-zid>/**`.
- **App-vs-app on a single device is not a security boundary.** Apps
  on the same device share a trust boundary; Zenoh ACL per-app is for
  blast-radius hygiene, not isolation. A hostile process on the
  device can read `/etc/hackline/device.pem` regardless of process
  separation.
- **Gateway → device.** Gateway is a Zenoh client/peer with ACL
  permitting it to query/subscribe `hackline/*/**` and pub/sub on
  per-request side channels. **The gateway is a privileged single
  point of compromise by design** — anyone who owns the gateway owns
  every device's loopback and message plane. Zenoh ACLs prevent
  device-to-device lateral movement, **not** gateway-to-device. This
  is acceptable because the gateway is the smallest, most-audited
  surface in the system; protect it accordingly (network ACLs,
  hardened host, key-on-HSM if you can).
- **User → gateway.** Bearer token claim flow lifted from
  [`token-service`](../../rubix-workspace/token-service) (see §6).
- **End-customer → gateway.** Same bearer-token flow, scoped to the
  device(s) they own (Phase 2).

There is no shared fleet secret. Compromising one device leaks
nothing about any other device. Revoking a device = pulling its
Zenoh ACL entries for all sessions sharing its cert chain and
deleting its row in `devices`.

### 3.6 Why each device app opens its own Zenoh session

Apps on the device do **not** proxy through `hackline-agent`. They
each open their own `zenoh::Session` via `hackline-client`.

Reasons (recorded in `DECISIONS.md` `client-session-model`):

1. Zenoh is built for this — multi-session-per-process is the
   documented pattern.
2. Proxying through the agent would force us to design a local IPC
   protocol, a fan-out multiplexer, and a second auth layer that all
   reinvent what Zenoh already does.
3. `hackline-agent` restarting (config change, crash, upgrade) would
   blackhole every app's message plane. Own-session means
   `systemctl restart hackline-agent` only drops byte tunnels.
4. The agent stays a thin Zenoh-to-loopback bridge with no IPC
   server, no auth check on local connections, no fan-out logic.
5. Matches how rubix-agent already opens its Zenoh session today — the
   migration is "swap the keyexpr namespace and the trait import."

Cost: every device app needs Zenoh credentials. Mitigation: one cert
chain per device at `/etc/hackline/device.pem` (mode 0640, group
`hackline`); ops adds an app to the group when installing it. Not
treated as an app-vs-app security boundary (see §3.5).

### 3.7 Constrained-device clients (ESP32 and friends)

Not every device on the fabric is a Linux box running
`hackline-agent`. We expect a meaningful population of **small
microcontroller-class devices — ESP32s and similar — that join the
Zenoh fabric directly as Zenoh clients** and participate in the
message plane only (events, logs, cmd, optionally `api/*`). They do
**not** run `hackline-agent`, do not host TCP tunnels, and have no
local filesystem story worth speaking of.

This is a first-class supported shape, not an afterthought. The
design implications:

- **Tunnel plane is N/A on these devices.** No `hackline-agent`, no
  loopback to expose, no `Open` RPC handler. They appear in the
  gateway's device list but with `tunnels: []` and any tunnel-listing
  RPC must tolerate a device that exposes none.
- **Message plane is the entire surface.** They publish
  `hackline/<zid>/msg/event/...` and `.../log/...`, subscribe
  `.../cmd/...`, and may serve `.../api/...` queryables. Same
  keyexpr namespace as Linux devices — gateway code does not branch
  on device class.
- **Own ZID, own keypair, own ACL row** — no different from a Linux
  device's `hackline-client` session in terms of trust model (§3.5).
- **`hackline-client` SDK is Rust-first.** ESP32-S3 / -C3 / -C6 run
  Rust on `esp-hal` / `esp-idf-svc` and Zenoh has a C client
  (`zenoh-c`) plus an in-progress `zenoh-pico` for very constrained
  targets. v0.1 ships docs + examples for Rust-on-ESP32 against
  Zenoh-pico (or zenoh-c via `bindgen`); a hand-written
  micro-SDK that hardcodes our keyexpr conventions and our
  `hackline-proto` JSON envelopes is Phase 5 (see §14 Q6).

#### 3.7.1 Cert / credential issuance

These devices cannot do interactive claim flows, cannot run a CLI,
and often arrive on the network already provisioned at the factory.
**Hackline-gateway must be able to issue a Zenoh credential bundle
(ZID + per-device keypair + Zenoh ACL grant) for a constrained
device on operator request, and return that bundle in a form that
can be flashed or written to the device out-of-band.**

Concretely:

```text
# Operator pre-issues a credential bundle for a device that doesn't exist yet
POST /v1/devices/issue
  { class: "constrained", label: "sensor-rack-7", expires_in_days: 365 }
--> { zid, key_bundle: <PEM/DER>, zenoh_config_snippet, gateway_ca_pem }
```

The operator flashes that bundle into the ESP32's NVS / SPIFFS at
factory time (or pushes it via OTA from a pre-existing fleet tool).
First time the device boots and joins Zenoh, gateway sees its
liveliness token under its assigned ZID and the device row flips
from `issued` to `online`.

Differences from the Linux-device claim flow (§6):

| | Linux device (`hackline-agent`) | Constrained (ESP32) |
|---|---|---|
| Initial enrollment | Device-initiated `POST /v1/claim` from the device itself | Operator-initiated `POST /v1/devices/issue` from the CLI/UI |
| Identity material lives | `/etc/hackline/device.pem` written by the agent | NVS partition flashed before deployment |
| Rotation | `hackline rotate <zid>` triggers agent to re-claim | Re-issue + re-flash (or OTA) — devices this small don't re-claim themselves |
| Cert lifetime default | Long (multi-year) | Short-ish (1 year) — operator can shorten per-class |
| Revocation | Pull ACL entry, agent disconnects | Pull ACL entry, device disconnects on next reconnect attempt |

The `devices` table grows a `class` column (`linux | constrained`)
and the issue/claim paths populate it. The gateway uses `class` to
decide whether tunnel-plane RPCs are even valid for that device.

#### 3.7.2 Why this matters now (not later)

We make this a first-class concern in v0.1 rather than "we'll add
ESP32 support eventually" because the **wire surface and the
cert-issuance API are the things that lock in early**. If we ship
`POST /v1/claim` as the only path to a working device, every ESP32
that shows up later either gets a bolt-on second issuance API (with
its own auth model and audit trail) or an awkward "pretend it's a
Linux device" wrapper. Cheap to design in now, expensive to
retrofit. The actual SDK + flashing tooling can land later (§14 Q6),
but the gateway-side issuance endpoint and the `class` column ship
with Phase 1 so we don't paint ourselves into a corner.

---

## 4. Crate layout

A workspace with six crates plus three binaries.

```
hackline/
├── Cargo.toml
├── README.md
├── SCOPE.md                ← this file
├── DECISIONS.md            ← short ADR log
└── crates/
    ├── hackline-proto/     (1) wire types: ZID, Topic, Envelope,
    │                           query payloads (OpenRequest, OpenAck,
    │                           ByteFrame, CmdAck), SSE event schema,
    │                           REST DTOs, errors.
    │                           No tokio, no zenoh, no I/O.
    ├── hackline-core/      (2) bridging helpers (TCP↔Zenoh pub/sub),
    │                           keyexpr builders for both planes.
    │                           MAY depend on tokio + zenoh.
    ├── hackline-client/    (3) device-side SDK. Wraps zenoh::Session
    │                           with hackline conventions:
    │                           publish_event, serve_api, subscribe_cmd.
    │                           Linked into rubix-agent and any other
    │                           device app. crates.io published.
    ├── hackline-agent/     (4) device-side binary (`hackline-agent`)
    │                           — tunnel plane only.
    ├── hackline-gateway/   (5) cloud-side library + binary
    │                           (`hackline-gateway`)
    └── hackline-cli/       (6) `hackline` user-facing CLI
                                (REST/SSE client only)
```

Binaries shipped: **`hackline-agent`**, **`hackline-gateway`**,
**`hackline`**.

Hard rules:

- **R1.** `hackline-proto` is pure types. No `tokio`, no `zenoh`, no
  filesystem.
- **R2.** `hackline-agent`, `hackline-client`, and `hackline-gateway`
  do not depend on each other. Anything they share lives in
  `hackline-core` or `hackline-proto`.
- **R3.** Only `hackline-cli`, `hackline-agent`, and the gateway's
  `main.rs` may install a logging subscriber, parse argv, or call
  `std::process::exit`. Library code returns `Result<_,_>`.
- **R4.** SQLite lives **only** in `hackline-gateway`. Devices have
  no persistent state of their own beyond config files and
  Zenoh-managed credentials.
- **R5.** `hackline-client` does not depend on `hackline-agent`.
  Device apps work even if the agent is stopped.

---

## 5. Wire surface

### 5.1 Zenoh keyexprs

```
# Tunnel plane (bytes)
hackline/<zid>/info                                queryable, AgentInfo JSON
hackline/<zid>/tcp/<port>/open                     queryable, OpenRequest → OpenAck
hackline/<zid>/tcp/<port>/<request_id>/up          pub (agent) / sub (gateway)
hackline/<zid>/tcp/<port>/<request_id>/down        pub (gateway) / sub (agent)

# Message plane (typed envelopes)
hackline/<zid>/msg/event/<topic...>                pub (app)     / sub (gateway)
hackline/<zid>/msg/log/<topic...>                  pub (app)     / sub (gateway)
hackline/<zid>/msg/cmd/<topic...>                  pub (gateway) / sub (app)
hackline/<zid>/msg/cmd-ack                         pub (app)     / sub (gateway)
hackline/<zid>/msg/api/<topic...>                  queryable (app), get (gateway)

# Liveliness
@/liveliness/hackline/<zid>                        Zenoh liveliness, agent-owned
```

`<zid>` here means "the Zenoh ZID of the session that owns the
declaration" — for the tunnel plane that's the agent's ZID; for the
message plane that's the app's ZID. Apps and the agent share a
device cert chain but have distinct ZIDs.

`<topic...>` is dot-separated user-defined hierarchy escaped per
§5.5: `graph.slot.temp.changed`, `block.install`, `audit.entry`.
Renders to `/`-separated when a Zenoh keyexpr is built.

### 5.2 Wire types (in `hackline-proto`)

```rust
// Tunnel plane
struct OpenRequest {
    request_id: Uuid,
    peer:       Option<String>,
}
struct OpenAck {
    ok:      bool,
    message: Option<String>,
}
struct ByteFrame {
    seq:  u64,                    // monotonic per-direction
    data: bytes::Bytes,
}

// Message plane
struct Envelope {
    id:           Uuid,                    // per-message, generated by sender
    ts:           DateTime<Utc>,
    content_type: String,                  // default "application/json"; reserved for future bincode
    headers:      BTreeMap<String, String>, // small, e.g. trace_id, source app
    payload:      bytes::Bytes,            // opaque to gateway
}

struct CmdEnvelope {
    cmd_id:    Uuid,                       // gateway-assigned, idempotency key
    topic:     String,
    enqueued_at: DateTime<Utc>,
    expires_at:  DateTime<Utc>,
    envelope:  Envelope,
}

struct CmdAck {
    cmd_id: Uuid,
    result: CmdResult,
    detail: Option<String>,
}

enum CmdResult { Accepted, Rejected, Failed, Done }
```

JSON encoding for v0.1 across both planes (debuggability wins; payloads
are small control frames). `content_type` on the envelope is reserved
so we can add bincode later without re-versioning the namespace.

### 5.3 REST (gateway)

JSON in / JSON out. `Authorization: Bearer <token>` on everything
except `GET /v1/health` and the claim endpoints.

```
# Health & claim
GET    /v1/health
GET    /v1/claim/status                          { claimed, can_claim }
POST   /v1/claim       { token, owner }          { token, owner }

# Devices
GET    /v1/devices                               [Device]
POST   /v1/devices     { zid, label }            Device
GET    /v1/devices/:id
PATCH  /v1/devices/:id { label?, customer_id? }  Device
DELETE /v1/devices/:id

GET    /v1/devices/:id/info                      live AgentInfo via Zenoh query
GET    /v1/devices/:id/health                    { last_seen_ts, latency_ms_p50, online }

# Tunnels (byte plane)
GET    /v1/tunnels                               [Tunnel]
POST   /v1/tunnels    { device_id, kind, local_port,
                         public_hostname?, public_port? } Tunnel
DELETE /v1/tunnels/:id

# Message plane — commands (durable, fire-and-forget with ack)
POST   /v1/devices/:id/cmd/:topic
       { payload, expires_in?, content_type? }   { cmd_id }
GET    /v1/devices/:id/cmd?status=pending|delivered|acked
                                                 [CmdOutboxRow]
DELETE /v1/cmd/:cmd_id                                       # cancel queued

# Message plane — RPC (synchronous, fails if device offline)
POST   /v1/devices/:id/api/:topic
       { payload, timeout_ms, content_type? }    { reply, content_type }

# Message plane — events & logs (read-only fan-out from device→cloud)
GET    /v1/events?device=ID&topic=T&since=…&cursor=…&limit=…
                                                 { entries, next_cursor }
GET    /v1/log?device=ID&topic=T&since=…&cursor=…&limit=…
                                                 { entries, next_cursor }

# Users / tokens
GET    /v1/users                                 [User]   (admin only)
POST   /v1/users      { name, role, device_scope?,
                        tunnel_scope?, expires_in? }     User
DELETE /v1/users/:id
POST   /v1/users/:id/tokens                      { token, expires_at }

# Audit
GET    /v1/audit?cursor=…&limit=…                { entries, next_cursor }
```

Conventions:

- All token-returning endpoints use the field name **`token`**. The
  string is the raw bearer; shown **once**, never returned again.
- `PATCH /v1/devices/:id` mutable fields are exactly `label` and
  `customer_id`. Adding mutable fields requires updating this list.
- `GET /v1/audit`, `/v1/events`, `/v1/log`, `/v1/devices/:id/cmd` are
  **cursor-based** (opaque cursor, server returns `next_cursor` until
  exhausted). Offset pagination is forbidden — these tables are
  append-only and grow.
- Token lookup uses indexed equality on `sha256(token)`.
  Constant-time comparison is on the **value comparison** path.
- `/v1/devices/:id/api/:topic` is synchronous — cloud-app sends, gateway
  issues a Zenoh `get`, device app's `serve_api` handler replies,
  gateway returns the reply. Fails with `503 device_unreachable` if
  the device's Zenoh liveliness token is gone.
- `/v1/devices/:id/cmd/:topic` always succeeds (writes to the outbox
  table). Delivery happens asynchronously when the device is reachable.

### 5.4 SSE (gateway)

Live updates without inventing a websocket protocol.

```
GET /v1/events/stream                                       admin-scoped, all events
GET /v1/devices/:id/events/stream                           one device, control-plane events
GET /v1/devices/:id/msg/events/stream?topic=…               live device→cloud message-plane events
GET /v1/devices/:id/msg/log/stream?topic=…                  live device→cloud logs
```

The control-plane `events/stream` carries the event types below. The
two `/msg/...` streams carry message-plane envelopes from the device,
filtered by topic-keyexpr (Zenoh wildcards apply: `temp.*`,
`graph.**`).

| `kind` | Required `data` keys |
|---|---|
| `device.online` | `device_id`, `zid`, `at` |
| `device.offline` | `device_id`, `zid`, `at`, `reason` |
| `tunnel.opened` | `tunnel_id`, `device_id`, `request_id`, `peer` |
| `tunnel.closed` | `tunnel_id`, `request_id`, `bytes_up`, `bytes_down`, `duration_ms` |
| `cmd.queued` | `cmd_id`, `device_id`, `topic` |
| `cmd.delivered` | `cmd_id`, `device_id`, `at` |
| `cmd.acked` | `cmd_id`, `device_id`, `result`, `at` |
| `cmd.expired` | `cmd_id`, `device_id` |
| `audit.entry` | full audit row, see §7.2 |

**Caddy / reverse-proxy requirement:** SSE through any HTTP proxy
requires response buffering disabled. The operator's Caddyfile must
include `flush_interval -1` for `/v1/events/stream`,
`/v1/devices/*/events/stream`, `/v1/devices/*/msg/*/stream`. nginx:
`proxy_buffering off`. Documented in §9.8 too.

### 5.5 Topic encoding

Topics are dotted (`graph.slot.temp.changed`). When rendered to a
Zenoh keyexpr, dots become `/`. Tokens containing `.` are not
permitted (validated on send). `*` and `**` in topic strings on the
**subscribe** side are passed through to Zenoh as wildcards;
**publish** topics must be fully qualified (no wildcards).

### 5.6 Why SSE, not gRPC

- gRPC needs `tonic` + `protoc` + per-language codegen. The only
  thing we'd use bidirectional streaming for is the event feed,
  which is one-directional.
- SSE is one-way (server→client) which exactly matches the event
  feed. Browser `EventSource` is built in. CLI uses `eventsource-stream`.
- All commands are RPC-style — REST handles them naturally.

If we ever need bidirectional streaming (live remote shell?) we can
add a single WebSocket endpoint without disturbing REST or SSE.

---

## 6. Auth — claim + owner token + scoped users

Lifted from
[`token-service`](../../rubix-workspace/token-service).

### 6.1 First-boot claim (gateway)

1. `hackline-gateway` starts with empty DB. Migrations run, then a
   single `INSERT INTO claim_pending` runs **iff** `users` is empty
   and `claim_pending` is empty (one transaction). The pending token
   value is generated by `token-crypto::generate()`; only its hash
   is stored. Raw token printed once on startup:
   ```
   hackline: first boot — claim with:
     hackline login --server https://hackline.example.com --token <claim>
   ```
2. `POST /v1/claim { token, owner }` runs as **one SQL transaction**:
   ```sql
   BEGIN IMMEDIATE;
   -- compare hash(token) vs claim_pending.token_hash, constant-time in app code
   DELETE FROM claim_pending WHERE id = 1;
   INSERT INTO users (name, role, ...) VALUES (?, 'owner', ...);
   INSERT INTO tokens (user_id, token_hash, ...) VALUES (?, ?, ...);
   COMMIT;
   ```
   Concurrent claim requests: SQLite's `BEGIN IMMEDIATE` ensures
   exactly one transaction wins; the other returns `409 already_claimed`.

3. Recovery if the operator missed the printed token:
   ```
   hackline-gateway print-claim
   ```
   Read-only; exits non-zero if no pending claim. Does not regenerate.
   To regenerate: `hackline-gateway reset-claim` (destructive).

### 6.2 Steady state

- Owner token client-side: `$XDG_CONFIG_HOME/hackline/credentials.json`
  (mode `0600`).
- Server stores only `sha256(token)` in `tokens.token_hash`.
- Constant-time compare (`subtle::ConstantTimeEq`) wraps the value
  comparison itself, not the row lookup.
- Owner can mint additional **scoped tokens**:
  - `role`: `owner` | `admin` | `support` | `viewer` | `customer`
    *(`customer` reserved; enforcement in Phase 2 — present in v0.1
    only so the schema doesn't migrate)*.
  - `device_scope`: `*` | JSON array of device ids
  - `tunnel_scope`: `*` | JSON array of tunnel ids
  - `expires_at`: optional
- All bearer tokens authenticate REST, SSE, and CLI uniformly.

### 6.3 Token storage shape

Tokens live in their own `tokens` table — one user, many tokens,
independent expiry & rotation. See §7.2.

### 6.4 External IdP integration (optional)

Hackline-gateway natively supports its own bearer-token auth (above)
for operators, support, CLI, and direct API consumers. For
**customer-facing flows where the consumer is also a user of the
service running on the device** (e.g. rubix Studio), the gateway
optionally runs as an OIDC client of an upstream IdP (Rauthy, Auth0,
Keycloak, anything OIDC-compliant). In that mode:

- Customer logs in via the IdP, gateway exchanges code for a hackline
  `ScopedToken` (L1+L2 enforced as normal).
- Gateway optionally injects a signed identity header into proxied
  tunnel requests so the device-side application can run its own
  authorization layer (L3) without itself depending on the IdP.

This is a hackline-side option — the device-side identity-header
contract lives with the consuming application. The first consumer
(rubix) documents its specific shape in
[`INTEGRATION-RUBIX.md` §9](./INTEGRATION-RUBIX.md#9-auth-seam--device-access-tunnel-access-in-device-authz).
The general pattern — "gateway terminates IdP auth, proxies with a
signed `X-<consumer>-User` header" — is generic; nothing in
hackline-gateway hardcodes rubix.

---

## 7. Persistence — SQLite, embedded

### 7.1 Why SQLite

- Single-binary deploy, zero ops.
- A few thousand devices fits comfortably.
- WAL mode gives concurrent reads with a single writer.
- Trivial backup (`sqlite3 gateway.db ".backup …"`).

If we outgrow SQLite (>~50k devices, multi-region active-active) we
migrate to Postgres. Data shape doesn't change. See `DECISIONS.md`
`persistence — 2026-05`.

### 7.2 Tables (v0.1)

```sql
CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE claim_pending (
  id          INTEGER PRIMARY KEY CHECK (id = 1),
  token_hash  TEXT    NOT NULL,
  created_at  INTEGER NOT NULL
);

CREATE TABLE users (
  id           INTEGER PRIMARY KEY,
  name         TEXT    NOT NULL UNIQUE,
  role         TEXT    NOT NULL CHECK (
                 role IN ('owner','admin','support','viewer','customer')
               ),
  device_scope TEXT    NOT NULL DEFAULT '*',
  tunnel_scope TEXT    NOT NULL DEFAULT '*',
  created_at   INTEGER NOT NULL,
  last_used_at INTEGER
);

CREATE TABLE tokens (
  id           INTEGER PRIMARY KEY,
  user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  token_hash   TEXT    NOT NULL UNIQUE,
  expires_at   INTEGER,
  created_at   INTEGER NOT NULL,
  last_used_at INTEGER
);
CREATE INDEX tokens_user ON tokens(user_id);

CREATE TABLE devices (
  id           INTEGER PRIMARY KEY,
  zid          TEXT    NOT NULL UNIQUE
                 CHECK (length(zid) BETWEEN 2 AND 32
                        AND zid GLOB '[0-9a-f]*'),
  label        TEXT    NOT NULL,
  customer_id  INTEGER,
  created_at   INTEGER NOT NULL,
  last_seen_at INTEGER
);

CREATE TABLE tunnels (
  id              INTEGER PRIMARY KEY,
  device_id       INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
  kind            TEXT    NOT NULL CHECK (kind IN ('tcp','http')),
  local_port      INTEGER NOT NULL,
  public_hostname TEXT,
  public_port     INTEGER,
  enabled         INTEGER NOT NULL DEFAULT 1,
  created_at      INTEGER NOT NULL,
  UNIQUE (public_hostname),
  UNIQUE (public_port),
  CHECK (
    (kind = 'http' AND public_hostname IS NOT NULL AND public_port IS NULL) OR
    (kind = 'tcp'  AND public_port     IS NOT NULL AND public_hostname IS NULL)
  )
);

-- Per-tunnel-session, NOT per-connection-event. ts_close NULL while
-- in flight. Per-event logging would be hundreds of millions of
-- rows/year at fleet scale.
CREATE TABLE audit (
  id          INTEGER PRIMARY KEY,
  ts_open     INTEGER NOT NULL,
  ts_close    INTEGER,
  user_id     INTEGER REFERENCES users(id),
  device_id   INTEGER REFERENCES devices(id),
  tunnel_id   INTEGER REFERENCES tunnels(id),
  request_id  TEXT    NOT NULL,
  action      TEXT    NOT NULL,
  peer        TEXT,
  bytes_up    INTEGER,
  bytes_down  INTEGER,
  detail      TEXT                              -- JSON
);
CREATE INDEX audit_open    ON audit(ts_open);
CREATE INDEX audit_device  ON audit(device_id);
CREATE INDEX audit_request ON audit(request_id);

-- Message-plane: durable command outbox. Cloud writes, gateway
-- delivers, device acks. Bounded by per-device row cap (default
-- 1000) and TTL (default 7d). Anything exceeding either is rejected
-- at POST time — we are not a general broker.
CREATE TABLE cmd_outbox (
  id           INTEGER PRIMARY KEY,
  cmd_id       TEXT    NOT NULL UNIQUE,        -- UUID, idempotency key
  device_id    INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
  topic        TEXT    NOT NULL,
  content_type TEXT    NOT NULL DEFAULT 'application/json',
  payload      BLOB    NOT NULL,
  enqueued_at  INTEGER NOT NULL,
  expires_at   INTEGER NOT NULL,
  delivered_at INTEGER,
  ack_at       INTEGER,
  ack_result   TEXT    CHECK (ack_result IN ('accepted','rejected','failed','done')),
  ack_detail   TEXT,
  attempts     INTEGER NOT NULL DEFAULT 0,
  last_error   TEXT,
  CHECK (length(payload) <= 65536)             -- 64 KiB hard cap; URL pattern for bigger
);
CREATE INDEX cmd_outbox_pending
  ON cmd_outbox(device_id, enqueued_at)
  WHERE delivered_at IS NULL;

-- Message-plane: bounded event ring per device. "Last N events from
-- edge-42." Default cap 10000 rows per device, oldest pruned. Not a
-- TSDB. Not for long-term analytics.
CREATE TABLE events (
  id           INTEGER PRIMARY KEY,
  device_id    INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
  topic        TEXT    NOT NULL,
  ts           INTEGER NOT NULL,
  content_type TEXT    NOT NULL DEFAULT 'application/json',
  payload      BLOB    NOT NULL,
  CHECK (length(payload) <= 65536)
);
CREATE INDEX events_device_ts ON events(device_id, ts);

-- Same shape as `events`, separate table so retention/cap can differ.
-- Default cap 10000 rows per device.
CREATE TABLE logs (
  id           INTEGER PRIMARY KEY,
  device_id    INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
  topic        TEXT    NOT NULL,
  ts           INTEGER NOT NULL,
  level        TEXT    NOT NULL CHECK (level IN ('trace','debug','info','warn','error')),
  payload      BLOB    NOT NULL,
  CHECK (length(payload) <= 65536)
);
CREATE INDEX logs_device_ts ON logs(device_id, ts);
```

#### Well-known `audit.action` values

| `action` | When | Required `detail` keys |
|---|---|---|
| `tunnel.session` | One bridged TCP connection (open + close share a row) | none |
| `auth.login` | Bearer token first used in a session | `user_agent`, `ip` |
| `auth.token.mint` | New token created | `user_id_target`, `expires_at` |
| `auth.token.revoke` | Token deleted | `user_id_target` |
| `device.create` | `POST /v1/devices` | `zid`, `label` |
| `device.delete` | `DELETE /v1/devices/:id` | `zid` |
| `tunnel.create` | `POST /v1/tunnels` | `kind`, `local_port`, `public_*` |
| `tunnel.delete` | `DELETE /v1/tunnels/:id` | none |
| `cmd.send` | `POST /v1/devices/:id/cmd/:topic` | `cmd_id`, `topic` |
| `cmd.cancel` | `DELETE /v1/cmd/:cmd_id` | `cmd_id` |
| `api.call` | `POST /v1/devices/:id/api/:topic` | `topic`, `outcome` |
| `claim.success` | `POST /v1/claim` | `owner` |

Adding an `action` requires updating this table.

### 7.3 Retention & caps

| Table | Default cap | Default retention | Configurable as |
|---|---|---|---|
| `audit` | unbounded (control-plane is cheap) | `tunnel.session` rows: 180 days; others: indefinite | `audit.retention_days` |
| `cmd_outbox` | 1000 rows per device | TTL on insert (default 7d), delivered+acked rows pruned at 30d | `cmd.max_per_device`, `cmd.default_ttl`, `cmd.history_days` |
| `events` | 10000 rows per device (oldest pruned) | n/a (ring) | `events.max_per_device` |
| `logs` | 10000 rows per device (oldest pruned) | n/a (ring) | `logs.max_per_device` |

Pruning is a background task: per-table, batched (`DELETE … LIMIT N`),
runs on a 60s timer. **Caps are enforced at write time** for
`events`/`logs` (oldest row deleted in the same transaction); TTL
expiry on `cmd_outbox` is enforced both at write time (refuse if
already at cap and oldest pending row hasn't expired) and by the
background prune.

### 7.4 Migrations

**`refinery`** (embed `.sql` in the binary, run on startup). One
binary brings its own schema; no runtime CLI needed.

### 7.5 Pool sizing

`r2d2` is sync; the gateway calls SQLite from async via
`tokio::task::spawn_blocking`. Pool size **must be ≤ tokio
blocking-thread budget** (default 512). Defaults: 16 read connections
+ 1 writer. Raising pool size requires raising blocking-thread budget
in the same change.

---

## 8. Device-side SDK — `hackline-client`

The crate device apps (rubix-agent, BACnet driver, anything) link in.
Thin wrapper around `zenoh::Session` that enforces hackline
conventions.

```rust
let cfg = hackline_client::Config::from_file("/etc/hackline/client.toml")?;
let session = hackline_client::Session::open(cfg).await?;

// Publish telemetry — fire-and-forget; reliable transport but no
// durability across device restart or device offline.
session.publish_event("graph.slot.temp.changed", &json!({ "v": 21.4 })).await?;

// Publish structured log
session.publish_log(LogLevel::Warn, "audit.entry", &json!({ ... })).await?;

// Serve a typed RPC (replaces fleet::mount). Handler is sync from
// the SDK's POV; the user can spawn inside.
session.serve_api("nodes.list", |req: ApiRequest| async move {
    let payload = list_nodes(&req.payload).await?;
    Ok(ApiReply::json(payload))
}).await?;

// Subscribe to commands — durable from gateway side, at-least-once.
let mut cmds = session.subscribe_cmd("block.install").await?;
while let Some(cmd) = cmds.next().await {
    if seen_before(cmd.cmd_id) { cmd.ack(CmdResult::Done).await?; continue; }
    match install_block(&cmd.payload).await {
        Ok(()) => cmd.ack(CmdResult::Done).await?,
        Err(e) => cmd.ack_with(CmdResult::Failed, e.to_string()).await?,
    }
}
```

### 8.1 Delivery semantics

| Method | Semantics | Durability |
|---|---|---|
| `publish_event` | Best-effort, reliable Zenoh transport | None — drops while offline |
| `publish_log` | Same as event | Same as event |
| `serve_api` | Synchronous req/reply; gateway sees `503` if device offline | None |
| `subscribe_cmd` | **At-least-once.** `cmd_id` is the idempotency key; app must dedupe. Ack removes from outbox. | Durable across device offline windows |

At-least-once was chosen over at-most-once because every realistic
device command (install, reboot, reload-config) is either idempotent
or trivially made idempotent with a server-assigned id (`cmd_id`).
At-most-once would silently drop commands across device restarts in
the gap between delivery and ack — strictly worse.

### 8.2 What the SDK does NOT do

- No second auth layer. Auth is Zenoh ACL on the session.
- No connection management beyond `zenoh::Session`'s own.
- No local IPC server. Apps consume the SDK as a library.
- No retry of `serve_api` handlers. The gateway sees the failure and
  surfaces it.

### 8.3 What the SDK adds beyond raw Zenoh

- `Topic` / `Envelope` types from `hackline-proto`.
- Keyexpr builders (no string concatenation in app code).
- Cmd ack convenience (publishes the ack envelope correctly).
- Sensible transport defaults (reliable for cmd/api, best-effort
  permitted for events at the caller's option).
- Startup ACL self-check (fail-closed if session has more than
  serve+publish on `hackline/<own-zid>/**`).

---

## 9. Dependencies — chosen, with reasons

Locked in for v0.1. Adding anything not listed needs a SCOPE update.

### 9.1 Internal (path)

- `hackline-proto`, `hackline-core`, `hackline-client`,
  `hackline-gateway`, `hackline-agent`.
- `token-crypto`, `domain-token`, `service-token` — pulled by **path**
  from `../../../rubix-workspace/token-service`.

### 9.2 Async runtime + I/O

| `tokio` `1` (`features = ["full"]`); `futures` `0.3`; `bytes` `1`. |

### 9.3 Zenoh

| `zenoh` latest 1.x stable; `zenoh-ext` matches. Feature flags pinned
per-crate so the agent build stays minimal (no router, no plugin
loader). |

### 9.4 Wire format

| `serde` `1`; `serde_json` `1`; `uuid` `1` (`v4`, `serde`);
`chrono` `0.4` (`serde`) for `Envelope.ts`. |

JSON wins for v0.1 even on the message plane: payloads are tiny;
debuggability wins. `Envelope.content_type` is reserved for a future
bincode swap without re-versioning the namespace.

### 9.5 HTTP server (gateway only)

| `axum` `0.8` (fall back to `0.7.x` if needed; record in DECISIONS);
`tower-http` matches `axum`; `tokio-stream` `0.1`; `hyper` `1`. |

### 9.6 HTTP client (CLI only)

| `reqwest` `0.12` (`json`, `rustls-tls`, `stream`); `eventsource-stream`. |

Gateway does **not** depend on `reqwest`.

### 9.7 SQLite

| `rusqlite` `0.32` (`bundled`); `r2d2` + `r2d2_sqlite`; `refinery`
`0.8` (`rusqlite`). See §7.5 for pool sizing. |

### 9.8 TLS / certs

Gateway runs **behind Caddy** for v0.1. Listens plain HTTP on
`127.0.0.1:<port>`, trusts `X-Forwarded-*` from Caddy.

```caddy
*.cloud.example.com, cloud.example.com {
  tls { on_demand }
  reverse_proxy 127.0.0.1:8080 {
    flush_interval -1
  }
}
```

`flush_interval -1` is non-optional for any SSE endpoint.

### 9.9 Auth crypto (transitive via token-service)

| `rand`, `sha2`, `subtle`, `base64`, `hex` — per token-service. |

### 9.10 Ergonomics

| `anyhow` `1` (binaries); `thiserror` `1` (libraries); `tracing`
`0.1`; `tracing-subscriber` `0.3` (`env-filter`, `json`); `clap` `4`
(`derive`, `env`); `directories` `5`; `humantime` `2`; `figment`
`0.10` (`toml`, `env`). |

### 9.11 Explicit non-dependencies

A future agent will reach for these — and shouldn't, without a SCOPE
update.

- **No `bore` / `wstunnel` / `frp` / `rathole` / `OpenZiti`.** Zenoh
  is the byte transport.
- **No `nats` / `kafka` / `rabbitmq`.** Zenoh is the message broker.
  Cmd outbox is SQLite + Zenoh pub/sub on top, not a competing broker.
- **No `tonic` / gRPC.** §5.6.
- **No `sqlx` / `diesel`.** §7.
- **No second tunnel mux library.** Each TCP connection is one
  Open-RPC + one pub/sub pair.
- **No `openssl` / `native-tls`.** rustls only when TLS lands.
- **No global Caddy management from our binary.** Operators run Caddy.

`hackline-core` is allowed to depend on `tokio` + `zenoh`; same for
`hackline-client`. The agent already pulls them, and a hypothetical
no-tokio consumer would depend on `hackline-proto` instead.

---

## 10. Observability

### 10.1 Logs

`tracing` everywhere. Each bridged TCP connection logs `tunnel_id`,
`device_id`, `request_id`, `peer` on open; bytes + duration on close.
Each cmd: `cmd_id`, `device_id`, `topic`, lifecycle transitions
(`queued`, `delivered`, `acked`, `expired`).

### 10.2 Metrics

`GET /metrics` (Prometheus text format, admin-token gated in v0.1):

- `hackline_devices_online{}` (gauge)
- `hackline_tunnel_sessions_total{kind,outcome}` (counter)
- `hackline_tunnel_active{kind}` (gauge)
- `hackline_tunnel_bytes_total{direction}` (counter)
- `hackline_cmd_outbox_depth{device}` (gauge)
- `hackline_cmd_total{outcome}` (counter; `accepted|rejected|failed|done|expired|cancelled`)
- `hackline_events_received_total{topic}` (counter; cardinality
  controlled — high-cardinality topics aggregated to a `_other` bucket
  by a configurable allowlist)
- `hackline_logs_received_total{level}` (counter)
- `hackline_api_calls_total{topic,outcome}` (counter)
- `hackline_audit_rows{}` (gauge)

### 10.3 Health & "last seen"

- `GET /v1/health`: process is up, DB reachable, Zenoh session
  established.
- `GET /v1/devices/:id/health`: device is **online** iff its Zenoh
  liveliness token (`@/liveliness/hackline/<zid>`) is currently
  visible. `last_seen_ts` is updated on **either** a successful
  query reply or a liveliness-token transition; the two sources are
  reconciled by taking the most recent timestamp.
- `latency_ms_p50` over the last N=20 successful `…/info` queries.

---

## 11. Configuration files

Both gateway and agent take TOML config via `--config FILE`, with
env-var overrides via `figment`'s `Env::prefixed`.

### 11.1 Gateway config

Search order: `--config` → `$HACKLINE_CONFIG` →
`/etc/hackline/gateway.toml` →
`$XDG_CONFIG_HOME/hackline/gateway.toml` → `./gateway.toml`.
Env prefix: `HACKLINE_GATEWAY_`.

```toml
[server]
listen           = "127.0.0.1:8080"
public_base_url  = "https://cloud.example.com"

[db]
path             = "/var/lib/hackline/gateway.db"

[zenoh]
config_file      = "/etc/hackline/zenoh-gateway.json5"
mode             = "client"
connect          = ["tls/router.example.com:7447"]

[audit]
retention_days   = 180

[tunnels.tcp]
listen_host      = "0.0.0.0"
port_range       = [10000, 19999]

[cmd]
max_per_device   = 1000
default_ttl      = "7d"
history_days     = 30

[events]
max_per_device   = 10000

[logs]
max_per_device   = 10000
```

### 11.2 Agent config (tunnel plane)

Search order: `--config` → `$HACKLINE_CONFIG` →
`/etc/hackline/agent.toml` → `$XDG_CONFIG_HOME/hackline/agent.toml`.
Env prefix: `HACKLINE_AGENT_`.

```toml
[zenoh]
config_file      = "/etc/hackline/zenoh-agent.json5"

[expose]
# Whitelist of local ports the agent will bridge. Anything not listed
# is rejected at the agent regardless of what the gateway requests.
ports            = [22, 8080]
```

`expose.ports` is **defence in depth** — gateway also enforces via
tunnel rows. The agent failing closed prevents a compromised gateway
from asking for `:5432` of the device's Postgres.

### 11.3 Client config (`hackline-client` SDK)

Apps using the SDK ship their own `client.toml` (or build `Config`
in code). Search order when using `Config::from_file(default_path)`:
`/etc/hackline/client.toml` →
`$XDG_CONFIG_HOME/hackline/client.toml`.

```toml
[zenoh]
config_file      = "/etc/hackline/zenoh-app.json5"
# distinct ZID per app; same cert chain as the device's other sessions

[app]
name             = "rubix-agent"            # appears in headers / audit
```

---

## 12. Graceful shutdown

### 12.1 Gateway

On `SIGTERM`:

1. Stop accepting new public TCP connections.
2. Stop accepting new HTTP requests (axum `with_graceful_shutdown`).
3. Wait up to **30 s** (configurable) for in-flight bridged
   connections to drain.
4. Drop pubs/subs on tunnel side channels.
5. Stop the cmd-delivery and prune background tasks; drain in-flight
   cmd writes; flush WAL; close DB.

### 12.2 Agent

On `SIGTERM`:

1. Undeclare all tunnel queryables.
2. Wait up to 10 s for in-flight bridges to drain.
3. Drop publishers/subscribers; close TCP streams to local services.
4. Drop liveliness token last.

### 12.3 Client SDK

`Session::close()` (or `Drop`):

1. Undeclare all queryables (`api/*`).
2. Stop subscribers; let any in-flight cmd handlers finish naturally
   (caller's responsibility — SDK does not abort futures).
3. Drop publishers.
4. Drop the underlying `zenoh::Session`.

In-flight bridged TCP connections and in-flight cmd handlers do not
survive restart. Acceptable because:

- HTTP clients retry naturally.
- SSH users notice and reconnect.
- Cmd outbox is durable — anything not yet acked is redelivered when
  the device reconnects (this is precisely why we picked
  at-least-once).

---

## 13. Phasing

Each phase ends with a working, demoable binary. No phase ships a
half-implemented surface from a later phase.

### Phase 0 — Zenoh spike (one day, before Phase 1)

- Single Rust binary spawning two `Session`s in one process,
  `mode=peer`, fixed `connect`/`listen` endpoints.
- Validates: `Open`+pub/sub side channels (tunnel plane), pub/sub
  envelope round-trip (event), queryable round-trip (api), liveliness
  observation (online/offline).
- Outcome recorded in `DECISIONS.md`.

### Phase 1 — Tunnel plane happy path *(v0.1 minimum demo)*

- All six crates scaffolded, `cargo check` clean.
- `hackline-agent` serves bridges for whitelisted local ports.
- `hackline-gateway` reads tunnel rows from SQLite, opens TCP
  listeners, brokers bridges.
- `hackline login`, `hackline tunnel add --tcp 22 --public-port 2222`
  work end-to-end.
- Demo: `ssh -p 2222 user@cloud.example.com` lands on the device's
  sshd.
- Tests: bridge byte-roundtrip (tokio, no Zenoh); full path with two
  in-process Zenoh peers (Phase 0 promoted).

### Phase 1.5 — Message plane: events + logs

- `hackline-client` SDK ships with `publish_event` / `publish_log`.
- Gateway runs a fan-in subscriber on `hackline/*/msg/event/**` and
  `hackline/*/msg/log/**`, writes to `events` and `logs` tables with
  ring-buffer pruning.
- `GET /v1/events`, `GET /v1/log`, and the live SSE streams work.
- `hackline events tail --device ID --topic …` works.
- Demo: rubix-agent (or a stub) publishes a slot-changed event;
  Studio sees it via SSE; cursor query returns history.

### Phase 2 — Message plane: commands + api + HTTP host-routing

- `cmd_outbox` table; gateway delivery loop; SDK `subscribe_cmd` with
  ack semantics; `POST /v1/devices/:id/cmd/:topic`.
- SDK `serve_api`; gateway `POST /v1/devices/:id/api/:topic`
  synchronous round-trip.
- Gateway HTTP front-end (axum, behind Caddy) routes by `Host:` to
  the right tunnel — `device-1234.cloud.com`.
- WebSocket upgrades pass through unchanged.
- `customer` role + per-device scope enforced at the HTTP edge.

### Phase 3 — Audit completeness + admin UI

- `tunnel.session` rows on every bridged connection with byte counts.
- `cmd.send`, `cmd.cancel`, `api.call` audit entries.
- Static React admin bundle served from gateway, talks to REST + SSE.
- "Live tunnels", "cmd outbox", "live events" views.
- `/metrics` unauthenticated within the operator's private network.

### Phase 4 — Multi-tenant orgs

- `orgs` table (id, slug, name, created_at), seeded with a single
  `default` org by `V005__orgs.sql`. `org_id` is a non-null foreign
  key on `users` and `devices` and is backfilled to the `default`
  org for every row that pre-dated the migration.
- `AuthedUser` carries `org_id`. Every REST handler that scopes by
  device or user filters rows by the caller's `org_id` first, then
  applies the per-device customer-scope check from Phase 2. A row
  belonging to a different org returns `404 not_found` (the same
  shape as "row does not exist") — leaking the minimum, per the
  long-standing rule that 403 reveals the row exists at all.
- Keyexpr prefix becomes `hackline/<org_slug>/<zid>/...`. Tunnel
  plane (`bridge`, `bridge-ack`) and message plane (`evt`, `log`,
  `cmd`, `cmd-ack`, `api`) all live under this prefix; the
  parser in `hackline-proto::keyexpr` is org-aware.
  `hackline-agent` reads `org_slug` from its config; the gateway
  composes the prefix from the device's row at bridge / publish
  time. Zenoh ACL grants per org (one entry per slug) prevent a
  device in org A from reaching org B's subscribers / queryables
  even if it tries.
- REST surface: `POST /v1/orgs` (owner-only, creates an org),
  `GET /v1/orgs` (owner-only, lists every org on the gateway),
  `GET /v1/orgs/me` (any authenticated caller, returns the
  caller's own org). Cross-org user provisioning is a follow-on:
  the owner creates an org row, mints a user pinned to that org
  via `POST /v1/users` once that handler grows an `org_id`
  parameter; for v0.1 the owner can still operate cross-org via
  per-org owner bearer tokens.
- Claim flow: `POST /v1/claim` accepts an optional `org` slug.
  If absent or equal to `default`, the owner is stamped into the
  seeded org. If a fresh slug is supplied, a new org row is
  inserted in the same transaction and the owner is stamped
  into it. The response echoes the slug so the CLI can cache
  it for display (`hackline whoami`, `hackline org inspect`);
  the server still enforces isolation off the bearer token, not
  off the cached slug.
- CLI: `hackline login --org <slug>` carries the org into the
  claim request. `hackline org create | list | inspect` covers
  the new REST surface. Existing subcommands need no change —
  the bearer token already names the org server-side.

Out of scope here:

- Per-org wildcard certs / DNS plumbing for branded
  `device-N.<org>.cloud.example.com`. The keyexpr prefix and
  the DB row land in Phase 4; the ACME and wildcard-cert work is
  Phase 5 (§14 Q5).
- ESP32 / constrained-device support beyond what the existing
  `devices.class` column records.

### Phase 5 — Deployment polish

- ACME inside the gateway (optional, one-binary deploys).
- Postgres backend behind the same SQL repository trait if scale forces it.
- Rust→TS codegen for `hackline-proto`; `@hackline/client` npm package
  built on Zenoh-WS.

---

## 14. Open questions

1. **Zenoh-ext liveliness latency.** §10.3 assumes
   `@/liveliness/hackline/<zid>` is observable from the gateway with
   reasonable latency. Validate during Phase 0.
2. **Cmd-delivery loop shape.** Push (gateway publishes
   `hackline/<zid>/msg/cmd/...` whenever queue non-empty + device
   online) vs pull (device subscribes once and gateway just publishes
   on enqueue). Push-on-online is the strawman; revisit after Phase 0
   shows what reconnect-redelivery looks like in Zenoh.
3. **Per-tunnel rate limit / connection cap.** Out of v0.1; recorded
   so we don't forget.
4. **`zenoh-ext::AdvancedPublisher` for replay-on-reconnect** of
   events. Could cut the event-loss-while-offline window. Not v0.1;
   confirm API stability before we'd need it.
5. **Customer-facing branding** of `device-N.cloud.com` — wildcard
   cert per operator-org or per-customer subdomain? Phase 4.
6. **`hackline-client` for non-Rust apps and constrained targets**
   (see §3.7). Phase 5+. WS-bridged Zenoh in browser is the obvious
   path for the JS SDK. For ESP32-class devices the strawman is
   Rust-on-`esp-hal` against `zenoh-pico` with our keyexpr
   conventions and `hackline-proto` JSON envelopes documented;
   embedded C apps either get a thin C wrapper or use `zenoh-c`
   directly with the same documented conventions. Decide in Phase 5
   based on which constrained targets we actually have customers for.
7. **`devices.class` enum extensibility** (see §3.7.1). v0.1 ships
   `linux | constrained`. If/when we add browser-resident or
   gateway-resident pseudo-devices the enum grows; keep the gateway
   code that branches on `class` in one module so the surface is
   small.

---

## 15. Pointers

- Zenoh: <https://zenoh.io/>
- token-service (sibling repo): `../../../rubix-workspace/token-service`
- Workspace rules: [`../CLAUDE.md`](../CLAUDE.md)
- Rejected alternatives & rationale: [`DECISIONS.md`](./DECISIONS.md)
- **First-consumer integration plan (rubix):** [`INTEGRATION-RUBIX.md`](./INTEGRATION-RUBIX.md) — load-bearing rules for how rubix consumes hackline. Read before adding any consumer-facing API.
