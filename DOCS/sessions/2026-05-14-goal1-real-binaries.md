# 2026-05-14 — Goal 1: Real agent + gateway binaries

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Add `toml` workspace dep | [x] |
| 1 | hackline-agent: config.rs (TOML loader + validation) | [x] |
| 2 | hackline-agent: error.rs | [x] |
| 3 | hackline-agent: connect.rs (queryable loop with port whitelist) | [x] |
| 4 | hackline-agent: main.rs (real binary) | [x] |
| 5 | hackline-gateway: config.rs + tunnel config | [x] |
| 6 | hackline-gateway: tunnel/tcp_listener.rs | [x] |
| 7 | hackline-gateway: tunnel/bridge.rs | [x] |
| 8 | hackline-gateway: bin/serve.rs (real binary) | [x] |
| 9 | cargo check + test | [x] |
| 10 | End-to-end test: two separate processes | [x] |

## Design

Goal 1 is minimal: get real binaries that read TOML config and bridge
TCP through Zenoh. No DB, no REST, no auth. The gateway reads a
hard-coded tunnel list from its config file.

Agent config (from DOCS/CONFIG.md):
- `allowed_ports`: whitelist of local ports
- `label`: human-friendly name
- `zenoh.mode`, `zenoh.listen`: Zenoh session config

Gateway config (minimal for Goal 1):
- `zenoh.mode`, `zenoh.connect`: Zenoh session config
- `[[tunnels]]` array: each entry has `zid`, `device_port`, `listen_port`

No `listen`, `database`, `listeners` fields yet — those are Goal 3+.

## Outcome

Goal 1 complete. Three separate processes (echo server on 9998,
`hackline-agent`, `hackline-gateway serve`) bridged TCP through Zenoh:

```
nc → gateway :9999 → Zenoh → agent → echo :9998 → Zenoh → gateway → nc
```

Output: `GOT: 'goal 1 two processes'`

Design notes:
- Agent takes an explicit `zid` in config. The gateway tunnel entries
  reference this same ZID. In production, ZID assignment comes from the
  claim/registration flow (Goal 5).
- Agent spawns one queryable task per allowed port, each running an
  independent accept loop.
- Gateway spawns one `tcp_listener` task per `[[tunnels]]` entry.
- No DB, no REST, no auth yet — config file is the source of truth.
