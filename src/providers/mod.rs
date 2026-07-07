//! Providers: fetch and cache metric snapshots for the diagnostic engine.
use std::time::Duration;

use crate::metrics::MetricSeriesSnapshot;

pub mod cache;
pub mod cached;
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

/// Probe `url` to choose between a raw `/metrics` scrape and the Prometheus
/// query API, then build the matching provider.
pub async fn resolve_provider(
    url: &str,
    timeout: f64,
    since: &str,
    model: Option<&str>,
) -> Result<Box<dyn Provider>, ProviderError> {
    use crate::clients::{ResolvedClient, resolve_client};
    let resolved = resolve_client(url, timeout)
        .await
        .map_err(ProviderError::Fetch)?;
    let provider: Box<dyn Provider> = match resolved {
        ResolvedClient::Scrape(_) => Box::new(scrape::new(url, timeout, since, model)?),
        ResolvedClient::Prometheus(_) => Box::new(prometheus::new(url, timeout, since, model)?),
    };
    Ok(provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_error_display() {
        let err = ProviderError::Parse("bad metrics".into());
        assert_eq!(err.to_string(), "snapshot parsing failed: bad metrics");
    }

    #[test]
    fn not_configured_display() {
        assert_eq!(
            ProviderError::NotConfigured.to_string(),
            "provider not configured"
        );
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
