//! Glue between an accepted TCP/HTTP socket and `hackline-core::bridge`.
//! Issues the Zenoh `connect` query, validates the ack, and starts the
//! byte copy.
