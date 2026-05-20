# hackline-core

The TCP↔Zenoh bridge plus the shared Zenoh-session helpers used by
both the gateway (initiator) and the agent (acceptor).

Anything that needs to land on **both** sides of the wire lives here.
The agent and the gateway never depend on each other; they share
through this crate. (SCOPE R2.)

## Files

| File | Owns |
|---|---|
| `lib.rs` | re-exports |
| `bridge.rs` | `tokio::io::copy_bidirectional` over a Zenoh stream |
| `session.rs` | Zenoh-session open / close helpers |
| `error.rs` | bridging error type |
