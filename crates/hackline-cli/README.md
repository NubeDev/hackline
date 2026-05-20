# hackline-cli

`hackline` — a thin REST/SSE client over the gateway. Every leaf
subcommand has its own file; CLI structure is documented in
[`DOCS/CLI.md`](../../DOCS/CLI.md).

## Layout

```
src/
  main.rs              clap dispatch
  config.rs            credentials.json + env overrides
  client.rs            reqwest wrapper
  output.rs            table / JSON formatting
  error.rs             CLI error type

  cmd/
    mod.rs
    login.rs
    whoami.rs
    events.rs
    device/
      mod.rs
      list.rs
      add.rs
      show.rs
      remove.rs
    tunnel/
      mod.rs
      list.rs
      add.rs
      remove.rs
    user/
      mod.rs
      list.rs
      add.rs
      remove.rs
    token/
      mod.rs
      mint.rs
```

The CLI does **no** business logic. If a command can't be implemented
by composing existing REST calls, add the REST endpoint first.
