# hackline-proto

Pure wire types and key-expression builders shared by the gateway,
the agent, the CLI, and any future SDK. **Zero runtime dependencies**:
no `tokio`, no `zenoh`, no filesystem. (SCOPE R1.)

## Files

| File | Owns |
|---|---|
| `lib.rs` | re-exports |
| `zid.rs` | `Zid` newtype + parse / display |
| `keyexpr.rs` | builders for the keyexprs in `DOCS/KEYEXPRS.md` |
| `connect.rs` | `ConnectRequest`, `ConnectAck` |
| `agent_info.rs` | `AgentInfo` |
| `event.rs` | SSE event variants |
| `error.rs` | proto-level error type |
