//! `hackline` CLI entry point. Parses argv, dispatches to `cmd/`.

mod client;
mod cmd;
mod config;
mod error;
mod output;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "hackline", about = "hackline CLI")]
struct Cli {
    /// Gateway base URL (overrides cached value)
    #[arg(long, global = true, env = "HACKLINE_SERVER")]
    server: Option<String>,

    /// Bearer token (overrides cached value)
    #[arg(long, global = true, env = "HACKLINE_TOKEN")]
    token: Option<String>,

    /// Output as JSON instead of human-readable tables
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Claim a fresh gateway and cache credentials
    Login {
        /// Gateway URL
        #[arg(long)]
        server: String,
        /// Claim token printed by the gateway at first boot
        #[arg(long)]
        token: String,
        /// Owner name
        #[arg(long, default_value = "owner")]
        name: String,
        /// Org slug to seed at claim time. Omit to land in the
        /// `default` org (single-tenant deployments).
        #[arg(long)]
        org: Option<String>,
    },
    /// Print cached identity
    Whoami,
    /// Device management
    #[command(subcommand)]
    Device(DeviceCmd),
    /// Tunnel management
    #[command(subcommand)]
    Tunnel(TunnelCmd),
    /// User management
    #[command(subcommand)]
    User(UserCmd),
    /// Org (tenant) management — SCOPE.md §13 Phase 4.
    #[command(subcommand)]
    Org(OrgCmd),
    /// Message-plane events (live tail + history)
    #[command(subcommand)]
    Events(EventsCmd),
    /// Message-plane logs (live tail + history)
    #[command(subcommand)]
    Log(LogCmd),
    /// Durable command outbox
    #[command(subcommand)]
    Cmd(CmdCmd),
    /// Synchronous device-side RPC
    #[command(subcommand)]
    Api(ApiCmd),
}

#[derive(Subcommand)]
enum CmdCmd {
    /// Enqueue a command for a device
    Send {
        #[arg(long)]
        device: i64,
        #[arg(long)]
        topic: String,
        /// JSON-encoded payload string
        #[arg(long, default_value = "{}")]
        payload: String,
        /// TTL in milliseconds (default 7 days server-side)
        #[arg(long)]
        expires_in_ms: Option<i64>,
    },
    /// List outbox entries for a device
    List {
        #[arg(long)]
        device: i64,
        /// `pending` | `delivered` | `acked`
        #[arg(long)]
        status: Option<String>,
    },
    /// Cancel a queued command
    Cancel {
        #[arg(long)]
        cmd_id: String,
    },
}

#[derive(Subcommand)]
enum ApiCmd {
    /// Synchronous RPC against a device's `serve_api` handler
    Call {
        #[arg(long)]
        device: i64,
        #[arg(long)]
        topic: String,
        #[arg(long, default_value = "{}")]
        payload: String,
        #[arg(long, default_value_t = 5000)]
        timeout_ms: u64,
    },
}

#[derive(Subcommand)]
enum EventsCmd {
    /// Follow the SSE stream and print one JSON line per delivery.
    Tail {
        #[arg(long)]
        device: Option<i64>,
        #[arg(long)]
        topic: Option<String>,
    },
    /// Page the cursor API for historical entries.
    History {
        #[arg(long)]
        device: Option<i64>,
        #[arg(long)]
        topic: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: i64,
    },
}

#[derive(Subcommand)]
enum LogCmd {
    /// Follow the SSE log stream.
    Tail {
        #[arg(long)]
        device: Option<i64>,
        #[arg(long)]
        topic: Option<String>,
    },
    /// Page the cursor API for historical log entries.
    History {
        #[arg(long)]
        device: Option<i64>,
        #[arg(long)]
        topic: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: i64,
    },
}

#[derive(Subcommand)]
enum DeviceCmd {
    /// Register a device
    Add {
        #[arg(long)]
        zid: String,
        #[arg(long)]
        label: String,
    },
    /// List all devices
    List,
    /// Show a single device
    Show {
        #[arg(long)]
        id: i64,
    },
    /// Remove a device
    Remove {
        #[arg(long)]
        id: i64,
    },
}

#[derive(Subcommand)]
enum TunnelCmd {
    /// Create a tunnel
    Add {
        #[arg(long)]
        device_id: i64,
        #[arg(long, default_value = "tcp")]
        kind: String,
        #[arg(long)]
        local_port: i64,
        #[arg(long)]
        public_port: Option<i64>,
        #[arg(long)]
        public_hostname: Option<String>,
    },
    /// List all tunnels
    List,
    /// Remove a tunnel
    Remove {
        #[arg(long)]
        id: i64,
    },
}

