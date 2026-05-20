//! `/v1/orgs` — tenant org administration. Owner-only mutations,
//! every authenticated user can `GET` their own org (`GET /v1/orgs/me`).
//! SCOPE.md §6 §13 Phase 4.

pub mod create;
pub mod get_me;
pub mod list;
