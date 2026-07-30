//! Provider trait and error, decoupled from the CLI's HTTP client error.
//!
//! The core diagnostic engine consumes snapshots through the [`Provider`]
//! trait. The trait and its error type live here so the backend can implement
//! providers without depending on reqwest. The CLI's concrete providers
//! implement this trait and map their [`ClientError`](crate::cli::clients::error::ClientError)
//! into [`ProviderError`] at the boundary.
use crate::core::models::MetricsSource;

/// Error returned by a provider.
///
/// `Fetch` wraps a boxed, provider-specific error so the core trait stays
/// decoupled from any concrete transport error type. CLI providers map their
/// `ClientError` into this variant at the call boundary.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("fetch failed: {0}")]
    Fetch(#[from] Box<dyn std::error::Error + Send + Sync>),
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
    pub metrics_source: MetricsSource,
}

/// A source of `MetricSeriesSnapshot`s.
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    /// Fetch a fresh snapshot.
    async fn fetch_snapshot(
        &self,
    ) -> Result<crate::core::metrics::MetricSeriesSnapshot, ProviderError>;

    /// Return static metadata about this provider.
    fn metadata(&self) -> ProviderMetadata;
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
            metrics_source: MetricsSource::Prometheus,
        };
        assert_eq!(meta.id, "prometheus");
    }
}
