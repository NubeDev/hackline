//! CLI error type. Exit codes:
//! - 0: success
//! - 1: unexpected error
//! - 2: invalid usage (let clap handle this)
//! - 3: gateway returned 4xx
//! - 4: gateway returned 5xx / unreachable

// Errors flow through anyhow for now; exit codes refined later.
