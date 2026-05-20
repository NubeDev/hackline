# 2026-05-15 — Goal 10: ACME certificate renewal (Phase 5)

## Plan

| # | Step | Status |
|---|---|---|
| 0 | Add `arc-swap` and `x509-parser` to workspace + gateway `tls` feature | [x] |
| 1 | Switch `TunnelTls` (and the field on `TlsState`) to `Option<Arc<ArcSwap<TlsAcceptor>>>` so a swap propagates to every per-accept loop without restarting listeners | [x] |
| 2 | Extract `acquire_acme_cert(cfg, cache_dir)` from `init_acme` so the same code path serves first acquisition and renewal | [x] |
| 3 | Add `cert_not_after(cert_pem) -> i64` helper (unix ms), parsing the leaf cert with `x509-parser` | [x] |
| 4 | Add `acme_renew_before_days` (default 30) and `acme_check_interval_secs` (default 12*3600) to `TlsConfig` | [x] |
| 5 | On startup, when an ACME cache cert exists, check expiry; if within renewal window, re-acquire before serving | [x] |
| 6 | Add `TlsState::reload(cert_pem, key_pem)` that updates `axum_config` (via `RustlsConfig::reload_from_pem`) and the acceptor swap together | [x] |
| 7 | Add `tls::spawn_renewal(state, cfg, cache_dir) -> JoinHandle` (only spawned in ACME mode) | [x] |
| 8 | Wire renewal task into `bin/serve.rs` `tokio::select!` | [x] |
| 9 | Unit tests: `cert_not_after` round-trip with rcgen-generated cert; renewal-window predicate | [x] |
| 10 | `cargo check -p hackline-gateway` (default) + `--features tls`; `cargo test --workspace` | [x] |

## Design

**Why `ArcSwap<TlsAcceptor>`.** The tunnel manager hands a `TunnelTls`
clone to every `run_tcp_listener`, which then clones it again into
each `tokio::spawn` per accepted connection. Restarting the listener
just to swap the cert would drop in-flight connections and require
plumbing a control channel. `Arc<ArcSwap<TlsAcceptor>>` keeps the
clone-cheap shape — every existing call site is still
`tls.clone()` — but `bridge_socket` now does
`acceptor_swap.load_full()` immediately before calling
`accept(tcp)`, so the very next handshake uses whatever cert the
renewer last installed. Already-handshaken sockets keep their old
session keys until they close, which is correct: rustls doesn't
require renegotiation when the server cert rotates.

**`RustlsConfig` already hot-reloads.** `axum-server` 0.8's
`RustlsConfig::reload_from_pem` is exactly this contract for the REST
listener. We don't need to wrap it; we just call it from the renewer.

**One renewal task, ACME only.** Self-signed and manual modes don't
renew. `spawn_renewal` returns `Option<JoinHandle>`; the select arm
in `serve.rs` collapses to `pending()` when there's no task. Renewer
loop: sleep `check_interval_secs`, parse `cert.pem` from the cache
dir, and if `now + renew_before_days >= not_after`, re-run
`acquire_acme_cert` and call `state.reload(...)`. Failures log
`warn!` and retry next tick — we never poison the running cert just
because Let's Encrypt hiccuped.

**Why parse expiry on every tick instead of caching it in memory.**
The cache file on disk is the single source of truth (it's what we
load on restart). Parsing one PEM every 12 h is free, and the
alternative — caching `not_after` in `TlsState` — adds a second
field that has to stay consistent with disk across `reload()`.

**Why `x509-parser` and not webpki / rustls' own cert types.** Neither
exposes a stable accessor for `notAfter`. `x509-parser` is a small
no-`unsafe` crate (already MIT/Apache-2.0) and is the standard pick
when you need to read fields out of a leaf cert without doing path
validation.

## Outcome

ACME-issued certs now renew automatically. With an ACME `[tls]`
block the gateway:

1. On boot, loads the cached cert if present and outside the
   renewal window; otherwise re-acquires.
2. Spawns a background renewer that wakes every
   `acme_check_interval_secs` (default 12 h), parses the cached
   cert's `notAfter`, and if it falls inside
   `acme_renew_before_days` (default 30) re-runs the HTTP-01 flow,
   writes the new PEMs to the cache dir, and hot-swaps them into
   both the REST `RustlsConfig` and the tunnel `TlsAcceptor`
   (via `Arc<ArcSwap<TlsAcceptor>>`).
3. In-flight TLS sessions keep their existing keys; the next
   handshake on either the REST listener or any tunnel TCP
   listener picks up the renewed cert.

Non-ACME modes (manual, self-signed) get `acme_cache_dir = None`
and the renewer is never spawned — same behaviour as before.

Verified:
- `cargo check -p hackline-gateway` — clean
- `cargo check -p hackline-gateway --features tls` — clean
- `cargo test -p hackline-gateway --features tls` — 17 unit tests
  pass (3 new in `tls::tests`), 0 failed, no new warnings.
- `cargo test --workspace` — all suites pass; only the two
  pre-existing `hackline-agent` dead-code warnings.

## What's next

- Postgres backend behind a SQL repository trait (SCOPE.md
  Phase 5).
- Rust→TS codegen for `hackline-proto`; `@hackline/client` npm
  package built on Zenoh-WS (SCOPE.md Phase 5).
- Wildcard certs for per-org subdomains (SCOPE.md §14 Q5) — needs
  DNS-01 challenge support in `instant-acme` (the wildcard prereq)
  rather than the HTTP-01 path used here.
