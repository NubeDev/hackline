# Config

Both binaries read TOML from `--config FILE` (default paths below).
Env vars override file values; flags override env vars.

## Gateway

Default path: `/etc/hackline/gateway.toml`, then
`$XDG_CONFIG_HOME/hackline/gateway.toml`.

```toml
# Bind address for the axum server. Caddy proxies to this.
listen = "127.0.0.1:8080"

# SQLite path. WAL mode is enabled at startup.
database = "/var/lib/hackline/gateway.db"

# Zenoh client config. See https://zenoh.io/docs/manual/configuration/
[zenoh]
mode = "client"
connect = ["tcp/router.example.com:7447"]

# Public-listener bind address for forwarded TCP/HTTP tunnels.
# Caddy terminates TLS in front of the HTTP listener.
[listeners]
http = "127.0.0.1:8081"
tcp_range = "0.0.0.0:20000-29999"

[log]
level = "info"  # error | warn | info | debug | trace
format = "json"  # json | pretty
```

Rendered + validated by [`hackline-gateway::config`](../crates/hackline-gateway/src/config.rs).

## Agent

Default path: `/etc/hackline/agent.toml`, then `./agent.toml`.

```toml
# Local ports the agent is allowed to bridge.
# Anything not listed here is rejected at the queryable layer.
allowed_ports = [22, 8080]

# Optional human label surfaced in AgentInfo.
label = "edge-1234"

[zenoh]
mode = "peer"
listen = ["tcp/0.0.0.0:7447"]

[log]
level = "info"
format = "json"
```

Rendered + validated by [`hackline-agent::config`](../crates/hackline-agent/src/config.rs).

## Precedence

```
flag  >  env  >  config file  >  built-in default
```

If a required value is missing at all four levels, both binaries
exit with a non-zero status and a one-line error pointing at the
missing key.
