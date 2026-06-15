//! Providers: fetch and cache metric snapshots for the diagnostic engine.
use std::time::Duration;

use crate::metrics::MetricSeriesSnapshot;

pub mod cache;
pub mod prometheus;
pub mod scrape;

pub use cache::RequestCycleCache;
pub use prometheus::PrometheusProvider;
pub use scrape::ScrapeProvider;

/// Error returned by a provider.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("fetch failed: {0}")]
    Fetch(#[from] crate::clients::error::ClientError),
    #[error("snapshot parsing failed: {0}")]
    Parse(String),
    #[error("provider not configured")]
    NotConfigured,
}

/// Metadata identifying a provider instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderMetadata {
    pub id: &'static str,
    pub endpoint: String,
}

/// A source of `MetricSeriesSnapshot`s.
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    /// Return a fresh snapshot, using the cache when possible.
    async fn fetch_snapshot(&self) -> Result<MetricSeriesSnapshot, ProviderError>;

    /// Return static metadata about this provider.
    fn metadata(&self) -> ProviderMetadata;
}

/// Default freshness window for cached snapshots.
pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(15);

/// Run the full diagnostic pipeline once.
pub async fn diagnose(
    provider: &dyn Provider,
    registry: &crate::rules::RuleRegistry,
) -> Result<Vec<crate::rules::RuleResult>, ProviderError> {
    let snapshot = provider.fetch_snapshot().await?;
    Ok(registry.run_all(&snapshot))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::rules::build_registry;
    use crate::{
        config::Config,
        metrics::series::{MetricSample, MetricSeries},
    };
    use std::sync::Mutex;

    #[derive(Default)]
    struct TestProvider {
        snapshots: Mutex<Vec<MetricSeriesSnapshot>>,
        calls: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl Provider for TestProvider {
        async fn fetch_snapshot(&self) -> Result<MetricSeriesSnapshot, ProviderError> {
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            let idx = (*calls - 1) % self.snapshots.lock().unwrap().len();
            Ok(self.snapshots.lock().unwrap()[idx].clone())
        }

        fn metadata(&self) -> ProviderMetadata {
            ProviderMetadata {
                id: "test",
                endpoint: "test://local".to_string(),
            }
        }
    }

    fn empty_snapshot() -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(60.0)]),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn diagnose_runs_pipeline() {
        let provider = TestProvider {
            snapshots: Mutex::new(vec![empty_snapshot()]),
            ..Default::default()
        };
        let registry = build_registry(&Config::default());
        let results = diagnose(&provider, &registry).await.unwrap();
        assert_eq!(results.len(), 10);
    }

    #[test]
    fn provider_metadata_is_static() {
        let meta = ProviderMetadata {
            id: "prometheus",
            endpoint: "http://prometheus:9090".to_string(),
        };
        assert_eq!(meta.id, "prometheus");
    }
}
