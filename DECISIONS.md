# hackline — DECISIONS

Short ADR log of choices that future agents will be tempted to
revisit. Each entry: what was chosen, what was rejected, and the
single sentence that should change your mind if you want to overturn
it.

---

## tunnel-engine — 2026-05

**Chosen:** Zenoh as the byte transport. Each TCP tunnel = one Zenoh
`get` (`Open` RPC) plus a per-request pub/sub pair on
`hackline/<zid>/tcp/<port>/<request_id>/{up,down}`.

**Rejected:**

- **bore** — one shared fleet secret, no per-device identity, no
  revocation, no audit; ~400 LOC hobby project, bus factor 1.
- **wstunnel** — designed for "one developer bypassing a corporate
  proxy"; auth is a shared `--http-upgrade-path-prefix` secret + mTLS
  CA, neither matches per-device fleet management.
- **rathole** — last release Oct 2023, repo moved orgs, latest CI
  failing; tokens are config-file entries, no runtime API.
- **frp** — actively maintained with a dashboard, but a Go sidecar
  with shared-token auth still needs a control plane bolted on, and
  we already have an overlay with identity (Zenoh).
- **OpenZiti** — the "right" answer in isolation but redundant when
  Zenoh is already on the device. Two overlays = two secrets, two
  ACLs, two ports, two metrics.

**Streaming-query consideration.** A single bidirectional Zenoh
`get` would be tidier than the pub/sub side-channel pattern, but
Zenoh 1.x replies are single-`Sample` request/response — not a
long-lived bytestream. Pub/sub is the standard idiom and composes
with `zenoh-ext::AdvancedPublisher` later if we need replay.

**Overturn this if:**

- Zenoh leaves the device fleet (then re-evaluate the whole stack), or
- A new Zenoh release breaks the pub/sub-on-side-channels pattern in
  SCOPE §3.5 with no working replacement, or
- Zenoh ACLs prove insufficient for per-device-keyexpr authorisation
  at fleet scale (e.g. ACL evaluation cost is O(devices) and we have
  10k+ entries — measure during Phase 4 work).

---

## message-broker — 2026-05

**Chosen:** Zenoh pub/sub + queryables for the message plane (events,
logs, cmd, api). Durable command outbox lives in **the gateway's
SQLite**, not in any broker.

**Rejected:**

- **NATS / JetStream** — the canonical answer for fleet messaging
  with durable streams, and the eventual answer if we ever need
  fan-in across millions of producers. Rejected for v0.1 because:
  (a) we'd be running two overlay networks (Zenoh for tunnel, NATS
  for messages); (b) NATS adds an operational dependency (cluster,
  accounts, JetStream tuning) when we have ~1000s of devices and a
  small ops team; (c) the only piece we need durability for is the
  **command** outbox, and SQLite handles that for our scale with no
  new ops surface.
- **Kafka / Redpanda** — same objections as NATS at higher cost.
- **MQTT** — weaker req/reply story than Zenoh, and we'd still want
  Zenoh for the tunnel plane.
- **In-Zenoh durable streams via `zenoh-ext::AdvancedPublisher` +
  storage plugins** — the storage-plugin story isn't where we'd want
  it for a critical command path; SQLite is auditable in three lines
  of `sqlite3` shell.

**Overturn this if:**

- Cmd outbox throughput exceeds what SQLite WAL handles with a single
  writer (measure: writes/sec to `cmd_outbox`; threshold ~5k/s on the
  gateway hardware), **or**
- A use case lands that needs cross-device fan-in topics with
  durability the gateway can't reasonably mediate (e.g. raw timeseries
  ingestion at TSDB rates), at which point we add NATS *alongside*
  Zenoh for that path only — not as a replacement.

---

## client-session-model — 2026-05

**Chosen:** Each device app opens its own `zenoh::Session` via
`hackline-client`. The SDK is a library, not a daemon. Apps do **not**
proxy through `hackline-agent`.

**Rejected:**

- **Local IPC proxy through `hackline-agent`** — would force us to
  design a local protocol (UDS? gRPC? named pipes on Windows?), a
  fan-out multiplexer, an auth boundary inside the device, and
  reconnect logic at two layers. All of which Zenoh already provides.
  Worse, every restart of the byte-tunnel daemon would blackhole
  every app's message plane.
