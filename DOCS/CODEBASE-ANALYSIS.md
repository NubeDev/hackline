# Hackline Rust Workspace — Complete Codebase Analysis

## Root Configuration

**File:** `/home/user/code/rust/codeless-workspace/hackline/Cargo.toml`

Workspace configuration with 5 crates as members:
- `crates/hackline-proto`
- `crates/hackline-core`
- `crates/hackline-agent`
- `crates/hackline-gateway`
- `crates/hackline-cli`

**Workspace-level settings:**
- Resolver: `2`
- Edition: `2021`
- Rust version: `1.88`
- License: `Apache-2.0`
- Repository: `https://github.com/NubeDev/hackline`

**Workspace dependencies (shared versions):**
- `tokio` 1.x (features: `full`)
- `zenoh` 1.9 (features: `transport_tcp`)
- `serde` 1.x (features: `derive`)
- `serde_json` 1.x
- `uuid` 1.x (features: `v4`, `serde`)
- `thiserror` 2.x
- `anyhow` 1.x
- `tracing` 0.1.x
- `tracing-subscriber` 0.3.x (features: `env-filter`, `json`)
- `futures` 0.3.x
- `toml` 0.8.x
- `rusqlite` 0.32.x (features: `bundled`)
- `r2d2` 0.8.x
- `r2d2_sqlite` 0.25.x
- `axum` 0.8.x
- `tower-http` 0.6.x (features: `cors`, `trace`)

**Workspace lints:**
- `rust::unsafe_code` = forbid (no unsafe allowed)
- `clippy::all` = warn with priority -1

---

## Document Summaries

### SCOPE.md (Load-bearing Design Doc)

**Problem Statement:** Operate IoT devices without customer port-opening via a Zenoh-native fleet service with:
- **Tunnel plane:** Raw TCP bytes (HTTP, SSH) exposed via cloud gateway
- **Message plane:** Typed JSON envelopes for events, commands, RPC, logs

**Key Design Points:**
1. Two planes on one Zenoh fabric (tunnel + message)
2. One gateway, one CLI, one device-side SDK, SQLite embedded, bearer-token auth
3. Already-deployed Zenoh on every device eliminates second overlay network need

**Architecture:**
- Gateway (cloud VPS, behind Caddy) runs axum REST + SSE + TCP listeners + SQLite + Zenoh client
- Agent (device) bridges Zenoh queries to local TCP services via one session per port
- Device apps via SDK open own Zenoh sessions (no proxy through agent)
- Liveliness tokens + message plane keyexprs separate from tunnel plane

**Non-Goals:** Not a generic ngrok; not multi-tenant SaaS in v0.1; not UDP; no TLS on device; not a TSDB; not unbounded command/event queues.

**Wire Surface:**
- Tunnel plane: `hackline/<zid>/tcp/<port>/connect` (query) + side-channel pub/sub for bytes
- Message plane: `hackline/<zid>/msg/{event,log,cmd,api}/<topic...>` (pub/sub + queryables)
- Liveliness: `@/liveliness/hackline/<zid>`

**REST Surface (gateway):**
- Health & claim: `GET /v1/health`, `GET /v1/claim/status`, `POST /v1/claim`
- Devices: `GET /v1/devices`, `POST /v1/devices`, `GET /v1/devices/:id`, `PATCH /v1/devices/:id`, `DELETE /v1/devices/:id`, `GET /v1/devices/:id/info`, `GET /v1/devices/:id/health`
- Tunnels: `GET /v1/tunnels`, `POST /v1/tunnels`, `DELETE /v1/tunnels/:id`
- Commands: `POST /v1/devices/:id/cmd/:topic`, `GET /v1/devices/:id/cmd?status=…`, `DELETE /v1/cmd/:cmd_id`
- RPC: `POST /v1/devices/:id/api/:topic`
- Events & logs: `GET /v1/events`, `GET /v1/log` (cursor-based pagination)
- Users/tokens: `GET /v1/users`, `POST /v1/users`, `DELETE /v1/users/:id`, `POST /v1/users/:id/tokens`
- Audit: `GET /v1/audit`
- SSE streams: `GET /v1/events/stream`, `GET /v1/devices/:id/events/stream`, `GET /v1/devices/:id/msg/{events,log}/stream`

**SQLite Schema:**
- `meta`: global KV
- `claim_pending`: first-boot token (one row, id=1)
- `users`: name, role (owner|admin|support|viewer|customer), device_scope, tunnel_scope, expires_at
- `tokens`: user_id FK, token_hash (UNIQUE), expires_at (separate from user for multi-token per user)
- `devices`: zid (UNIQUE), label, customer_id, created_at, last_seen_at
- `tunnels`: device_id FK, kind (tcp|http), local_port, public_hostname (http only), public_port (tcp only), enabled, CHECK constraint enforces kind<→host/port mapping
- `audit`: ts, user_id, device_id, tunnel_id, action (text), detail (JSON)
- `cmd_outbox`: (Phase 2 not yet in migration) durable command queue with ack tracking
- `events`: (Phase 1.5) bounded ring per device
- `logs`: (Phase 1.5) bounded ring per device

**Auth:** Bearer-token claim flow (lifted from token-service):
1. Gateway boot: if `users` empty, insert pending claim token (raw, sha256 hash stored)
2. `POST /v1/claim { token, owner }`: atomic consume-pending + create-owner user + create token in one txn
3. Owner can mint scoped tokens (role, device_scope, tunnel_scope, expires_at)
4. All REST/SSE authenticated uniformly via `Authorization: Bearer <token>`
5. Constant-time compare on token lookup

**Constrained Device Support (§3.7):** ESP32s can join as Zenoh clients, participate in message plane only (no tunnels), same keyexpr namespace. Gateway has `POST /v1/devices/issue` to pre-issue credentials for OTA flash.

**Trust Model:**
- Zenoh ZID + ACL enforces per-device isolation on fabric
- Gateway is single point of compromise (by design; protect it)
- App-vs-app on device not a security boundary (shared /etc/hackline/device.pem)
- Each session gets own ZID under same cert chain

**Deployment:** SQLite with WAL for a few thousand devices; Postgres later if >~50k devices. Lexical ordering of retention policies prevents cap+TTL edge cases.

### DECISIONS.md (ADR Log)

**tunnel-engine — 2026-05:** Zenoh (chosen) vs bore/wstunnel/rathole/frp/OpenZiti (rejected). Pub/sub side-channels for bytes, not streaming gets. Overturn if: Zenoh leaves fleet, pub/sub pattern breaks at scale, or ACL evals become O(devices).

**message-broker — 2026-05:** Zenoh pub/sub + queryables (chosen) vs NATS/JetStream/Kafka/MQTT/in-Zenoh durable streams (rejected). Cmd outbox lives in gateway SQLite, not broker. Overturn if: cmd throughput exceeds SQLite WAL (~5k/s write), or fan-in timeseries use case lands.

**client-session-model — 2026-05:** Apps open own `zenoh::Session` (chosen) vs local IPC proxy through agent (rejected). Each app gets its own ZID under device cert chain. Not app-vs-app security boundary. Overturn if: constrained target cannot run Zenoh.

**persistence — 2026-05:** SQLite + rusqlite + r2d2 + spawn_blocking (chosen) vs sqlx/Postgres/Sled (rejected). Async SQLite compile-heavy, query macros need live DB. Overturn if: active-active multi-region, >~50k devices, or prune workload dominates writer.

