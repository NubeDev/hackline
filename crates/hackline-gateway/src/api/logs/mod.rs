//! `/v1/log` REST + SSE surface. Same shape as events but with a
//! `level` filter. The path is `/v1/log` (singular) per SCOPE.md §5.3.

pub mod list;
pub mod stream;
