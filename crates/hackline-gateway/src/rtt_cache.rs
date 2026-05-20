//! Tiny per-(org, device) cache for the synchronous `rtt_ms`
//! probe in `GET /v1/devices/:id/health`.
//!
//! Every cache hit skips a 250 ms-bounded liveliness query (and,
//! more importantly, the corresponding wakeup on the device).
//! Misses fall through to the probe and write the result back —
//! including `None`, because a failed probe is the most expensive
//! kind and re-running it for the same answer wastes a quarter
//! second per request.
//!
//! The cache is intentionally minimal: one mutex around a
//! `HashMap`, no async, no singleflight. Every critical section
//! is a constant-time map op; the lock is never held across
//! `.await`. Contention is negligible because the map is touched
//! only by `/v1/devices/:id/health`, which is a low-rate endpoint
//! even in the worst case (admin UI poll).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Defence against a pathological burst of distinct device ids
/// inside a single TTL window. On overflow the whole map is
/// cleared rather than evicting one entry; the hot keys refill
/// on the next requests and the cold keys were going to expire
/// in <1 s anyway. Picking a single eviction victim would need
/// a recency signal that this cache deliberately does not track.
const MAX_ENTRIES: usize = 4096;

#[derive(Copy, Clone)]
struct Entry {
    at: Instant,
    rtt_ms: Option<i64>,
}

#[derive(Clone)]
pub struct RttCache {
    inner: Arc<Mutex<HashMap<(i64, i64), Entry>>>,
    ttl: Duration,
}

impl RttCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            ttl,
        }
    }

    /// Returns `Some(value)` when a fresh entry exists, `None`
    /// when the key is absent or its entry has expired. The
    /// outer `Option` distinguishes "miss" from the inner
    /// `Option<i64>` ("hit, last probe failed").
    pub fn get(&self, key: (i64, i64)) -> Option<Option<i64>> {
        let map = self.inner.lock().expect("rtt cache mutex poisoned");
        let entry = map.get(&key)?;
        if entry.at.elapsed() <= self.ttl {
            Some(entry.rtt_ms)
        } else {
            None
        }
    }

    /// Stores `rtt_ms` for `key` and opportunistically prunes
    /// expired entries to bound steady-state memory. On absolute
    /// overflow the map is cleared (see `MAX_ENTRIES`).
    pub fn put(&self, key: (i64, i64), rtt_ms: Option<i64>) {
        let mut map = self.inner.lock().expect("rtt cache mutex poisoned");
        let ttl = self.ttl;
        map.retain(|_, e| e.at.elapsed() <= ttl);
        if map.len() >= MAX_ENTRIES {
            map.clear();
        }
        map.insert(
            key,
            Entry {
                at: Instant::now(),
                rtt_ms,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miss_returns_none_for_unknown_key() {
        let c = RttCache::new(Duration::from_secs(1));
        assert!(c.get((1, 2)).is_none());
    }

    #[test]
    fn hit_returns_stored_value_including_none() {
        let c = RttCache::new(Duration::from_secs(1));
        c.put((1, 2), Some(7));
        assert_eq!(c.get((1, 2)), Some(Some(7)));
        c.put((1, 3), None);
        assert_eq!(c.get((1, 3)), Some(None));
    }

    #[test]
    fn expired_entry_reads_as_miss() {
        let c = RttCache::new(Duration::from_millis(1));
        c.put((1, 2), Some(7));
        std::thread::sleep(Duration::from_millis(10));
        assert!(c.get((1, 2)).is_none());
    }

    #[test]
    fn put_replaces_existing_entry() {
        let c = RttCache::new(Duration::from_secs(1));
        c.put((1, 2), Some(7));
        c.put((1, 2), Some(11));
        assert_eq!(c.get((1, 2)), Some(Some(11)));
    }

    #[test]
    fn org_prefix_isolates_same_device_id_across_orgs() {
        let c = RttCache::new(Duration::from_secs(1));
        c.put((1, 42), Some(7));
        c.put((2, 42), Some(99));
        assert_eq!(c.get((1, 42)), Some(Some(7)));
        assert_eq!(c.get((2, 42)), Some(Some(99)));
    }
}