**events — 2026-05:** SSE (chosen) vs gRPC streaming/WebSocket (rejected). One-directional server→client fits SSE perfectly. Reverse-proxy needs `flush_interval -1` (Caddy) or `proxy_buffering off` (nginx). Overturn if bidirectional needed.

**audit-grain — 2026-05:** One row per bridged TCP session + control-plane mutation (chosen) vs per-HTTP-request or per-event (rejected). Retention: 180 days for `tunnel.session`, indefinite for control-plane. Overturn if regulator demands per-request audit.

**tls-on-device — 2026-05:** Device serves plain HTTP on 127.0.0.1 (chosen) vs per-device Let's Encrypt or self-signed (rejected). All public TLS on cloud gateway (Caddy). Overturn if app refuses plain HTTP config.

**cmd-delivery-semantics — 2026-05:** At-least-once delivery (chosen) vs at-most-once or exactly-once (rejected). `cmd_id` is idempotency key; app dedupes. At-most-once drops across restarts; at-least-once + idempotent is practice. Overturn if use case is genuinely non-idempotent.

**events-retention — 2026-05:** Ring-buffer per device with row cap (chosen) vs time-based only / unbounded / off-by-default (rejected). Default 10k rows per device, oldest pruned on write. Overturn if users want time-windowed history more than row cap.

**payload-format — 2026-05:** JSON for all v0.1 (chosen) vs bincode/MessagePack/Cbor/Protobuf (rejected). `Envelope.content_type` reserved for future bincode addition. Overturn if JSON encode/decode dominates gateway CPU at load.

**core-crate-footprint — 2026-05:** hackline-core may depend on tokio+zenoh (chosen) vs split into pure+impl sub-crates (rejected). Agent already pulls tokio/zenoh; footprint cost zero. Overturn if real no-tokio consumer appears.

**auth-seam-pending — 2026-05:** OPEN. L1+L2 (device+tunnel access) in hackline, L3 (in-device authz) in app. Recommended: Option α (gateway signs `X-Rubix-User` header). Must ratify before Phase 2 ships or customer-facing URL exposed.

**standalone-vs-in-tree — 2026-05:** Hackline standalone (chosen) vs in-tree to rubix (rejected). Consumers depend on hackline-client + hackline-proto only. Wire is contract. Overturn if feature needs transport beyond Zenoh keyexprs + HTTP tunnels.

**data-plane-shape — 2026-05:** Open+side-channel pub/sub (chosen) vs streaming get or persistent session (rejected). Zenoh 1.x replies are single-Sample, not bidi streams. Overturn if future Zenoh release adds true bidi-stream queries.

### HOW-TO-ADD-CODE.md (File Layout & Contribution Rules)

**Rule Zero:** One responsibility per file.
- Max 400 lines/file (split at 300)
- Max 50 lines/function
- Max ~10 public items/module
- Max 4 nesting depth

**REST routes:** One file per `(resource, verb)` — e.g., `api/devices/list.rs`, `api/devices/create.rs`

**CLI subcommands:** One file per leaf command — e.g., `cmd/device/list.rs`, `cmd/device/add.rs`

**Database:** One file per table — e.g., `db/users.rs`, `db/devices.rs`

**Naming:** No `utils.rs`, `helpers.rs`, `common.rs`, `misc.rs` — name the concept.

**Crate dependency direction (hard rules R1–R4):**
- R1: `hackline-proto` is pure types (no tokio, zenoh, filesystem)
- R2: `hackline-agent` + `hackline-gateway` don't depend on each other; shared code lives in `hackline-core` or `hackline-proto`
- R3: Only `*-cli`, `*-agent`, `*-gateway` main.rs install logger, parse argv, or call `exit()`
- R4: SQLite only in `hackline-gateway`

**Code location decision tree:**
- Q1 (wire type) → `crates/hackline-proto/src/`
- Q2 (TCP↔Zenoh bridge) → `crates/hackline-core/src/bridge.rs`
- Q3 (REST endpoint) → `crates/hackline-gateway/src/api/<resource>/<verb>.rs` (handlers max 20 lines, thin)
- Q4 (CLI subcommand) → `crates/hackline-cli/src/cmd/<group>/<verb>.rs`
- Q5 (SQL table) → `crates/hackline-gateway/migrations/V###__<name>.sql` + `crates/hackline-gateway/src/db/<table>.rs`
- Q6 (agent queryable) → `crates/hackline-agent/src/` (one file per concept)
- Q7 (docs) → `DOCS/` or `DECISIONS.md`

**Comment rules:**
1. Doc-comment every public item
2. Explain why, not what
3. No session-progress markers (comments describe code as-is)
4. No emojis, ASCII banners, decoration
5. `TODO` / `FIXME` always carry owner or ticket
6. Stale comment is worse than no comment

