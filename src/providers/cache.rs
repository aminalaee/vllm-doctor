//! Time-bound cache for metric snapshots.
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::metrics::MetricSeriesSnapshot;

/// Thread-safe cache with a configurable TTL.
#[derive(Debug)]
pub struct RequestCycleCache {
    ttl: Duration,
    inner: Mutex<Option<(Instant, MetricSeriesSnapshot)>>,
}

impl RequestCycleCache {
    /// Create a cache that expires after `ttl`.
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            inner: Mutex::new(None),
        }
    }

    /// Return the cached snapshot if it is still fresh.
    pub fn get(&self) -> Option<MetricSeriesSnapshot> {
        let guard = self.inner.lock().unwrap();
        guard
            .as_ref()
            .filter(|(t, _)| t.elapsed() < self.ttl)
            .map(|(_, s)| s.clone())
    }

    /// Store a new snapshot and reset the freshness timer.
    pub fn update(&self, snapshot: MetricSeriesSnapshot) {
        let mut guard = self.inner.lock().unwrap();
        *guard = Some((Instant::now(), snapshot));
    }
}

impl Default for RequestCycleCache {
    fn default() -> Self {
        Self::new(super::DEFAULT_CACHE_TTL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::series::{MetricSample, MetricSeries};

    fn sample_snapshot() -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(1.0)]),
            ..Default::default()
        }
    }

    #[test]
    fn cache_returns_none_when_empty() {
        let cache = RequestCycleCache::new(Duration::from_secs(60));
        assert!(cache.get().is_none());
    }

    #[test]
    fn cache_returns_fresh_snapshot() {
        let cache = RequestCycleCache::new(Duration::from_secs(60));
        let snapshot = sample_snapshot();
        cache.update(snapshot.clone());
        assert_eq!(cache.get(), Some(snapshot));
    }

    #[test]
    fn cache_expires_after_ttl() {
        let cache = RequestCycleCache::new(Duration::from_millis(10));
        cache.update(sample_snapshot());
        std::thread::sleep(Duration::from_millis(15));
        assert!(cache.get().is_none());
    }

    #[test]
    fn cache_update_replaces_previous_entry() {
        let cache = RequestCycleCache::new(Duration::from_secs(60));
        let first = sample_snapshot();
        let second = MetricSeriesSnapshot {
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(5.0)]),
            ..Default::default()
        };
        cache.update(first);
        cache.update(second.clone());
        assert_eq!(cache.get(), Some(second));
    }
}
