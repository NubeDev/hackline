# 2026-05-15 — Goal 9: Tunnel TCP TLS wrapping

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Make `hackline-core::bridge::run_bridge` generic over `AsyncRead + AsyncWrite + Send + Unpin + 'static` | [x] |
| 1 | Add `initiate_bridge_io_with_id<S>` for callers that have already wrapped the socket (TLS) | [x] |
| 2 | Add `TunnelTls` type alias in `tunnel::tcp_listener` (`Option<TlsAcceptor>` under feature, `Option<Infallible>` otherwise) | [x] |
| 3 | Thread the acceptor through `run_tcp_listener`, `run_bridged_connection`, `manager::run`, `bin/serve.rs` | [x] |
| 4 | Branch in `bridge_socket`: TLS-handshake then call generic bridge; else call existing TcpStream bridge | [x] |
| 5 | `cargo check -p hackline-gateway` (default features) | [x] |
| 6 | `cargo check -p hackline-gateway --features tls` | [x] |
| 7 | `cargo test --workspace` (no new failures, no new warnings) | [x] |

## Outcome

`TlsState.acceptor` (built in `tls.rs` for goal 8) is now wired into
the tunnel TCP listener. When the operator configures `[tls]`, every
accepted tunnel TCP socket completes a rustls handshake before bytes
are pumped through the Zenoh bridge. With no `[tls]` block, listeners
bridge raw TCP — unchanged from goal 8.

The same cert chain that fronts the REST API also fronts the tunnel
listeners: `tls::TlsState` builds `axum_config` and `acceptor` from
the same PEM bytes, so renewal in a future tick will reload both.

Verified:
- `cargo check -p hackline-gateway` (no features) — clean
- `cargo check -p hackline-gateway --features tls` — clean
- `cargo test --workspace` — all suites pass; only the two
  pre-existing `hackline-agent` dead-code warnings, no new ones.

## Design

**Generic bridge (hackline-core).** `run_bridge` previously called
`TcpStream::into_split`, hard-coding the IO type. To support a TLS
wrapper without duplicating the byte-pump, the function is now
generic over `AsyncRead + AsyncWrite + Send + Unpin + 'static` and
uses `tokio::io::split`. The split halves carry an internal lock,
but each half stays inside one direction-specific task, so the
locking is uncontended. The public `initiate_bridge` /
`initiate_bridge_with_id` signatures still take `TcpStream` — every
existing caller (the device-side `accept_bridge`, the spike example,
`tunnel::http_router`) keeps compiling unchanged. New
`initiate_bridge_io_with_id<S>` is the entry point for pre-wrapped
sockets.

**`TunnelTls` type alias.** Avoids bleeding `#[cfg(feature = "tls")]`
into every signature in the manager + listener. Under the feature it
is `Option<tokio_rustls::TlsAcceptor>`; without it, `Option<Infallible>`
— same shape, same cloneability, but unconstructable and zero-sized.
`bridge_socket` is the one place a `#[cfg]` branch on TLS lives.

**No cert duplication.** The tunnel acceptor is `tls_state.acceptor.clone()`
in `bin/serve.rs`. `tokio_rustls::TlsAcceptor` wraps an
`Arc<ServerConfig>`, so the clone is cheap and a single rustls
config handles both REST and tunnel sockets.

**ALPN.** The acceptor in `tls.rs` advertises `h2` and `http/1.1`.
That's correct for the REST listener and harmless for raw tunnel
TCP, where the client picks whatever ALPN it wants (or none) and the
bytes pass through after the handshake. No protocol-aware logic
runs inside the bridge; rustls only enforces the handshake.

## What's next

- ACME cert renewal: background task that watches expiry and calls
  `RustlsConfig::reload()` on `axum_config` plus rebuilds the
  `TlsAcceptor` (so freshly accepted tunnel sockets pick up the new
  cert chain).
- Postgres backend behind a SQL repository trait (SCOPE.md Phase 5).
- Rust→TS codegen for `hackline-proto`; `@hackline/client` npm
  package built on Zenoh-WS (SCOPE.md Phase 5).
