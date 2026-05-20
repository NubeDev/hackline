# Goal: Agent Diag UI v2

**Date:** 2026-05-20  
**Status:** Done

## Problem

The current agent diag UI (`127.0.0.1:9999`) is a bare-bones status
page. It shows identity and recent connections but doesn't tell the
operator:

1. Whether the agent is **actually connected** to a gateway (and which one).
2. What the agent *does* — the use case, the mental model.
3. How to set it up on different platforms.
4. What ports are exposed and how to **change them** without editing
   TOML and restarting.

## Use Cases

Two deployment modes, same binary:

| Mode | Example | Description |
|---|---|---|
| **Edge device** | Raspberry Pi, Rubix gateway, industrial controller | Agent runs as a systemd service; exposes local services (HTTP APIs, Modbus-TCP, SSH) to the cloud via Zenoh tunnels. Operator configures once and forgets. |
| **PC proxy** | Developer laptop, home server | User runs the agent to expose local dev servers (e.g. `localhost:3000`, `localhost:8080`) through the hackline gateway for remote access — like a self-hosted ngrok. Ports change frequently. |

## Plan

| # | Step | Status |
|---|---|---|
| 1 | Add gateway connection status to `/api/v1/info` response | done |
| 2 | New "Overview" landing page with connection state hero card | done |
| 3 | Show gateway URL, org, connectivity (connected / disconnected) | done |
| 4 | "How it works" panel — brief explanation of the tunnel model | done |
| 5 | "Setup" page — config file reference, systemd example, PC quick-start | done |
| 6 | Port management UI — list exposed ports, add/remove dynamically | done |
| 7 | Hot-reload port config (add/remove queryables at runtime without restart) | done |
| 8 | Persist port changes back to `agent.toml` (optional, behind confirm) | skipped (see below) |
| 9 | Connection quality indicator (last handshake time, round-trip if available) | partial (live peer count + 5s badge poller) |
| 10 | Polish: responsive layout, better empty states, toast notifications | done |

## Outcome

Verified end-to-end against the locally-built `hackline-agent` binary
(`cargo build -p hackline-agent`) running with `[diag] enabled = true`
on `127.0.0.1:19999`:

```text
$ curl -s http://127.0.0.1:19999/api/v1/info
{"zid":"5e0c0001","label":"smoke-agent","org":"default",
 "ports":[{"port":9988,"from_config":true}],"version":"0.0.0","uptime_s":1,
 "gateway":{"configured":[],"peer_count":0,"connected":false}}

$ curl -s -X POST -H 'content-type: application/json' \
    -d '{"port":4242}' http://127.0.0.1:19999/api/v1/ports
{"port":4242,"from_config":false}

$ curl -s -X POST -H 'content-type: application/json' \
    -d '{"port":4242}' http://127.0.0.1:19999/api/v1/ports
{"error":"config: port 4242 is already being served"}

$ curl -s -X DELETE http://127.0.0.1:19999/api/v1/ports/4242 -o /dev/null \
    -w '%{http_code}\n'
204
```

The agent logs `queryable ready ke=hackline/.../tcp/4242/connect
runtime=true` on add and aborts the task on remove, so Zenoh sees a
clean undeclare. Workspace checks:

```text
cargo clippy --workspace --all-targets -- -D warnings    # clean
cargo test --workspace                                   # all green
```

## Design

### Where state moved

`DiagState` previously held read-only metadata plus a bounded
connection-event ring. It now also owns:

- `Arc<Session>` — so handlers can call
  `session.info().peers_zid().await` and declare new queryables.
- `Zid` (typed) — passed into `keyexpr::connect(...)` when adding a
  runtime port.
- `RwLock<HashMap<u16, ActivePort>>` — the active port set. Each
  `ActivePort` wraps the `JoinHandle` for the per-port queryable loop
  plus a `from_config` flag so the UI distinguishes startup ports
  from runtime-added ones (the latter are lost on restart, which the
  UI calls out).

This means `cfg.allowed_ports` is no longer the source of truth at
runtime — the map is. `start_initial_ports` seeds it from config at
boot via the same `spawn_port_queryable` function that the diag
`POST /api/v1/ports` handler calls. One code path for both entry
points keeps adds/removes consistent.

### Why `JoinHandle::abort` for removal

Zenoh's queryable is held inside the per-port receive loop. Aborting
the `JoinHandle` drops the future, which drops the queryable, which
undeclares the keyexpr. Cleaner than threading an explicit shutdown
channel through every loop, and there is nothing in the loop body
that needs graceful cleanup.

### Gateway connectivity definition

"Connected" is `session.info().peers_zid().await.count() > 0`. In a
device deployment the only peer is the gateway, so this is a true
gateway-reachability signal. In peer-mode dev setups (multiple agents
on a LAN) it just means "some Zenoh neighbour is up", which is still
useful — and the UI separately shows the configured connect
endpoints, so the operator can correlate.