- **Single shared `zenoh::Session` per device, multiplexed via local
  channels** — same reinvention, slightly less code than the IPC
  variant, but still ties app message-plane lifetime to one process's
  lifetime.

**Cost accepted:** every device app needs Zenoh credentials. Not
treated as an app-vs-app security boundary — a hostile process on
the device can read the cert from disk regardless of process
separation. Apps share `/etc/hackline/device.pem` (mode 0640, group
`hackline`); each `Session` gets its own ZID under the same chain.

**Overturn this if:**

- A target deployment genuinely cannot run a Zenoh session (e.g. tiny
  MCU app in C that only speaks MQTT, or a legacy binary we can't
  relink). Then `hackline-agent` grows a small per-client adapter
  subcommand for *those specific clients only* — not the default
  architecture.

---

## persistence — 2026-05

**Chosen:** SQLite via `rusqlite` (bundled), `r2d2` pool, `refinery`
migrations, `tokio::task::spawn_blocking` from async handlers.

**Rejected:**

- **`sqlx`** — async sqlite is compile-heavy, query macros require a
  live DB at build time, our write volume is one writer doing
  control-plane ops + cmd outbox writes.
- **Postgres in v0.1** — premature; a few thousand devices fit in
  SQLite with WAL. Repository trait keeps the door open.
- **Sled / redb** — no SQL means hand-rolled query layer for the
  admin UI, audit, and cmd-outbox views; not worth it at our scale.

**Overturn this if:** active-active multi-region, device count >~50k,
or the prune workload starts dominating writer time.

---

## events — 2026-05

**Chosen:** Server-Sent Events (SSE) over the existing HTTP server
for both control-plane events and message-plane fan-out
(`/v1/devices/:id/msg/events/stream`).

**Rejected:**

- **gRPC streaming** — needs `tonic` + `protoc` + per-language
  codegen; the only thing we'd use bidirectional streaming for is the
  event feed, which is one-directional.
- **WebSocket** — bidi, but our event feed is server→client only.
  Reserved for a future "live remote shell" feature.

**Reverse-proxy gotcha.** SSE through Caddy / nginx requires response
buffering disabled (`flush_interval -1` for Caddy,
`proxy_buffering off` for nginx). The operator's reverse-proxy
config carries this requirement; SCOPE §5.4 / §9.8 spell it out.

**Overturn this if:** a feature lands that genuinely needs
client→server streaming over the same channel.

---

## audit-grain — 2026-05

**Chosen:** One row per **bridged TCP session** (open + close + byte
counts), plus one row per control-plane mutation (`cmd.send`,
`api.call`, `device.create`, …). NOT per-HTTP-request, NOT per-event,
NOT per-log-line. Retention default 180 days for `tunnel.session`,
indefinite for control-plane actions.

**Rejected:**

- **Per-HTTP-request audit** — at fleet scale (1000s of devices ×
  100s of UI requests/day) this hits hundreds of millions of rows per
  year.
- **Per-message-plane-event audit** — same problem at higher
  multiplier; events already live in the `events` ring with their
  own bounded retention.