**Test rules:**
- Test with code (same PR)
- One test file per source file
- Loopback Zenoh router for E2E (don't mock transport)

**Workflow:**
```
cargo check --workspace
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

**Commit etiquette:**
- Conventional commits: `feat(gateway):`, `fix(agent):`, `docs:`, `chore:`
- Commit only when asked; never amend
- One logical change per commit; body explains why

---

## CRATE 1: hackline-proto

**Purpose:** Wire types and key-expression builders. Pure types only — no I/O, no async, no filesystem.

**Dependencies:** `serde`, `serde_json`, `uuid`, `thiserror`

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-proto/src/lib.rs`

**Purpose:** Module re-export entrypoint.

**Public items:**
- Module `agent_info`
- Module `connect`
- Module `error`
- Module `event`
- Module `keyexpr`
- Module `zid`
- Re-export `ConnectAck`
- Re-export `ConnectRequest`
- Re-export `Zid`

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-proto/src/error.rs`

**Purpose:** Proto-level error type for parsing/construction failures.

**Public items:**
- Enum `ProtoError` (derives Debug, Error)
  - `InvalidZid(String)` — invalid Zenoh device ID format
  - `InvalidKeyExpr(String)` — invalid key expression
  - `Json(serde_json::Error)` — JSON serialization/deserialization error (via #[from])

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-proto/src/connect.rs`

**Purpose:** Per-tunnel-open exchange between gateway and agent.

**Public items:**
- Struct `ConnectRequest` (derives Debug, Clone, Serialize, Deserialize)
  - `request_id: Uuid` — ties gateway/agent log lines together
  - `peer: Option<String>` — peer address for agent audit log
  - Doc comment: Sent by gateway as Zenoh get payload on `hackline/<zid>/tcp/<port>/connect`

- Struct `ConnectAck` (derives Debug, Clone, Serialize, Deserialize)
  - `request_id: Uuid`
  - `ok: bool`
  - `message: Option<String>`
  - Doc comment: Reply from agent; if ok=true, paired pub/sub channels ready for bytes

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-proto/src/event.rs`

**Purpose:** SSE event variants emitted by gateway.

**Public items:**
- Constant `CLOSE_SENTINEL: &[u8] = b""` — zero-length payload signaling stream close
- Enum `Event` (derives Debug, Clone, Serialize, Deserialize with tag="type", rename_all="snake_case")
  - `DeviceOnline { device_id: i64 }`
  - `DeviceOffline { device_id: i64 }`
  - `TunnelOpened { tunnel_id: i64 }`
  - `TunnelClosed { tunnel_id: i64 }`
  - `TunnelConnection { tunnel_id: i64, request_id: Uuid }`

**Invariants:** One enum variant per event type listed in REST-API.md (not yet complete in v0.1 scaffolding).

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-proto/src/zid.rs`

**Purpose:** Zenoh device-id newtype with parsing/validation.

**Public items:**
- Struct `Zid` (derives Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)
  - `Zid(String)` (private field)
  - Doc comment: Validated Zenoh device identifier

- Impl `Zid`
  - `pub fn new(raw: &str) -> Result<Self, ProtoError>` — parse and validate raw string as ZID (lowercase hex, length 2..=32)
  - `pub fn as_str(&self) -> &str` — borrow inner string
  
- Impl `fmt::Display for Zid`
  - Forwards to inner string
  
- Impl `TryFrom<String> for Zid`
  - Calls `Self::new(&s)`
  
- Impl `From<Zid> for String`
  - Unwraps inner string

- Tests: `#[cfg(test)] mod tests`
  - `valid_zid()` — tests "ab", "0123456789abcdef", uppercase normalization
  - `rejects_bad_zid()` — tests empty, single char, non-hex, >32 chars

**Invariants:** Canonical form is lowercase hex 2–32 chars; no separators allowed. All serialization/deserialization goes through the newtype.

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-proto/src/agent_info.rs`

**Purpose:** Payload returned by `hackline/<zid>/info` queryable.

**Public items:**
- Struct `AgentInfo` (derives Debug, Clone, Serialize, Deserialize)
  - `label: Option<String>` — human-readable agent label
  - `allowed_ports: Vec<u16>` — ports this agent will bridge

**Usage:** Gateway queries the agent for its config state; device returns this to advertise available ports.

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-proto/src/keyexpr.rs`

**Purpose:** Key-expression builders (no string concatenation in application code).

**Public items:**
- Function `connect(zid: &Zid, port: u16) -> String` — builds `hackline/<zid>/tcp/<port>/connect`
- Function `info(zid: &Zid) -> String` — builds `hackline/<zid>/info`
- Function `health(zid: &Zid) -> String` — builds `hackline/<zid>/health`
- Function `stream_gw(zid: &Zid, request_id: &Uuid) -> String` — builds `hackline/<zid>/stream/<request_id>/gw` (gateway→agent data)
- Function `stream_dev(zid: &Zid, request_id: &Uuid) -> String` — builds `hackline/<zid>/stream/<request_id>/dev` (agent→gateway data)

- Tests: `#[cfg(test)] mod tests`
  - `keyexpr_shape()` — validates format of each builder

**Invariants:** Catalogue is in `DOCS/KEYEXPRS.md`; every keyexpr in that table is built by exactly one function so a typo cannot slip into the wire.

---

## CRATE 2: hackline-core

**Purpose:** TCP↔Zenoh bridging helpers (TCP socket ↔ Zenoh pub/sub side channels).

**Dependencies:** `hackline-proto`, `tokio`, `zenoh`, `serde_json`, `uuid`, `thiserror`, `tracing`, `futures`

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-core/src/lib.rs`

**Purpose:** Module entrypoint.

**Public items:**
- Module `bridge`
- Module `error`
- Module `session`

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-core/src/error.rs`

**Purpose:** Transport-level error type (distinct from proto errors).

**Public items:**
- Enum `BridgeError` (derives Debug, Error)
  - `Zenoh(zenoh::Error)` — via #[from]
  - `Io(std::io::Error)` — via #[from]
  - `Proto(hackline_proto::error::ProtoError)` — via #[from]
  - `Rejected(String)` — connect ack rejected with message
  - `AckTimeout` — timeout waiting for connect ack

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-core/src/bridge.rs`

**Purpose:** Bidirectional byte bridge between TCP stream and Zenoh pub/sub channels. Connect handshake via one-shot query/reply; data flows on side channels until close.

**Constants:**
- `READ_BUF: usize = 32 * 1024`
- `ACK_TIMEOUT: Duration = Duration::from_secs(10)`
- `QUERY_TIMEOUT: Duration = Duration::from_secs(2)`

**Public items:**
- Async function `accept_bridge(session: &Session, zid: &Zid, port: u16, query: zenoh::query::Query) -> Result<(), BridgeError>`
  - Agent side: accept a connect query, open local TCP socket, run byte bridge until close
  - Parses ConnectRequest from query payload
  - Opens TCP to 127.0.0.1:port
  - Replies with ConnectAck (ok=true or ok=false with message)
  - Drops query to send "final reply" frame (prevents gateway get() timeout hang)
  - Calls `run_bridge()` on side channels
  - Returns BridgeError if TCP connect fails or bridge errors

- Async function `initiate_bridge(session: &Session, zid: &Zid, port: u16, tcp: TcpStream, peer_addr: Option<String>) -> Result<(), BridgeError>`
  - Gateway side: issue connect query, wait for ack, run byte bridge
  - Generates UUID request_id
  - Issues Zenoh get on `hackline/<zid>/tcp/<port>/connect` with ConnectRequest payload
  - Waits for reply with ACK_TIMEOUT
  - Parses ConnectAck; returns Rejected if ok=false
  - Calls `run_bridge()` on side channels
  - Returns BridgeError on any failure

- Async function `run_bridge(session: &Session, tcp: TcpStream, subscribe_ke: &str, publish_ke: &str) -> Result<(), BridgeError>` (private)
  - Splits TCP socket into reader/writer
  - Declares publisher on `publish_ke` and subscriber on `subscribe_ke`
  - Spawns two tasks:
    - `tcp_to_zenoh`: reads TCP, publishes to Zenoh; on EOF publishes CLOSE_SENTINEL
    - `zenoh_to_tcp`: subscribes Zenoh, writes to TCP; on CLOSE_SENTINEL breaks
  - Awaits both with `tokio::try_join!`

**Invariants:**
- Each TCP connection gets one UUID request_id
- Two-directional channels: gateway→agent (`/gw`), agent→gateway (`/dev`)
- Close signaled via empty bytes (CLOSE_SENTINEL from proto)
- Both sides drop query/subscriber when done to clean up Zenoh state
- 32 KiB read buffer for TCP chunks

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-core/src/session.rs`

**Purpose:** Zenoh session open/close helpers (gateway and agent use same semantics).

**Public items:**
- Async function `open(config: zenoh::Config) -> Result<Session, crate::error::BridgeError>`
  - Opens Zenoh session with given config
  - Returns BridgeError on failure

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-core/examples/spike.rs`

**Purpose:** Spike example: two Zenoh peers in one process proving TCP↔Zenoh bridge works end-to-end.

**Behavior:**
1. Starts trivial TCP echo server on 127.0.0.1:9998
2. Opens two Zenoh sessions (agent peer mode on 7447, gateway on 7448)
3. Agent side: declares queryable on `hackline/aa01/tcp/9998/connect`, accepts bridges
4. Gateway side: listens on 127.0.0.1:9999, initiates bridges on each connection
5. Data flow: nc 127.0.0.1:9999 → (gateway) → Zenoh → (agent) → 127.0.0.1:9998

**Constants:**
- `AGENT_ZID: &str = "aa01"`
- `LOCAL_PORT: u16 = 9998`
- `PUBLIC_PORT: u16 = 9999`

**Functions:**
- `#[tokio::main] async fn main() -> Result<()>` — orchestrates the spike
- `async fn run_agent(session: &zenoh::Session, zid: &Zid, port: u16) -> Result<()>` — agent task
- `async fn run_gateway(session: &zenoh::Session, zid: &Zid, public_port: u16, device_port: u16) -> Result<()>` — gateway task
- `async fn run_echo_server(port: u16)` — echo server task
- `fn peer_config(listen_port: u16, connect_port: u16) -> Result<zenoh::Config>` — builds peer config with loopback endpoints, disables multicast

**Dependencies:** `anyhow`, `hackline_core::bridge`, `hackline_proto::Zid`, `tokio`, `tracing`

---

## CRATE 3: hackline-agent

**Purpose:** Device-side binary (`hackline-agent`): bridges Zenoh queries to local TCP services (tunnel plane only).

**Dependencies:** `hackline-proto`, `hackline-core`, `tokio`, `zenoh`, `serde`, `serde_json`, `toml`, `thiserror`, `anyhow`, `tracing`, `tracing-subscriber`

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-agent/src/main.rs`

**Purpose:** Binary entry point — argv parsing, logging subscriber install.

**Public items:** None (main binary)

**Private modules:**
- `mod config` — TOML config loader
- `mod connect` — tunnel queryable handler
- `mod error` — agent error type
- `mod info` — `hackline/<zid>/info` queryable (stub)
- `mod liveliness` — `hackline/<zid>/health` liveliness token (stub)

**Function:** `#[tokio::main] async fn main() -> anyhow::Result<()>`
1. Reads config path from argv[1] or defaults to "agent.toml"
2. Loads `AgentConfig::load(&config_path)`
3. Initializes tracing (env-filter, json or pretty format)
4. Parses ZID from config
5. Converts config to Zenoh config, opens session
6. Calls `connect::serve_connect(session, &zid, &cfg.allowed_ports)`
7. Returns anyhow::Result

**Function:** `fn init_tracing(level: &str, format: &str)`
- Creates EnvFilter from env or defaults to `level` param
- Initializes tracing-subscriber with either json or pretty format

**Invariants:** Only main.rs and binaries call `std::process::exit` or install logging (R3).

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-agent/src/config.rs`

**Purpose:** TOML config loader (schema in `DOCS/CONFIG.md`). Unknown keys rejected to catch typos.

**Public items:**
- Struct `AgentConfig` (derives Debug, Deserialize with deny_unknown_fields)
  - `zid: String` — Zenoh device ID
  - `allowed_ports: Vec<u16>` — ports agent will bridge
  - `label: Option<String>` — human-readable label
  - `zenoh: ZenohConfig`
  - `log: LogConfig` (with default)

- Struct `ZenohConfig` (derives Debug, Deserialize with deny_unknown_fields)
  - `mode: String` (default "peer")
  - `listen: Vec<String>` (default empty)
  - `connect: Vec<String>` (default empty)

- Struct `LogConfig` (derives Debug, Deserialize with deny_unknown_fields)
  - `level: String` (default "info")
  - `format: String` (default "pretty")
  - Impl `Default` — returns defaults

- Functions (const defaults):
  - `fn default_mode() -> String { "peer".into() }`
  - `fn default_log_level() -> String { "info".into() }`
  - `fn default_log_format() -> String { "pretty".into() }`

- Impl `AgentConfig`
  - `pub fn load(path: &Path) -> Result<Self, AgentError>` — reads file, parses TOML, validates allowed_ports not empty
  - `pub fn to_zenoh_config(&self) -> Result<zenoh::Config, AgentError>` — builds Zenoh Config from TOML, inserts mode/listen/connect/scouting settings, disables multicast

**Invariants:** allowed_ports must not be empty; disables multicast scouting to avoid interference.

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-agent/src/error.rs`

**Purpose:** Agent-level error type.

**Public items:**
- Enum `AgentError` (derives Debug, Error)
  - `Config(String)` — configuration error
  - `Bridge(hackline_core::error::BridgeError)` — via #[from]
  - `Zenoh(zenoh::Error)` — via #[from]
  - `Io(std::io::Error)` — via #[from]
  - `PortDenied(u16)` — requested port not in allowed_ports

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-agent/src/connect.rs`

**Purpose:** Handles `hackline/<zid>/tcp/<port>/connect` queries. Validates port against whitelist before opening loopback TCP and handing to bridge.

**Public items:**
- Async function `serve_connect(session: Arc<Session>, zid: &Zid, allowed_ports: &[u16]) -> Result<(), AgentError>`
  - Declares one queryable per allowed port
  - Spawns one task per port that loops on queryable.recv_async()
  - On query, spawns async task to `bridge::accept_bridge()` (don't block queryable loop)
  - Logs queryable ready on each port
  - Blocks until all queryable tasks complete (forever under normal operation)

**Invariants:** One queryable per port; port whitelist enforced before TCP connect (defense in depth — gateway also enforces via tunnel rows). Agent stays a thin bridge with no IPC, no local auth, no fan-out.

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-agent/src/info.rs`

**Stub:** Doc comment only — "Serves `hackline/<zid>/info` — returns an `AgentInfo` reply."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-agent/src/liveliness.rs`

**Stub:** Doc comment only — "Declares the `hackline/<zid>/health` liveliness token. The gateway watches this to surface online/offline status."

---

## CRATE 4: hackline-gateway

**Purpose:** Cloud-side library + binary (`hackline-gateway serve`): REST + SSE + TCP listeners + SQLite + Zenoh client.

**Dependencies:** `hackline-proto`, `hackline-core`, `tokio`, `zenoh`, `serde`, `serde_json`, `toml`, `thiserror`, `anyhow`, `tracing`, `tracing-subscriber`, `rusqlite`, `r2d2`, `r2d2_sqlite`, `axum`, `tower-http`

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/lib.rs`

**Purpose:** Library entrypoint; exposes modules to binaries and tests.

**Public items:**
- Module `api`
- Module `auth`
- Module `config`
- Module `db`
- Module `error`
- Module `events_bus`
- Module `state`
- Module `tunnel`
- Module `zenoh_client`

**Note:** Binaries auto-discovered from `src/bin/*`:
- `serve` — run the gateway
- `print-claim` — print pending claim token (read-only)
- `reset-claim` — wipe users + reissue claim (destructive)

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/error.rs`

**Purpose:** Gateway error type mapping to HTTP status codes via IntoResponse impl.

**Public items:**
- Enum `GatewayError` (derives Debug, Error)
  - `Config(String)` — configuration error → 500
  - `Bridge(hackline_core::error::BridgeError)` — via #[from] → 500
  - `Zenoh(zenoh::Error)` — via #[from] → 500
  - `Io(std::io::Error)` — via #[from] → 500
  - `Proto(hackline_proto::error::ProtoError)` — via #[from] → 500
  - `Db(rusqlite::Error)` — via #[from] → 500
  - `Pool(r2d2::Error)` — via #[from] → 500
  - `NotFound` → 404
  - `BadRequest(String)` → 400

- Impl `IntoResponse for GatewayError`
  - Maps each variant to (StatusCode, JSON error response)
  - Most errors return 500 with generic "internal error" message (don't leak internals)
  - NotFound returns 404, BadRequest returns 400 with specific message

**Invariants:** Handlers can `?` freely; error impl converts to HTTP response automatically.

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/config.rs`

**Purpose:** TOML config loader (schema in `DOCS/CONFIG.md`). Unknown keys rejected.

**Public items:**
- Struct `GatewayConfig` (derives Debug, Deserialize with deny_unknown_fields)
  - `listen: Option<String>` — HTTP listen address (default 127.0.0.1:8080)
  - `database: Option<String>` — SQLite path (default gateway.db)
  - `zenoh: ZenohConfig`
  - `tunnels: Vec<TunnelEntry>` (default empty; used for tunnel manager)
  - `log: LogConfig` (default)

- Struct `ZenohConfig` (derives Debug, Deserialize with deny_unknown_fields)
  - `mode: String` (default "client")
  - `listen: Vec<String>` (default empty)
  - `connect: Vec<String>` (default empty)

- Struct `TunnelEntry` (derives Debug, Clone, Deserialize with deny_unknown_fields)
  - `zid: String` — device ZID
  - `device_port: u16` — local port on device
  - `listen_port: u16` — public listener port

- Struct `LogConfig` (derives Debug, Deserialize with deny_unknown_fields)
  - `level: String` (default "info")
  - `format: String` (default "pretty")
  - Impl `Default`

- Const functions:
  - `fn default_mode() -> String { "client".into() }`
  - `fn default_log_level() -> String { "info".into() }`
  - `fn default_log_format() -> String { "pretty".into() }`

- Impl `GatewayConfig`
  - `pub fn load(path: &Path) -> Result<Self, GatewayError>` — reads file, parses TOML
  - `pub fn to_zenoh_config(&self) -> Result<zenoh::Config, GatewayError>` — builds Zenoh Config, inserts mode/listen/connect/scouting, disables multicast

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/state.rs`

**Purpose:** Process-wide application state passed to every axum handler.

**Public items:**
- Struct `AppState` (derives Clone)
  - `db: DbPool` — r2d2 connection pool
  - `zenoh: Arc<zenoh::Session>` — single Zenoh session for the gateway

**Invariants:** Concrete (no `dyn`); tests build real state against loopback Zenoh router rather than mocking.

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/zenoh_client.rs`

**Stub:** Doc comment only — "The single Zenoh session the gateway holds open. Wraps `hackline-core::session` with gateway-specific config (ACL, discovery mode, reconnect policy)."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/events_bus.rs`

**Stub:** Doc comment only — "In-process broadcast bus that fans gateway events out to every connected SSE subscriber. Backed by `tokio::sync::broadcast`; lagging subscribers see a `Lagged` error and reconnect."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/db/mod.rs`

**Purpose:** SQLite repository module entrypoint. All sync; called from async handlers via `tokio::task::spawn_blocking`. Pool max-size ≤ tokio blocking-thread budget.

**Public items:**
- Module `audit` — audit table repository
- Module `claim` — claim_pending atomic flow
- Module `devices` — devices table repository
- Module `migrations` — refinery migrations runner
- Module `pool` — r2d2 pool setup
- Module `tunnels` — tunnels table repository
- Module `users` — users table repository

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/db/pool.rs`

**Purpose:** r2d2 pool setup. Opens SQLite with WAL mode, foreign-key pragma, timeout.

**Public items:**
- Type alias `DbPool = Pool<SqliteConnectionManager>`
- Function `open(path: &Path) -> Result<DbPool, GatewayError>`
  - Creates `SqliteConnectionManager::file(path)`
  - Initializes connection with:
    - `PRAGMA journal_mode = WAL;` — write-ahead logging
    - `PRAGMA foreign_keys = ON;` — enforce foreign keys
    - `PRAGMA busy_timeout = 5000;` — 5s timeout on lock
  - Builds Pool with max_size=16 (conservative against tokio blocking-thread budget of 512)
  - Returns GatewayError on pool build failure

**Invariants:** Pool size must stay ≤ tokio blocking-thread budget. 16 is safe default; raising requires raising blocking-thread budget in same change.

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/db/migrations.rs`

**Purpose:** Embedded refinery migrations (V001__init.sql). Run on every boot; idempotent.

**Public items:**
- Constant `V001_INIT: &str = include_str!("../../migrations/V001__init.sql")`
- Function `run(conn: &Connection) -> Result<(), GatewayError>`
  - Creates `_migrations` table if not exists
  - Checks if version 1 applied
  - If not applied: executes V001_INIT, inserts migration record, logs info
  - Returns GatewayError on any DB error

**Invariants:** Tracks applied migrations in `_migrations(version, name, applied_at)` table. One-shot per boot.

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/db/devices.rs`

**Purpose:** `devices` table repository: insert, list, get, delete.

**Public items:**
- Struct `Device` (derives Debug, Serialize)
  - `id: i64` — primary key
  - `zid: String` — Zenoh device ID (UNIQUE)
  - `label: String` — human-readable label
  - `customer_id: Option<i64>` — FK to customer (Phase 2)
  - `created_at: i64` — unix timestamp
  - `last_seen_at: Option<i64>` — unix timestamp of last liveliness or query reply

- Function `insert(conn: &Connection, zid: &str, label: &str) -> Result<Device, GatewayError>`
  - INSERTs row with created_at=unixepoch(), returns last_insert_rowid, calls get() to fetch full row

- Function `list(conn: &Connection) -> Result<Vec<Device>, GatewayError>`
  - SELECT all devices ORDER BY id

- Function `get(conn: &Connection, id: i64) -> Result<Device, GatewayError>`
  - SELECT by id; returns NotFound on no rows

- Function `delete(conn: &Connection, id: i64) -> Result<bool, GatewayError>`
  - DELETE by id; returns bool indicating whether row was deleted

**Invariants:** ZID UNIQUE constraint enforced by schema. Cascading deletes on tunnels via FK.

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/db/tunnels.rs`

**Purpose:** `tunnels` table repository. Kind/hostname/port CHECK constraint in migration; this layer maps rows.

**Public items:**
- Struct `Tunnel` (derives Debug, Clone, Serialize)
  - `id: i64`
  - `device_id: i64` — FK to devices
  - `kind: String` — "tcp" or "http"
  - `local_port: i64` — device-side port
  - `public_hostname: Option<String>` — HTTP only (e.g., "device-1234.cloud.com")
  - `public_port: Option<i64>` — TCP only
  - `enabled: bool` — whether listener is active
  - `created_at: i64`

- Struct `TunnelWithZid` (derives Debug, Clone)
  - `id: i64`
  - `zid: String` — device ZID (joined from devices table)
  - `kind: String`
  - `local_port: u16`
  - `public_port: u16`
  - `enabled: bool`
  - Used by tunnel manager to spin up listeners

- Function `insert(conn: &Connection, device_id: i64, kind: &str, local_port: i64, public_hostname: Option<&str>, public_port: Option<i64>) -> Result<Tunnel, GatewayError>`
  - INSERTs row with created_at=unixepoch(), returns full row via get()

- Function `list(conn: &Connection) -> Result<Vec<Tunnel>, GatewayError>`
  - SELECT all tunnels ORDER BY id

- Function `get(conn: &Connection, id: i64) -> Result<Tunnel, GatewayError>`
  - SELECT by id; returns NotFound on no rows

- Function `delete(conn: &Connection, id: i64) -> Result<bool, GatewayError>`
  - DELETE by id

- Function `list_active_tcp(conn: &Connection) -> Result<Vec<TunnelWithZid>, GatewayError>` (important for tunnel manager)
  - SELECT WHERE enabled=1 AND kind='tcp' AND public_port IS NOT NULL
  - Joins devices table to get ZID
  - Used by tunnel::manager to load startup tunnels

- Private function `fn row_to_tunnel(row: &rusqlite::Row) -> rusqlite::Result<Tunnel>` (helper)

**Invariants:** Schema CHECK enforces (kind='http' → public_hostname NOT NULL, public_port NULL) OR (kind='tcp' → public_port NOT NULL, public_hostname NULL). public_hostname and public_port both have UNIQUE constraints.

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/db/users.rs`

**Stub:** Doc comment only — "`users` table repository: insert, lookup-by-token-hash, list, delete, scope checks."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/db/audit.rs`

**Stub:** Doc comment only — "`audit` table repository: append + cursor-paginated read. Retention strategy is documented in `DOCS/DATABASE.md`."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/db/claim.rs`

**Stub:** Doc comment only — "`claim_pending` row + the atomic claim consumption transaction."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/tunnel/mod.rs`

**Purpose:** Public-facing tunnel listeners and bridge to hackline-core.

**Public items:**
- Module `bridge` — glue to hackline-core::bridge
- Module `http_router` — HTTP host routing (Phase 2)
- Module `manager` — watches tunnels table, opens/closes listeners
- Module `tcp_listener` — per-tunnel TCP listener

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/tunnel/manager.rs`

**Purpose:** Watches `tunnels` table and opens/closes listeners to match. Single source of truth for "which listeners are live right now."

**Public items:**
- Async function `run(db: DbPool, session: Arc<Session>) -> Result<(), GatewayError>`
  - Loads active TCP tunnels from DB via `tunnels::list_active_tcp()`
  - If none, returns pending to block forever (awaits `std::future::pending()`)
  - Otherwise spawns one listener task per active tunnel
  - Spawns each with `tokio::spawn()`
  - Awaits all handles
  - Returns GatewayError on DB/startup failures

**Invariants:** Runs concurrently with axum server in main binary via `tokio::select!`. Logs startup count.

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/tunnel/tcp_listener.rs`

**Purpose:** Per-tunnel TCP listener. One task per `kind = 'tcp'` row; accepts connections, hands each to bridge.

**Public items:**
- Async function `run_tcp_listener(session: Arc<Session>, zid: Zid, device_port: u16, listen_port: u16) -> Result<(), GatewayError>`
  - Binds TcpListener to 0.0.0.0:listen_port
  - Logs ready (listen_port, zid, device_port)
  - Loops on listener.accept()
  - On each connection, spawns task calling `hackline_core::bridge::initiate_bridge()` with peer address
  - Returns GatewayError on bind failure or loop errors

**Invariants:** Listener stays open forever; processes one connection per iteration.

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/tunnel/bridge.rs`

**Stub:** Doc comment only — "Glue between an accepted TCP/HTTP socket and `hackline-core::bridge`. Issues the Zenoh `connect` query, validates the ack, and starts the byte copy."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/auth/mod.rs`

**Purpose:** Bearer-token auth: hashing, claim flow, scope enforcement.

**Public items:**
- Module `claim` — first-boot claim flow
- Module `middleware` — axum extractor for auth
- Module `scope` — device_scope / tunnel_scope enforcement
- Module `token` — token generation, SHA-256 hashing, constant-time compare

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/auth/token.rs`

**Stub:** Doc comment only — "Token generation, SHA-256 hashing, constant-time compare via `subtle`. Tokens are 32-byte URL-safe; only the hash hits disk."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/auth/claim.rs`

**Stub:** Doc comment only — "First-boot claim flow. The pending row is generated at startup if `users` is empty; `POST /v1/claim` consumes it atomically (delete + insert in one transaction so two simultaneous claimants cannot both win)."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/auth/scope.rs`

**Stub:** Doc comment only — "`device_scope` / `tunnel_scope` enforcement for non-owner roles. Called by handlers after the auth extractor has identified the user."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/auth/middleware.rs`

**Stub:** Doc comment only — "axum extractor that authenticates `Authorization: Bearer <token>` against the `users` table and returns an `AuthedUser` to handlers."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/mod.rs`

**Purpose:** REST + SSE surface. One file per (resource, verb); full catalogue in `DOCS/REST-API.md`.

**Public items:**
- Module `audit` — audit log endpoints
- Module `claim` — claim endpoints
- Module `devices` — device endpoints
- Module `events` — SSE event endpoints
- Module `health` — health check endpoint
- Module `router` — axum router builder
- Module `tunnels` — tunnel endpoints
- Module `users` — user + token endpoints

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/router.rs`

**Purpose:** Builds axum `Router` by composing every handler. Only file knowing full URL surface.

**Public items:**
- Function `build(state: AppState) -> Router`
  - Creates Router with routes:
    - `GET /v1/health` → `super::health::get`
    - `GET /v1/devices` → `super::devices::list::handler`
    - `POST /v1/devices` → `super::devices::create::handler`
    - `GET /v1/devices/{id}` → `super::devices::get::handler`
    - `DELETE /v1/devices/{id}` → `super::devices::delete::handler`
    - `GET /v1/tunnels` → `super::tunnels::list::handler`
    - `POST /v1/tunnels` → `super::tunnels::create::handler`
    - `DELETE /v1/tunnels/{id}` → `super::tunnels::delete::handler`
  - Attaches state to all routes

**Invariants:** All routes in one place so the API surface is visible at a glance.

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/health.rs`

**Purpose:** `GET /v1/health` — unauthenticated liveness probe.

**Public items:**
- Async function `get() -> Json<serde_json::Value>`
  - Returns `{ "status": "ok" }`

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/devices/mod.rs`

**Purpose:** Device endpoints module entrypoint.

**Public items:**
- Module `create` — POST /v1/devices
- Module `delete` — DELETE /v1/devices/:id
- Module `get` — GET /v1/devices/:id
- Module `health` — GET /v1/devices/:id/health (stub)
- Module `info` — GET /v1/devices/:id/info (stub)
- Module `list` — GET /v1/devices
- Module `patch` — PATCH /v1/devices/:id (stub)

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/devices/list.rs`

**Purpose:** `GET /v1/devices` — list devices visible to caller's scope.

**Public items:**
- Async function `handler(State(state): State<AppState>) -> Result<Json<Vec<devices::Device>>, GatewayError>`
  - Gets DB connection from state
  - Spawns blocking task calling `devices::list(&conn)`
  - Returns Json<Vec<Device>> or GatewayError

**Invariants:** Handlers max 20 lines; business logic in db/ or domain module. Thin layer pattern.

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/devices/create.rs`

**Purpose:** `POST /v1/devices` — register device by ZID.

**Public items:**
- Struct `CreateDevice` (derives Deserialize)
  - `zid: String`
  - `label: String`

- Async function `handler(State(state): State<AppState>, Json(body): Json<CreateDevice>) -> Result<(StatusCode::CREATED, Json<devices::Device>), GatewayError>`
  - Gets DB connection
  - Spawns blocking task calling `devices::insert(&conn, &body.zid, &body.label)`
  - Returns (201 CREATED, Device) or GatewayError

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/devices/get.rs`

**Purpose:** `GET /v1/devices/:id`.

**Public items:**
- Async function `handler(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<devices::Device>, GatewayError>`
  - Gets DB connection
  - Spawns blocking task calling `devices::get(&conn, id)`
  - Returns Json<Device> or GatewayError (NotFound if not found)

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/devices/delete.rs`

**Purpose:** `DELETE /v1/devices/:id` — cascades to `tunnels` via FK.

**Public items:**
- Async function `handler(State(state): State<AppState>, Path(id): Path<i64>) -> Result<StatusCode, GatewayError>`
  - Gets DB connection
  - Spawns blocking task calling `devices::delete(&conn, id)`
  - Returns 204 NO_CONTENT if deleted, or NotFound

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/devices/patch.rs`

**Stub:** Doc comment only — "`PATCH /v1/devices/:id` — mutates `label`, `customer_id`. Other fields are rejected at the deserializer."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/devices/health.rs`

**Stub:** Doc comment only — "`GET /v1/devices/:id/health` — last-seen + liveliness latency."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/devices/info.rs`

**Stub:** Doc comment only — "`GET /v1/devices/:id/info` — issues a Zenoh query against `hackline/<zid>/info` and returns the agent's `AgentInfo`."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/tunnels/mod.rs`

**Purpose:** Tunnel endpoints module entrypoint.

**Public items:**
- Module `create` — POST /v1/tunnels
- Module `delete` — DELETE /v1/tunnels/:id
- Module `list` — GET /v1/tunnels

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/tunnels/list.rs`

**Purpose:** `GET /v1/tunnels`.

**Public items:**
- Async function `handler(State(state): State<AppState>) -> Result<Json<Vec<tunnels::Tunnel>>, GatewayError>`
  - Gets DB connection
  - Spawns blocking task calling `tunnels::list(&conn)`
  - Returns Json<Vec<Tunnel>> or GatewayError

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/tunnels/create.rs`

**Purpose:** `POST /v1/tunnels` — opens new public listener for TCP or registers host route for HTTP.

**Public items:**
- Struct `CreateTunnel` (derives Deserialize)
  - `device_id: i64`
  - `kind: String` — "tcp" or "http"
  - `local_port: i64`
  - `public_hostname: Option<String>` — for HTTP
  - `public_port: Option<i64>` — for TCP

- Async function `handler(State(state): State<AppState>, Json(body): Json<CreateTunnel>) -> Result<(StatusCode::CREATED, Json<tunnels::Tunnel>), GatewayError>`
  - Gets DB connection
  - Spawns blocking task calling `tunnels::insert()` with body fields
  - Returns (201 CREATED, Tunnel) or GatewayError

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/tunnels/delete.rs`

**Purpose:** `DELETE /v1/tunnels/:id` — closes listener, deletes row.

**Public items:**
- Async function `handler(State(state): State<AppState>, Path(id): Path<i64>) -> Result<StatusCode, GatewayError>`
  - Gets DB connection
  - Spawns blocking task calling `tunnels::delete(&conn, id)`
  - Returns 204 NO_CONTENT if deleted, or NotFound

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/users/mod.rs`

**Purpose:** User + token endpoints module entrypoint.

**Public items:**
- Module `create` — POST /v1/users (stub)
- Module `delete` — DELETE /v1/users/:id (stub)
- Module `list` — GET /v1/users (stub)
- Module `mint_token` — POST /v1/users/:id/tokens (stub)

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/users/list.rs`

**Stub:** Doc comment only — "`GET /v1/users` — admin only."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/users/create.rs`

**Stub:** Doc comment only — "`POST /v1/users` — owner mints a scoped user."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/users/delete.rs`

**Stub:** Doc comment only — "`DELETE /v1/users/:id`."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/users/mint_token.rs`

**Stub:** Doc comment only — "`POST /v1/users/:id/tokens` — issue a new token for an existing user. Returns the raw token once; only the hash is persisted."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/claim/mod.rs`

**Purpose:** Claim endpoints module entrypoint.

**Public items:**
- Module `post` — POST /v1/claim (stub)
- Module `status` — GET /v1/claim/status (stub)

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/claim/post.rs`

**Stub:** Doc comment only — "`POST /v1/claim` — atomic consume-pending + insert-owner."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/claim/status.rs`

**Stub:** Doc comment only — "`GET /v1/claim/status` — `{ claimed, can_claim }`."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/events/mod.rs`

**Purpose:** SSE event endpoints module entrypoint.

**Public items:**
- Module `all` — GET /v1/events (stub)
- Module `per_device` — GET /v1/devices/:id/events (stub)

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/events/all.rs`

**Stub:** Doc comment only — "`GET /v1/events` — admin SSE feed of all events."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/events/per_device.rs`

**Stub:** Doc comment only — "`GET /v1/devices/:id/events` — SSE feed scoped to one device."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/audit/mod.rs`

**Purpose:** Audit-log read endpoints module entrypoint.

**Public items:**
- Module `list` — GET /v1/audit (stub)

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/api/audit/list.rs`

**Stub:** Doc comment only — "`GET /v1/audit?cursor=&limit=` — cursor-paginated audit feed."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/bin/serve.rs`

**Purpose:** `hackline-gateway serve` — boot gateway, bind every listener.

**Public items:** None (binary)

**Function:** `#[tokio::main] async fn main() -> anyhow::Result<()>`
1. Reads config path from argv[1] or defaults to "gateway.toml"
2. Loads `GatewayConfig::load(&config_path)`
3. Initializes tracing
4. Logs startup
5. Gets DB path from config or defaults to "gateway.db"
6. Opens DB pool via `pool::open()`
7. Gets connection, runs migrations via `migrations::run(&conn)`
8. Logs DB ready
9. Converts config to Zenoh config, opens session
10. Logs Zenoh ZID
11. Creates AppState with db + zenoh
12. Gets listen address from config or defaults to "127.0.0.1:8080"
13. Builds axum router via `api::router::build(state)`
14. Binds TcpListener to listen_addr
15. Logs listening
16. Runs axum server + tunnel manager concurrently via `tokio::select!`
17. Returns anyhow::Result

**Function:** `fn init_tracing(level: &str, format: &str)`
- Creates EnvFilter from env or defaults to `level`
- Initializes tracing-subscriber with json or pretty format

**Invariants:** Only main.rs installs logger and parses argv (R3).

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/bin/print_claim.rs`

**Purpose:** `hackline-gateway print-claim` — print pending claim token (read-only).

**Public items:** None (binary)

**Function:** `fn main()`
- `unimplemented!("hackline-gateway print-claim: scaffolding only")`

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/src/bin/reset_claim.rs`

**Purpose:** `hackline-gateway reset-claim` — wipe `users` table, seed new pending claim (destructive recovery).

**Public items:** None (binary)

**Function:** `fn main()`
- `unimplemented!("hackline-gateway reset-claim: scaffolding only")`

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-gateway/migrations/V001__init.sql`

**Purpose:** Initial schema. Mirrors DOCS/DATABASE.md exactly.

**Tables:**
1. `meta` — global KV store
   - `key TEXT PRIMARY KEY`
   - `value TEXT NOT NULL`

2. `claim_pending` — first-boot token
   - `id INTEGER PRIMARY KEY CHECK (id = 1)` — one row only
   - `token_hash TEXT NOT NULL`
   - `created_at INTEGER NOT NULL`

3. `users` — authenticated users
   - `id INTEGER PRIMARY KEY`
   - `name TEXT NOT NULL` (implied UNIQUE, not explicit in migration; SCOPE says UNIQUE)
   - `role TEXT NOT NULL` CHECK(role IN ('owner','admin','support','viewer','customer'))
   - `token_hash TEXT NOT NULL UNIQUE`
   - `device_scope TEXT NOT NULL DEFAULT '*'` — "*" or JSON array of device IDs
   - `tunnel_scope TEXT NOT NULL DEFAULT '*'` — "*" or JSON array of tunnel IDs
   - `expires_at INTEGER`
   - `created_at INTEGER NOT NULL`
   - `last_used_at INTEGER`
   - **Schema note:** Migration has `token_hash` on users; SCOPE describes separate `tokens` table. This is inconsistency (scaffolding stage).

4. `devices` — registered devices
   - `id INTEGER PRIMARY KEY`
   - `zid TEXT NOT NULL UNIQUE`
   - `label TEXT NOT NULL`
   - `customer_id INTEGER`
   - `created_at INTEGER NOT NULL`
   - `last_seen_at INTEGER`

5. `tunnels` — HTTP/TCP tunnels
   - `id INTEGER PRIMARY KEY`
   - `device_id INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE`
   - `kind TEXT NOT NULL` CHECK(kind IN ('tcp','http'))
   - `local_port INTEGER NOT NULL`
   - `public_hostname TEXT` UNIQUE
   - `public_port INTEGER` UNIQUE
   - `enabled INTEGER NOT NULL DEFAULT 1`
   - `created_at INTEGER NOT NULL`
   - CHECK: (kind='http' AND public_hostname IS NOT NULL AND public_port IS NULL) OR (kind='tcp' AND public_port IS NOT NULL AND public_hostname IS NULL)

6. `audit` — audit log
   - `id INTEGER PRIMARY KEY`
   - `ts INTEGER NOT NULL`
   - `user_id INTEGER REFERENCES users(id)`
   - `device_id INTEGER REFERENCES devices(id)`
   - `tunnel_id INTEGER REFERENCES tunnels(id)`
   - `action TEXT NOT NULL`
   - `detail TEXT` — JSON
   - Indexes: audit_ts (ts), audit_device (device_id)

**Notable differences from SCOPE §7.2:**
- Missing: `cmd_outbox`, `events`, `logs` tables (Phase 1.5+)
- `audit` table simplified: only ts, action, detail (missing ts_open/ts_close, request_id, peer, bytes_up/down — scaffolding only)
- `users` table has `token_hash` (not separate `tokens` table yet)
- No explicit UNIQUE on `users.name` in migration (SCOPE requires it)

---

## CRATE 5: hackline-cli

**Purpose:** `hackline` CLI: thin REST/SSE client over gateway (no business logic).

**Dependencies:** `hackline-proto` only (deliberately NOT `hackline-core` or `hackline-gateway`; pure HTTP/SSE client).

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/main.rs`

**Purpose:** Binary entry point — argv parsing, logging subscriber install.

**Public items:** None (binary)

**Private modules:**
- `mod client` — thin reqwest wrapper
- `mod cmd` — subcommand modules
- `mod config` — credentials cache + env-var overrides
- `mod error` — CLI error type
- `mod output` — output formatting

**Function:** `fn main()`
- `unimplemented!("hackline-cli: scaffolding only")`

**Note:** Scaffolding indicates argv parsing and main dispatch not yet wired.

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/error.rs`

**Stub:** Doc comment only — "CLI error type. Exit codes: 0=success, 1=unexpected, 2=invalid usage (clap), 3=gateway 4xx, 4=gateway 5xx/unreachable."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/config.rs`

**Stub:** Doc comment only — "Credentials cache + env-var overrides. Cache file lives at `$XDG_CONFIG_HOME/hackline/credentials.json`, mode 0600."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/client.rs`

**Stub:** Doc comment only — "Thin `reqwest` wrapper that injects `Authorization: Bearer …` and decodes JSON via the types from `hackline-proto`."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/output.rs`

**Stub:** Doc comment only — "Output formatting. Default human-readable tables; `--json` for machine consumption. No business logic."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/mod.rs`

**Purpose:** Subcommand modules entrypoint.

**Public items:**
- Module `device` — device subcommands
- Module `events` — events subcommand
- Module `login` — login subcommand
- Module `token` — token subcommands
- Module `tunnel` — tunnel subcommands
- Module `user` — user subcommands
- Module `whoami` — whoami subcommand

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/login.rs`

**Stub:** Doc comment only — "`hackline login --server URL --token TOKEN [--owner NAME]`. For the very first call against a fresh gateway, this is the claim flow; subsequent calls just cache credentials."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/whoami.rs`

**Stub:** Doc comment only — "`hackline whoami` — print the current cached identity + scope."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/events.rs`

**Stub:** Doc comment only — "`hackline events [--device ID]` — follow the SSE feed."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/device/mod.rs`

**Purpose:** Device subcommands entrypoint.

**Public items:**
- Module `add` — add subcommand
- Module `list` — list subcommand
- Module `remove` — remove subcommand
- Module `show` — show subcommand

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/device/list.rs`

**Stub:** Doc comment only — "`hackline device list`."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/device/add.rs`

**Stub:** Doc comment only — "`hackline device add --zid ZID --label TEXT`."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/device/show.rs`

**Stub:** Doc comment only — "`hackline device show ID`."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/device/remove.rs`

**Stub:** Doc comment only — "`hackline device remove ID`."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/tunnel/mod.rs`

**Purpose:** Tunnel subcommands entrypoint.

**Public items:**
- Module `add` — add subcommand
- Module `list` — list subcommand
- Module `remove` — remove subcommand

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/tunnel/list.rs`

**Stub:** Doc comment only — "`hackline tunnel list`."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/tunnel/add.rs`

**Stub:** Doc comment only — "`hackline tunnel add --device ID --tcp PORT [--public-port N]` or `hackline tunnel add --device ID --http PORT --hostname HOST`."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/tunnel/remove.rs`

**Stub:** Doc comment only — "`hackline tunnel remove ID`."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/user/mod.rs`

**Purpose:** User subcommands entrypoint.

**Public items:**
- Module `add` — add subcommand
- Module `list` — list subcommand
- Module `remove` — remove subcommand

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/user/list.rs`

**Stub:** Doc comment only — "`hackline user list`."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/user/add.rs`

**Stub:** Doc comment only — "`hackline user add --name NAME --role ROLE [--devices ID,...] [--expires DUR]`."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/user/remove.rs`

**Stub:** Doc comment only — "`hackline user remove ID`."

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/token/mod.rs`

**Purpose:** Token subcommands entrypoint.

**Public items:**
- Module `mint` — mint subcommand

---

### File: `/home/user/code/rust/codeless-workspace/hackline/crates/hackline-cli/src/cmd/token/mint.rs`

**Stub:** Doc comment only — "`hackline token mint --user ID` — prints the new token once."

---

## Summary of Implementation Status

**Phase 1 — Tunnel plane happy path (v0.1 minimum demo):**
- ✅ All crates scaffolded, `cargo check` clean
- ✅ `hackline-core` bridge complete (TCP↔Zenoh byte transfer)
- ✅ `hackline-proto` wire types complete
- ✅ `hackline-agent` config loading + connect queryables ready (with stubs for info/liveliness)
- ✅ `hackline-gateway` basic structure (db, error, config, state, tunnel manager/listener, API router + device/tunnel handlers)
- ⚠️ SQLite schema (V001__init.sql) has inconsistency: `users.token_hash` vs SCOPE's separate `tokens` table
- 🚫 Many API endpoints are stubs (users, claim, events, audit)
- 🚫 hackline-cli is scaffolding only

**Phase 1.5 — Message plane: events + logs (not started)**

**Phase 2 — Message plane: commands + api + HTTP host-routing (not started)**

**Phase 3 — Audit completeness + admin UI (not started)**

---

This completes the exhaustive file-by-file analysis of the Hackline Rust workspace.
