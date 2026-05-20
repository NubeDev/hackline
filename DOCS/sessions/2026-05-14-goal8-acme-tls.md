# Goal 8: ACME/TLS inside the gateway (SCOPE.md Phase 5)

**Date:** 2026-05-14
**Branch:** master (linear on top of goal 7)
**Status:** complete

## What was done

Added optional TLS termination directly inside hackline-gateway,
eliminating the need for a reverse proxy (Caddy/nginx) in front.

Three modes, selected via the `[tls]` config block:

1. **Self-signed** (`self_signed = true`) — generates an ephemeral
   rcgen certificate on startup. Dev/testing only.

2. **Manual certs** (`cert_path` + `key_path`) — loads PEM files from
   disk. For operators who manage their own certs.

3. **ACME** (`acme_domain` + `acme_email`) — full Let's Encrypt flow
   via `instant-acme` 0.8 with HTTP-01 challenge on port 80. Certs
   and account credentials are cached to disk; restarts reuse them.
   Supports `acme_staging = true` for rate-limit-safe testing.

Everything is gated behind the `tls` Cargo feature so the binary stays
small when TLS isn't needed.

## Files changed

| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | Added `axum-server`, `tokio-rustls`, `rustls`, `rcgen`, `instant-acme`, `rustls-pemfile` to workspace deps |
| `crates/hackline-gateway/Cargo.toml` | Added optional TLS deps, `[features] tls = [...]` |
| `crates/hackline-gateway/src/lib.rs` | Registered `#[cfg(feature = "tls")] pub mod tls` |
| `crates/hackline-gateway/src/config.rs` | Added `TlsConfig`, `TlsMode`, mode validation, 8 unit tests |
| `crates/hackline-gateway/src/tls.rs` | **New** — TLS provider: self-signed, manual, ACME init + HTTP-01 challenge server |
| `crates/hackline-gateway/src/bin/serve.rs` | Conditional `axum_server::bind_rustls` vs plain TCP; `https://` in claim token |

## Design decisions

- **`axum-server` 0.8 with `tls-rustls`** rather than rolling our own
  `hyper` TLS acceptor. `RustlsConfig` supports hot-reload which we'll
  use for ACME cert renewal later.

- **`instant-acme` 0.8** with the `rcgen` feature so `Order::finalize()`
  handles CSR generation internally. Cleaner than manual CSR assembly.

- **`TlsState` struct** holds both `axum_server::tls_rustls::RustlsConfig`
  (for REST) and `tokio_rustls::TlsAcceptor` (for tunnel TCP sockets).
  Tunnel TCP TLS wrapping not wired yet (needs the acceptor passed into
  `tcp_listener.rs`).

- **Feature-gated** — `#[cfg(feature = "tls")]` on the module and the
  serve path. Without the feature, a `[tls]` block in config produces a
  clear error at startup.

## What's next

- Wire `TlsState.acceptor` into `tcp_listener.rs` so tunnel TCP
  connections are also TLS-terminated.
- ACME cert renewal (background task that re-runs the flow before
  expiry and calls `RustlsConfig::reload()`).
- Wildcard certs for per-org subdomains (SCOPE.md §14 Q5).
- Postgres backend (rest of Phase 5).

## Test results

```
20 tests passed (14 unit + 6 integration), 0 failed
cargo check (no tls): ok
cargo check --features tls: ok
```