**Overturn this if:** a regulator demands per-request audit (then we
buy a real log store; we don't extend SQLite further).

---

## tls-on-device — 2026-05

**Chosen:** Device serves plain HTTP on `127.0.0.1`. No Caddy, no
certs, no TLS code on the device. All public TLS is on the cloud
gateway (Caddy in front of axum).

**Rejected:**

- **Per-device Let's Encrypt** — DNS challenge per device, renewal
  failures, clock-drift on Pis. Operational nightmare.
- **Self-signed certs on device** — adds complexity and gets us
  nothing because the link is already encrypted by Zenoh.

**Overturn this if:** an application on the device refuses to run
over plain HTTP and we can't disable that requirement in its config.

---

## cmd-delivery-semantics — 2026-05

**Chosen:** **At-least-once** delivery for the cmd outbox.
`cmd_id` (server-assigned UUID) is the idempotency key; device app
must dedupe.

**Rejected:**

- **At-most-once with ack-removes-from-outbox-before-delivery** —
  silently drops commands across device restarts in the gap between
  delivery and ack. Strictly worse for our use case (install block,
  reboot, reload-config — all idempotent or trivially made so with
  `cmd_id`).
- **Exactly-once** — would require distributed coordination we
  explicitly don't want. The literature is clear that at-least-once +
  idempotent operations is how this is done in practice.

**Overturn this if:** a use case lands that is genuinely not
idempotent and cannot be made so via `cmd_id` (vanishingly unlikely
for IoT control commands).

---

## events-retention — 2026-05

**Chosen:** **Ring-buffer per device** for `events` and `logs` tables
(default 10000 rows each). Oldest row deleted in the same transaction
as the write that exceeds cap. Configurable cap; configurable swap to
time-based at operator's discretion.

**Rejected:**

- **Time-based retention only** — what people expect from a log
  store, but unbounded disk growth on a chatty device. We'd be a TSDB
  by accident.
- **Unbounded** — same problem, slower-motion.
- **Off-by-default** — events are core to the message plane; turning
  them off makes Studio's "what just happened" view useless.

**Overturn this if:** users consistently want time-windowed history
and our default ring size is too small in practice. The cap is
already configurable; the bigger lever is letting cap be expressed
as `since_ts` instead of `row_count`. Implement that before
reconsidering the whole design.

---

## payload-format — 2026-05

**Chosen:** **JSON** for all message-plane payloads in v0.1.
`Envelope.content_type` field reserved (default `application/json`)
so a future bincode addition is non-breaking at the namespace level.

**Rejected for v0.1:**

- **bincode-only** — smaller and faster, but opaque to debugging
  tools (`hackline events tail`, `sqlite3 events.db`,
  ad-hoc Zenoh subscribers). Premature optimisation for our
  expected payload sizes (slot updates, command bodies).
- **MessagePack / CBOR** — same trade as bincode with worse Rust
  ecosystem support.
- **Protobuf** — pulls in `prost` or `protoc`; no compelling reason
  to add the codegen step when JSON is good enough.

**Overturn this if:** profiling shows JSON encode/decode dominating
gateway CPU at expected production load (e.g. cmd_outbox writes
serialised >5k/s and serde_json is the top frame).

---

## core-crate-footprint — 2026-05

**Chosen:** `hackline-core` and `hackline-client` may depend on
`tokio` + `zenoh`. No artificial split into "pure" + "tokio impl"
sub-crates.

**Rejected:**

- **Splitting `hackline-core` into traits + tokio impl** — would add
  a crate boundary to keep build-time pure for a hypothetical thin
  consumer that doesn't exist. The agent already pulls tokio and
  zenoh, so the footprint cost is zero in practice. `hackline-proto`
  already covers pure types for any consumer that needs the schema
  without runtime.

**Overturn this if:** a real consumer appears (e.g. a no-tokio
embedded build of the wire schema) that needs the trait-only seam.

---

## auth-seam-pending — 2026-05

**Status:** **Open.** The choice between Options α / β / γ is not
yet made. Recorded here so it can't be made by accident.

**Context:** Auth has three independent layers between a customer
browser and a rubix graph node — L1 (device access), L2 (tunnel
access), L3 (in-device authz). L1+L2 live in hackline-gateway; L3
lives in rubix unchanged. The seam is how a Rauthy-issued user
identity reaches a rubix `role: edge` device that does not (per
RAUTHY-MIGRATION.md) run Rauthy. Full analysis in
[`INTEGRATION-RUBIX.md` §9](./INTEGRATION-RUBIX.md#9-auth-seam--device-access-tunnel-access-in-device-authz).

**Recommended (not yet ratified):** **Option α** — hackline-gateway
terminates auth, signs an `X-Rubix-User` header with a per-device
Ed25519 key, edge has a new `GatewayHeaderProvider` alongside its
existing `StaticTokenProvider`.

**Rejected (pending ratification):**

- **Option β** — push Rauthy verification onto the edge. Contradicts
  RAUTHY-MIGRATION's explicit edge-stays-Rauthy-free promise; couples
  edge auth to cloud JWKS availability; introduces stale-cache bugs
  on disconnected edges.
- **Option γ** — gateway is the only thing that ever calls the
  device. Reintroduces the `FleetRequestTransport` shape we just
  walked away from in INTEGRATION-RUBIX §3.2; loses user identity in
  the device's audit log.

**This decision must be ratified before:** Phase 2 of hackline (HTTP
host-routing) ships, or any rubix `role: edge` device is exposed to
a real customer through `https://device-N.cloud.com/`. Shipping a
customer-facing per-device URL against an undecided seam will
accidentally ratify whichever option happens to be wired up first.

**Overturn the recommendation if:** rubix's auth model changes to
allow Rauthy on the edge — then Option β is strictly simpler. If
RAUTHY-MIGRATION's edge-stays-Rauthy-free stance ever softens, this
ADR should be revisited the same day.

---

## standalone-vs-in-tree — 2026-05

**Chosen:** Hackline is a standalone project. Consumers (rubix today,
others later) depend only on `hackline-client` + `hackline-proto`.
Full rules in [`INTEGRATION-RUBIX.md`](./INTEGRATION-RUBIX.md).

**Rejected:**

- **Build hackline directly inside rubix-agent.** Faster on day 1,
  measurably worse by month 6. The exact failure mode is documented:
  rubix already tried this with its `transport-fleet-zenoh` +
  `Scope::Remote` + `FleetRequestTransport` stack, accreted six gaps
  (per `FLEET-TRANSPORT.md`), and ended up parking the whole thing in
  a not-yet-functional `com.rubix.fleet` extension in May 2026. We do
  not run that experiment a second time.
- **Build hackline as a rubix extension from the start.** Same
  problem one layer out — the extension still lives in the rubix
  release cadence and CI matrix.
- **Shared SPI trait abstraction (`trait HacklineTransport` in
  rubix's `spi` crate, with hackline as one impl).** Adds a seam that
  pays for nothing: tests already use a real loopback Zenoh router
  (proven by `fleet_zenoh_e2e.rs`), so the trait isn't earning
  testability, and the only "second impl" anyone has ever wanted was
  the `NullTransport` no-op that `Option<Arc<Session>>` handles
  natively. The trait was the seam that let the original fleet code
  accrete; we are not putting it back.
- **Parallel rubix-only RPC channel (e.g. a thin gRPC sidechannel
  between rubix-gateway and rubix-agent that bypasses hackline for
  "hot" paths).** Violates R2 (the wire is the contract) and locks
  out future TS/Dart SDKs from using the same surface. If a path
  needs to be faster than hackline can deliver, fix hackline; if
  hackline genuinely can't deliver, that's a hackline scope question,
  not a "let's bypass it" question.

**The four guardrails** (R1–R4 in `INTEGRATION-RUBIX.md`) are what
make standalone strictly better than in-tree. Without them this
decision flips to a coin toss; with them it's not close.

**Overturn this if:** rubix needs a feature that genuinely cannot
fit through Zenoh keyexprs and HTTP tunnels (the two transports
hackline exposes). At that point the right move is not to fork
hackline into rubix, but to figure out what kind of transport the
feature actually needs and decide whether hackline grows to cover it
or a separate sibling project takes it on. As long as rubix is one of
N consumers (real or plausibly future), standalone wins.

---

## data-plane-shape — 2026-05

**Chosen:** **`Open` query + per-request pub/sub side channels** for
the tunnel plane. (Documented in SCOPE §3.5 and §5.1.)

**Rejected:**

- **One streaming Zenoh `get` carrying bytes** — Zenoh 1.x replies
  are single-`Sample` request/response semantics, not long-lived bidi
  bytestreams. We'd be misusing the API.
- **Per-tunnel persistent `Session` between gateway and agent** —
  fights Zenoh's session model and gives up its multiplexing for
  nothing.

**Overturn this if:** a future Zenoh release adds true bidi-stream
queries with the same reliability and ordering properties as the
side-channel pattern. Then we may consolidate.

---

## tls-termination — 2026-05

**Chosen:** **axum-server 0.8 + tokio-rustls + instant-acme 0.8**
behind a `tls` Cargo feature flag. Three modes via `[tls]` config
block: self-signed (rcgen), manual PEM certs, ACME HTTP-01.
`RustlsConfig` shared between REST listener and tunnel TCP acceptor;
supports hot-reload for ACME cert renewal.

**Rejected:**

- **rustls-acme** — higher-level but couples TLS acceptor creation
  tightly to its own server loop; doesn't compose with axum-server's
  `bind_rustls` or with wrapping raw `TcpStream`s for tunnel sockets.
- **Always require Caddy/nginx in front** — adds a deployment
  dependency and breaks the "single binary" goal for Phase 5.
- **openssl** — extra C dependency, harder cross-compilation.

**Overturn this if:** rustls-acme gains an API that produces a
standalone `ServerConfig` we can share with tunnel listeners, or if
the tunnel TCP TLS wrapping proves too complex with the current
approach.
