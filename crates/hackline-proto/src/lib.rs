//! Wire types and key-expression builders shared by every hackline
//! component. Pure types only — no I/O, no async, no filesystem.

pub mod agent_info;
pub mod connect;
pub mod error;
pub mod event;
pub mod keyexpr;
pub mod msg;
pub mod zid;

pub use agent_info::AgentInfo;
pub use connect::{ConnectAck, ConnectRequest};
pub use msg::{ApiReply, ApiRequest, CmdAck, CmdEnvelope, CmdResult, LogLevel, MsgEnvelope};
pub use zid::Zid;
