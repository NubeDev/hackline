//! `/v1/events` REST + SSE surface. The cursor API returns history,
//! the SSE stream returns the live broadcast — same row shape.

pub mod list;
pub mod stream;
