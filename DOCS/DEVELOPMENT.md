# Development

## Local loop

```sh
cargo check --workspace
cargo test  --workspace
cargo fmt   --all
cargo clippy --workspace --all-targets -- -D warnings
```

There is no `make`, no `mani`, no orchestration. Cargo is enough.

## Running a gateway against a local Zenoh router

1. Start a Zenoh router somewhere reachable:
   ```sh
   zenohd --listen tcp/127.0.0.1:7447
   ```
2. Start the agent (as root if it needs to bind low ports):
   ```sh
   cargo run -p hackline-agent -- --config dev/agent.toml
   ```
3. Start the gateway:
   ```sh
   cargo run -p hackline-gateway -- serve --config dev/gateway.toml
   ```
4. Claim it:
   ```sh
   cargo run -p hackline-cli -- login \
     --server http://127.0.0.1:8080 \
     --token "<copy from gateway log>" \
     --owner alice
   ```
5. Add a tunnel:
   ```sh
   cargo run -p hackline-cli -- tunnel add \
     --device 1 --tcp 22 --public-port 2222
   ```
6. SSH through it:
   ```sh
   ssh -p 2222 user@127.0.0.1
   ```

`dev/` config files are not checked in; copy from `DOCS/CONFIG.md`.

## Tests

- `cargo test --workspace` runs the unit + integration suites.
- Integration tests spin up an in-process Zenoh peer (no external
  router required). The Phase 1 spike confirms two `Session`s in one
  process can talk via `mode=peer` with a fixed `connect/listen`
  endpoint — if that test fails, the loopback transport setup is
  wrong; do not paper over it with mocks.

## Conventional commits

`feat(gateway): …`, `fix(agent): …`, `docs: …`, `chore: …`.
Commit only when the user asks. Never amend; new commit each time.
