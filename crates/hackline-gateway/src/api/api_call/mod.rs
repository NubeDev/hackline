//! Synchronous message-plane RPC (`POST /v1/devices/:id/api/:topic`).
//! One Zenoh `get` against `hackline/<zid>/msg/api/<topic>` — fails
//! fast with `503 device_unreachable` if the device is offline, or
//! `504 device_timeout` if the queryable doesn't reply in time.

pub mod call;
