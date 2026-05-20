//! First-boot claim flow. The pending row is generated at startup if
//! `users` is empty; `POST /v1/claim` consumes it atomically — the
//! delete and insert run in one transaction so two simultaneous
//! claimants cannot both win.
//!
//! Business logic lives in `db::claim`; this module re-exports what
//! the API handlers need.
