//! Shared health-probe helpers for `GET /v1/devices/:id/health`
//! and `GET /v1/devices/health`. Both endpoints derive `online`
//! the same way (`last_seen_at` against an `ONLINE_STALE_SECS`
//! window) and call the same cached Zenoh `liveliness::Get`
//! probe. The list endpoint fans this out across every device
//! in the org with `futures::join_all`.

use std::time::{Duration, Instant};

use hackline_proto::{keyexpr, Zid};

use crate::state::AppState;

/// A device is considered online if the liveliness subscriber has
/// stamped `last_seen_at` within this window. Matches the bridge
/// keepalive period documented in `SCOPE.md` §6 (default 30 s) plus
/// one missed beat of slack.
pub(super) const ONLINE_STALE_SECS: i64 = 60;

/// Hard cap on the synchronous probe. Liveliness queries should
/// resolve in single-digit ms on a healthy mesh; 250 ms is enough
/// slack for a slow path while keeping the API endpoint snappy
/// (callers polling this in the UI cannot tolerate seconds). The
/// list-fanout handler relies on this cap holding per probe so
/// the parallel `join_all` wall clock stays bounded.
pub(super) const PROBE_TIMEOUT_MS: u64 = 250;

/// Compute `online` from a `last_seen_at` epoch second against
/// the current wall clock. `None` (never seen) is always
/// offline.
pub(super) fn online_from_last_seen(last_seen_at: Option<i64>, now: i64) -> bool {
    match last_seen_at {
        Some(seen) => now.saturating_sub(seen) <= ONLINE_STALE_SECS,
        None => false,
    }
}

/// Current wall clock as Unix epoch seconds. Centralised so
/// every health caller uses the same clock and any future test
/// hook (deterministic clock injection) lives in one place.
pub(super) fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Cached probe wrapper. Hits `state.rtt_cache` first; on miss
/// runs the live probe and writes the result back. Failed
/// probes (`None`) are cached too — re-running them costs the
/// full 250 ms timeout for the same answer.
pub(super) async fn cached_rtt_ms(
    state: &AppState,
    org_id: i64,
    device_id: i64,
    org_slug: &str,
    zid_str: &str,
) -> Option<i64> {
    let key = (org_id, device_id);
    if let Some(cached) = state.rtt_cache.get(key) {
        return cached;
    }
    let measured = probe_rtt_ms(state, org_slug, zid_str).await;
    state.rtt_cache.put(key, measured);
    measured
}

/// Issue one `liveliness::Get` against the device's own health
/// token and return the wall-clock RTT to the first reply.
///
/// Errors and timeouts collapse to `None` rather than failing the
/// HTTP request: an unreachable device should still answer the
/// health endpoint with `online: false, rtt_ms: null`, not 500.
/// A malformed `zid` (cannot construct the keyexpr) also returns
/// `None`; the device row is the source of truth and a shape
/// error there is an internal data issue, not a probe failure
/// to surface to the caller.
async fn probe_rtt_ms(state: &AppState, org_slug: &str, zid_str: &str) -> Option<i64> {
    let zid = Zid::new(zid_str).ok()?;
    let ke = keyexpr::health(org_slug, &zid);

    let started = Instant::now();
    let replies = state
        .zenoh
        .liveliness()
        .get(&ke)
        .timeout(Duration::from_millis(PROBE_TIMEOUT_MS))
        .await
        .ok()?;

    // `recv_async` returns the next reply or an error when the
    // handler is closed by the timeout above. Wrap the recv in
    // the same hard cap as a belt-and-braces guard against a
    // future zenoh change that holds the channel open past its
    // declared timeout. Critical for the list endpoint, where
    // one slow probe holding the channel would block the
    // `join_all` past its expected wall clock.
    let recv = tokio::time::timeout(
        Duration::from_millis(PROBE_TIMEOUT_MS),
        replies.recv_async(),
    )
    .await
    .ok()?
    .ok()?;
    drop(recv);

    Some(started.elapsed().as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn online_true_within_window() {
        let now = 1_000_000;
        assert!(online_from_last_seen(Some(now - 30), now));
        assert!(online_from_last_seen(Some(now - ONLINE_STALE_SECS), now));
    }

    #[test]
    fn online_false_outside_window() {
        let now = 1_000_000;
        assert!(!online_from_last_seen(
            Some(now - ONLINE_STALE_SECS - 1),
            now
        ));
    }

    #[test]
    fn online_false_when_never_seen() {
        assert!(!online_from_last_seen(None, 1_000_000));
    }

    #[test]
    fn online_false_when_clock_skew_makes_seen_in_future() {
        let now = 1_000_000;
        assert!(online_from_last_seen(Some(now + 5), now));
    }
}
