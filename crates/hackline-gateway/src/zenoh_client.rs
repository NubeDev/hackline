//! The single Zenoh session the gateway holds open. Wraps
//! `hackline-core::session` with gateway-specific config (ACL,
//! discovery mode, reconnect policy).
