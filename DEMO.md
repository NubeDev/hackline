# Hackline demo runbook

**Audience:** AI agents (and humans) bringing up the full stack so a
user can poke at the admin UI and tunnel HTTP through an agent. Every
section ends with **what to report back to the user** so they have
the URLs and tokens they need.

This doc is opinionated and uses fixed ports + the dev `Makefile`.
For deeper configuration see [DOCS/CONFIG.md](DOCS/CONFIG.md) and
[DOCS/DEVELOPMENT.md](DOCS/DEVELOPMENT.md).

## What you are starting

| Component | Purpose | Port |
|---|---|---|
| `hackline-gateway` | REST API + SSE + tunnel listener + SQLite | `127.0.0.1:8080` (Zenoh on `7448`) |
| `hackline-ui` (vite dev) | React admin UI, proxies to gateway | `127.0.0.1:1430` |
| `hackline-agent` | Device-side bridge, exposes local ports via Zenoh | diag at `127.0.0.1:9999` |
| Python `http.server` | A tunnelable target on the agent | `127.0.0.1:9998` |

State lives in `.hackline-dev/` (gitignored): SQLite DB, PID files,
logs, dev gateway config. `make clean` wipes it.

## 0. Preflight

```bash
cd /path/to/hackline
which cargo pnpm python3            # all three must resolve
ls .hackline-dev/ 2>/dev/null       # may not exist on first run
```

If `.hackline-dev/gateway.db` already exists, the gateway is already
claimed and the owner token is in there. To force a fresh demo:

```bash
make clean
```

## 1. Build (cold start only)

```bash
cargo build -p hackline-gateway --bin serve
cargo build -p hackline-agent
```

Skip if `target/debug/hackline-gateway` and `target/debug/hackline-agent`
already exist and the code is unchanged.

## 2. Gateway + UI

```bash
make start
```

`make start` writes `.hackline-dev/gateway.toml` on first run, then
launches the gateway and the vite dev server in the background.

Sanity:

```bash
make status
# expected:
# gateway: running (pid …, 127.0.0.1:8080)
# ui:      running (pid …, 127.0.0.1:1430)

curl -sf http://127.0.0.1:8080/v1/health   # → {"status":"ok"}
curl -sf -o /dev/null -w "%{http_code}\n" http://127.0.0.1:1430/   # → 200
```

If `make start` fails with `Address already in use`, run:

```bash
make kill         # frees 8080 + 1430 by port
fuser -k 7448/tcp # Zenoh listener; not in Makefile yet
rm -f .hackline-dev/gateway.pid .hackline-dev/ui.pid
make start
```

## 3. Claim the gateway (one-time per DB)

On the very first boot of a fresh DB the gateway prints a one-shot
claim token. Trade it for an owner bearer token via `POST /v1/claim`.

```bash
make claim
# CLAIM TOKEN: <claim_xxxxxxxxxxxx>
```

If `make claim` prints "no claim token in log — already claimed",
**skip to step 4** — there is an owner token already in the DB
(recover it from `.hackline-dev/owner-token` if you saved it earlier,
otherwise `make clean && make start` to redo the flow).

Exchange the claim token for the real bearer token:

```bash
CLAIM=$(make -s claim | awk '/CLAIM TOKEN/{print $3}')
RESP=$(curl -sf -X POST http://127.0.0.1:8080/v1/claim \
  -H 'Content-Type: application/json' \
  -d "{\"token\":\"$CLAIM\",\"name\":\"admin\"}")
TOKEN=$(echo "$RESP" | python3 -c 'import json,sys; print(json.load(sys.stdin)["token"])')
echo "$TOKEN" > .hackline-dev/owner-token
echo "owner token: $TOKEN"
```

Verify:

```bash
curl -sf http://127.0.0.1:8080/v1/devices \
  -H "Authorization: Bearer $TOKEN"
# → [] on a fresh DB
```

## 4. Agent + tunnel target

Write a dev agent config (only on first run) and start the Python
target plus the agent itself. The agent registers with the gateway
via Zenoh liveliness — no token required, because the trust boundary
is the Zenoh fabric (SCOPE.md §3).