`peers_zid()` is part of the stable Zenoh surface (not behind
`unstable`), so this works without enabling additional zenoh
features. `transports()`/`links()` would have given richer per-link
latency info but they're `#[zenoh_macros::unstable]`, which we keep
off to mirror the rest of the workspace.

### Why no TOML write-back (step 8 skipped)

Two reasons. First, `serde::Deserialize` + `toml::from_str` round-trip
would discard comments and reorder keys, which is hostile to
hand-edited config files. Second, the runtime-port chip in the UI is
visibly orange ("runtime") vs. blue ("from config"), so the operator
already knows the add is ephemeral and can copy the port into the
TOML themselves. Persisting it silently is the worst of both worlds.

### Frontend layout

Same vanilla-JS + Bootstrap pattern as before, no build step. Five
tabs replace the previous three: **overview** (hero card + ports
chips + "how it works"), **ports** (add/remove form + table),
**connections** (existing bridge-event ring), **zenoh** (now also
lists connected peer ZIDs), **setup** (tabbed systemd vs. PC quick
start with copy-pasteable recipes). The connectivity badge in the
navbar polls `/api/v1/info` every 5s so it stays live across page
transitions.

### Static asset caching

Initial cut served `/static/*` with `Cache-Control: public, max-age=300`,
which made sense for ~120 KB of Bootstrap CSS but bit us immediately:
after rebuilding and restarting the agent, the browser kept rendering
the old `app.js` (visible as the legacy "IDENTITY" / "ALLOWED PORTS"
cards on `#/ports`) until the 5-minute TTL elapsed. The assets are
baked into the binary via `include_str!` and only change with a
rebuild, so the long TTL has no upside — switched all baked assets
to `Cache-Control: no-cache` so a binary upgrade is picked up on the
next reload without operators needing a hard-refresh.

## Shutdown change worth flagging

Main previously blocked on `connect::serve_connect(...)` for the
lifetime of the agent. That function went away (its work is now
spread across the per-port tasks owned by `DiagState`), so main now
blocks on `tokio::signal::ctrl_c()`. The liveliness token, info
queryable, and port queryables all stay declared until that signal
fires.

## Design Notes

### Connection status

The agent already holds an `Arc<zenoh::Session>`. Zenoh sessions
expose connectivity info (peer list, transport state). We surface:

- **Gateway reachable:** yes/no (based on Zenoh transport to the
  gateway's listen endpoint).
- **Gateway URL:** from `zenoh.connect` in config.
- **Org:** from config.
- **Session ZID:** already shown; now front-and-centre.
- **Uptime + last activity:** already available.

### Port management (runtime)

Currently `allowed_ports` is loaded once at startup and each port gets
a Zenoh queryable declared. To support dynamic add/remove:

1. Store the active queryables in a `HashMap<u16, QueryableGuard>` behind
   an `RwLock` in `DiagState` (or a new `AgentState`).
2. New diag endpoints: `POST /api/v1/ports` (add), `DELETE /api/v1/ports/{port}` (remove).
3. Adding a port: validate → declare queryable → insert into map → optionally
   append to TOML.
4. Removing: undeclare queryable (drop the guard) → remove from map.
5. UI: simple form with port number input + "Expose" button; each
   active port has a "Remove" button.

Loopback-only bind means no auth is needed for these mutations (same
trust model as SSH access to the box).

### Landing page content

The overview page should answer in 10 seconds:

- **Am I connected?** → Green/red badge with gateway URL.
- **What am I exposing?** → List of ports with live/unreachable status.
- **What is this?** → One-paragraph explanation:
  > "This agent bridges local TCP services to the hackline gateway
  > over Zenoh. Remote users can reach your services via tunnels
  > created in the gateway UI — no port forwarding or VPN required."

### Setup page content

Two tabs:

**Edge device (Raspberry Pi / Linux service):**
```
1. Copy hackline-agent binary to the device
2. Create /etc/hackline/agent.toml with your ZID, org, and gateway connect URL
3. Set allowed_ports to the services you want to expose
4. Enable the systemd unit: systemctl enable --now hackline-agent
```

**PC proxy (quick start):**
```
1. Download hackline-agent for your OS
2. Run: hackline-agent agent.toml
3. Open http://127.0.0.1:9999 and add ports to expose
4. Create tunnels in the gateway UI to access them remotely
```

## Out of Scope

- Auth on the diag UI (loopback-only is the security boundary).
- Writing a new React/Vite UI (keep the vanilla JS + Bootstrap approach;
  no build step, ships as `include_str!`).
- Gateway-side changes (the gateway already handles tunnels; this is
  agent-local UX only).
