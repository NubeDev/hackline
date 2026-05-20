# CLI

`hackline` is a thin REST/SSE client over the gateway. Each leaf
subcommand has its own file under
[`crates/hackline-cli/src/cmd/`](../crates/hackline-cli/src/cmd/).

## Top-level

| Command | File |
|---|---|
| `hackline login` | `cmd/login.rs` |
| `hackline whoami` | `cmd/whoami.rs` |
| `hackline events` | `cmd/events.rs` |

## Devices

| Command | File |
|---|---|
| `hackline device list` | `cmd/device/list.rs` |
| `hackline device add` | `cmd/device/add.rs` |
| `hackline device show ID` | `cmd/device/show.rs` |
| `hackline device remove ID` | `cmd/device/remove.rs` |

## Tunnels

| Command | File |
|---|---|
| `hackline tunnel list` | `cmd/tunnel/list.rs` |
| `hackline tunnel add` | `cmd/tunnel/add.rs` |
| `hackline tunnel remove ID` | `cmd/tunnel/remove.rs` |

## Users / tokens

| Command | File |
|---|---|
| `hackline user list` | `cmd/user/list.rs` |
| `hackline user add` | `cmd/user/add.rs` |
| `hackline user remove ID` | `cmd/user/remove.rs` |
| `hackline token mint --user ID` | `cmd/token/mint.rs` |

## Gateway operator commands

`hackline-gateway` is a separate binary built from
`crates/hackline-gateway/src/bin/`.

| Command | File |
|---|---|
| `hackline-gateway serve` | `bin/serve.rs` |
| `hackline-gateway reset-claim` | `bin/reset_claim.rs` |
| `hackline-gateway print-claim` | `bin/print_claim.rs` |

`print-claim` only succeeds while the claim is pending; it lets an
operator who missed the boot log recover without a full reset.

## Env vars

| Var | Equivalent flag |
|---|---|
| `HACKLINE_SERVER` | `--server` |
| `HACKLINE_TOKEN` | `--token` |
| `HACKLINE_CONFIG` | `--config` |

Credentials cache: `$XDG_CONFIG_HOME/hackline/credentials.json`,
mode `0600`.