```bash
cat > .hackline-dev/agent.toml <<'EOF'
zid = "de0100000001"
allowed_ports = [9998]
label = "dev-agent"
org = "default"

[zenoh]
mode = "peer"
connect = ["tcp/127.0.0.1:7448"]

[diag]
enabled = true
bind = "127.0.0.1:9999"

[log]
level = "info,hackline_core=debug"
format = "pretty"
EOF

# Python file server on 9998 — the thing tunnels will hit.
setsid bash -c 'python3 -c "
import http.server, socketserver
socketserver.TCPServer((\"127.0.0.1\", 9998),
    http.server.SimpleHTTPRequestHandler).serve_forever()
" > .hackline-dev/http9998.log 2>&1 & echo $! > .hackline-dev/http9998.pid' \
    < /dev/null

# Agent.
setsid bash -c './target/debug/hackline-agent .hackline-dev/agent.toml \
    > .hackline-dev/agent.log 2>&1 & echo $! > .hackline-dev/agent.pid' \
    < /dev/null

sleep 2
```

Sanity:

```bash
curl -sf http://127.0.0.1:9998/ -o /dev/null -w "target  %{http_code}\n"
curl -sf http://127.0.0.1:9999/api/v1/info | python3 -m json.tool | head -20
curl -sf http://127.0.0.1:8080/v1/devices -H "Authorization: Bearer $TOKEN" \
    | python3 -m json.tool
```

The `/api/v1/info` response should show `"gateway":{"connected":true,…}`
and `/v1/devices` should now include the agent's row with
`"zid":"de0100000001"`.

## 5. Create a tunnel (end-to-end check)

```bash
DEVICE_ID=$(curl -sf http://127.0.0.1:8080/v1/devices \
  -H "Authorization: Bearer $TOKEN" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["id"])')

curl -sf -X POST http://127.0.0.1:8080/v1/tunnels \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"device_id\":$DEVICE_ID,\"port\":9998}" \
  | python3 -m json.tool
```

The response includes a `listen` port on the gateway. Hit it:

```bash
TUNNEL_PORT=$(curl -sf http://127.0.0.1:8080/v1/tunnels \
  -H "Authorization: Bearer $TOKEN" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)[-1]["listen_port"])')
curl -sf "http://127.0.0.1:$TUNNEL_PORT/" -o /dev/null -w "tunnel %{http_code}\n"
```

`tunnel 200` means bytes flowed gateway → Zenoh → agent → loopback
:9998 → back.

## 6. Report to the user

Print this block back to the user verbatim (substitute the real
token). The token is the most important field — without it nothing
in the admin UI works.

```text
Running Services
  Gateway REST API   http://127.0.0.1:8080    (all routes at /v1/...)
  Admin UI           http://127.0.0.1:1430    (proxies to gateway)
  Agent Diag UI      http://127.0.0.1:9999    (loopback-only)
  Test HTTP server   127.0.0.1:9998           (tunnelable target)

Auth
  API Token   <TOKEN>
  Username    admin
  Org         default

Use the token as `Authorization: Bearer <TOKEN>` for API calls.
Paste the same token into the admin UI's Settings → Token field on first load.

Agent
  ZID            de0100000001
  Device ID      <DEVICE_ID>
  Exposed port   9998 (Python HTTP file server)

Quick test:
  curl -s http://127.0.0.1:8080/v1/tunnels \
    -H "Authorization: Bearer <TOKEN>" \
    -H "Content-Type: application/json" \
    -d '{"device_id":<DEVICE_ID>,"port":9998}'

To stop:
  make stop                                                # gateway + UI
  kill $(cat .hackline-dev/agent.pid)                      # agent
  kill $(cat .hackline-dev/http9998.pid)                   # python target
```

The token also lives at `.hackline-dev/owner-token` for later
sessions; the agent and Python PIDs at `.hackline-dev/agent.pid`
and `.hackline-dev/http9998.pid`.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `SSE stream error` in Live events page | Token not pasted into UI Settings | Open Settings, paste `$TOKEN`, reload |
| `/v1/devices` returns `[]` | Agent not connected to gateway | `curl 127.0.0.1:9999/api/v1/info` → check `gateway.connected: true`; if false, gateway's Zenoh listener (`7448`) is down or the agent's `connect` URL is wrong |
| `make claim` says "already claimed" | DB exists from a previous run | If you saved the token, reuse it (`cat .hackline-dev/owner-token`); else `make clean && make start` |
| Tunnel `curl` hangs | Python target not running, or agent dropped | `pgrep -af 'http.server'` and `pgrep -af hackline-agent`; restart step 4 |
| `Address already in use` on restart | Stale PID files | `make kill` then `make start`; for Zenoh `fuser -k 7448/tcp` |

## What is intentionally not in this runbook

- TLS / ACME (`tls` feature). Demo is HTTP only.
- Multi-tenant orgs beyond the seeded `default`.
- The `hackline-cli` claim flow — the raw `curl` exchange above is
  shorter and shows the wire shape the AI agent may need to script.
- Postgres backend. Demo is SQLite only.