#[derive(Subcommand)]
enum OrgCmd {
    /// Create a new org (owner-only).
    Create {
        #[arg(long)]
        slug: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// List every org on the gateway (owner-only).
    List,
    /// Show the caller's own org.
    Inspect,
}

#[derive(Subcommand)]
enum UserCmd {
    /// Create a user
    Add {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "operator")]
        role: String,
    },
    /// List all users
    List,
    /// Remove a user
    Remove {
        #[arg(long)]
        id: i64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let json = cli.json;

    match cli.command {
        Command::Login {
            server,
            token,
            name,
            org,
        } => {
            cmd::login::run(&server, &token, &name, org.as_deref()).await?;
        }
        Command::Whoami => {
            cmd::whoami::run(json)?;
        }
        Command::Device(sub) => {
            let c = client::Client::from_args_or_cache(cli.server, cli.token)?;
            match sub {
                DeviceCmd::Add { zid, label } => {
                    cmd::device::add::run(&c, &zid, &label, json).await?
                }
                DeviceCmd::List => cmd::device::list::run(&c, json).await?,
                DeviceCmd::Show { id } => cmd::device::show::run(&c, id, json).await?,
                DeviceCmd::Remove { id } => cmd::device::remove::run(&c, id).await?,
            }
        }
        Command::Tunnel(sub) => {
            let c = client::Client::from_args_or_cache(cli.server, cli.token)?;
            match sub {
                TunnelCmd::Add {
                    device_id,
                    kind,
                    local_port,
                    public_port,
                    public_hostname,
                } => {
                    cmd::tunnel::add::run(
                        &c,
                        device_id,
                        &kind,
                        local_port,
                        public_port,
                        public_hostname.as_deref(),
                        json,
                    )
                    .await?
                }
                TunnelCmd::List => cmd::tunnel::list::run(&c, json).await?,
                TunnelCmd::Remove { id } => cmd::tunnel::remove::run(&c, id).await?,
            }
        }
        Command::Org(sub) => {
            let c = client::Client::from_args_or_cache(cli.server, cli.token)?;
            match sub {
                OrgCmd::Create { slug, name } => {
                    cmd::org::create(&c, &slug, name.as_deref(), json).await?
                }
                OrgCmd::List => cmd::org::list(&c, json).await?,
                OrgCmd::Inspect => cmd::org::inspect(&c, json).await?,
            }
        }
        Command::User(sub) => {
            let c = client::Client::from_args_or_cache(cli.server, cli.token)?;
            match sub {
                UserCmd::Add { name, role } => cmd::user::add::run(&c, &name, &role, json).await?,
                UserCmd::List => cmd::user::list::run(&c, json).await?,
                UserCmd::Remove { id } => cmd::user::remove::run(&c, id).await?,
            }
        }
        Command::Events(sub) => {
            let c = client::Client::from_args_or_cache(cli.server, cli.token)?;
            match sub {
                EventsCmd::Tail { device, topic } => {
                    cmd::events::tail(
                        &c,
                        device,
                        topic.as_deref(),
                        cmd::events::StreamKind::Events,
                    )
                    .await?
                }
                EventsCmd::History {
                    device,
                    topic,
                    limit,
                } => {
                    cmd::events::history(
                        &c,
                        device,
                        topic.as_deref(),
                        limit,
                        json,
                        cmd::events::StreamKind::Events,
                    )
                    .await?
                }
            }
        }
        Command::Log(sub) => {
            let c = client::Client::from_args_or_cache(cli.server, cli.token)?;
            match sub {
                LogCmd::Tail { device, topic } => {
                    cmd::events::tail(&c, device, topic.as_deref(), cmd::events::StreamKind::Logs)
                        .await?
                }
                LogCmd::History {
                    device,
                    topic,
                    limit,
                } => {
                    cmd::events::history(
                        &c,
                        device,
                        topic.as_deref(),
                        limit,
                        json,
                        cmd::events::StreamKind::Logs,
                    )
                    .await?
                }
            }
        }
        Command::Cmd(sub) => {
            let c = client::Client::from_args_or_cache(cli.server, cli.token)?;
            match sub {
                CmdCmd::Send {
                    device,
                    topic,
                    payload,
                    expires_in_ms,
                } => {
                    let p: serde_json::Value = serde_json::from_str(&payload)
                        .map_err(|e| anyhow::anyhow!("invalid --payload JSON: {e}"))?;
                    cmd::cmd_outbox::send(&c, device, &topic, p, expires_in_ms, json).await?
                }
                CmdCmd::List { device, status } => {
                    cmd::cmd_outbox::list(&c, device, status.as_deref(), json).await?
                }
                CmdCmd::Cancel { cmd_id } => cmd::cmd_outbox::cancel(&c, &cmd_id).await?,
            }
        }
        Command::Api(sub) => {
            let c = client::Client::from_args_or_cache(cli.server, cli.token)?;
            match sub {
                ApiCmd::Call {
                    device,
                    topic,
                    payload,
                    timeout_ms,
                } => {
                    let p: serde_json::Value = serde_json::from_str(&payload)
                        .map_err(|e| anyhow::anyhow!("invalid --payload JSON: {e}"))?;
                    cmd::cmd_outbox::api_call(&c, device, &topic, p, timeout_ms, json).await?
                }
            }
        }
    }

    Ok(())
}
