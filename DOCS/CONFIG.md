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

[zenoh.access_control]
enabled = true
default_permission = "deny"

[[zenoh.access_control.rules]]
id = "allow-device-prefix"
permission = "allow"
messages = ["put", "delete", "query", "reply", "declare_subscriber", "declare_queryable"]
key_exprs = ["hackline/default/device-01/**"]

[[zenoh.access_control.subjects]]
id = "device-01"
cert_common_names = ["device-01"]

[[zenoh.access_control.policies]]
rules = ["allow-device-prefix"]
subjects = ["device-01"]

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

[zenoh.access_control]
enabled = true
default_permission = "deny"

[[zenoh.access_control.rules]]
id = "allow-gateway-own-prefix"
permission = "allow"
messages = ["query", "reply", "put", "delete", "declare_subscriber", "declare_queryable", "liveliness_token"]
key_exprs = ["hackline/default/device-01/**"]

[[zenoh.access_control.subjects]]
id = "gateway"
cert_common_names = ["hackline.zenoh.nube-iiot.com"]

[[zenoh.access_control.policies]]
rules = ["allow-gateway-own-prefix"]
subjects = ["gateway"]

[log]
level = "info"
format = "json"
```

Rendered + validated by [`hackline-agent::config`](../crates/hackline-agent/src/config.rs).

### ACL fields

`[zenoh.access_control]` mirrors Zenoh's top-level `access_control`
object.

- `enabled`: enable or disable ACL enforcement.
- `default_permission`: `"allow"` or `"deny"` for unmatched traffic.
- `rules`: message filters (`messages`, optional `flows`, `key_exprs`) with
	`permission`.
- `subjects`: peer identities (`cert_common_names`, `usernames`,
	`interfaces`, `link_protocols`, `zids`).
- `policies`: attaches rule IDs to subject IDs.

For production, prefer subject matching by certificate common name
(`cert_common_names`) with mTLS. ZID matching is useful for local
prototyping but is not a strong identity by itself.

## Precedence

```
flag  >  env  >  config file  >  built-in default
```

If a required value is missing at all four levels, both binaries
exit with a non-zero status and a one-line error pointing at the
missing key.
