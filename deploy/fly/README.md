# Deploying hackline-gateway to Fly.io

This directory contains a minimal Docker + Fly.io setup for the
`hackline-gateway` binary. Agents and CLIs are not deployed here —
they run on user hardware and connect inwards.

## Files

- [Dockerfile](../Dockerfile) — multi-stage build. `rust:1.82-bookworm`
  compiles `hackline-gateway` (release, default features, no `tls`).
  Runtime image is `debian:bookworm-slim` with `ca-certificates` +
  `libssl3`. Cargo registry and `target/` are cache-mounted so
  rebuilds reuse compiled deps.
- [.dockerignore](../.dockerignore) — keeps `target/`, `node_modules/`,
  dev DBs, UI sources, and docs out of the build context.
- [fly.toml](../fly.toml) — single-machine Fly app config.
- [deploy/fly/gateway.toml](./gateway.toml) — gateway config baked
  into the image. Binds `0.0.0.0:8080`, DB at `/data/gateway.db`.

## Connection model (current image)

The image runs the gateway with TLS terminated at Fly's edge:

- **REST + SSE** on `https://<app>.fly.dev` → Fly proxy → container
  `:8080`. This is the path agents and clients use.
- **Zenoh raw TCP** on `:7448` is exposed via `[[services]]` for
  agents on networks that allow arbitrary outbound TCP. Networks that
  only permit `:80`/`:443` outbound will not reach this port — see
  "Reachability" below.
- **Per-tunnel TCP listeners** declared in `gateway.toml` are *not*
  routed by Fly unless you also add matching `[[services]]` blocks
  to `fly.toml`.

## First-time setup

```bash
cd hackline
fly apps create <your-app-name>
# Edit fly.toml: set `app = "<your-app-name>"` and pick a region
fly volumes create hackline_data --region <region> --size 1
fly deploy
```

The first boot prints a one-time claim token to the machine log:

```bash
fly logs | grep -A2 'CLAIM TOKEN'
```

Use it with `hackline login --server https://<app>.fly.dev --token <token>`
to bind the first user.

## Updating

```bash
cd hackline
fly deploy
```

State on `/data` (SQLite, future ACME cache) survives the rebuild
because the volume is reattached.

## Reachability caveats

The current config assumes devices can either:

1. Reach `https://<app>.fly.dev` (REST + SSE on :443), or
2. Open raw outbound TCP to `<app>.fly.dev:7448` for Zenoh.

Devices behind corporate proxies, captive portals, or strict mobile
networks will only have option 1. For those:

- Use REST + SSE only and remove the `[[services]]` block in
  `fly.toml` that exposes `:7448`, plus the `tcp/0.0.0.0:7448` line
  in `deploy/fly/gateway.toml`.
- For HTTP-shaped tunnels, set `http_listen` in `gateway.toml` and
  route `device-<id>.<base>` through Fly's HTTPS edge — the gateway
  already implements host-header routing
  (see `crates/hackline-gateway/src/tunnel/http_router.rs`).
- Raw-TCP tunnels through restrictive networks are not solved by
  this image; they need either Zenoh-over-WebSocket on :443 (the
  workspace currently builds Zenoh with `transport_tcp` only) or a
  per-tunnel `[[services]]` block in `fly.toml`.

## Scaling

Single machine only. The gateway uses SQLite as the source of truth
and `r2d2_sqlite` as the connection pool — multi-machine scale-out
needs a different storage layer first. `min_machines_running = 1`
and `auto_stop_machines = false` keep the one machine warm so
long-lived SSE streams and Zenoh sessions don't drop.

## Switching to in-process TLS

If you want the gateway to terminate TLS itself (ACME, manual certs,
or self-signed) instead of relying on Fly's edge:

1. Edit the `cargo build` line in [Dockerfile](../Dockerfile) to add
   `--features tls`.
2. Add a `[tls]` block to [gateway.toml](./gateway.toml) per the
   schema in `crates/hackline-gateway/src/config.rs`.
3. In `fly.toml`, swap the `[http_service]` block for a raw TCP
   `[[services]]` on internal port 443 and set `force_https = false`
   (or remove the field). Fly will then forward TLS bytes untouched.
