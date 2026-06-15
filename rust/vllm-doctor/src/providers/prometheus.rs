//! Prometheus-backed provider.
use std::sync::Arc;
use std::time::Duration;

use reqwest::Client as HttpClient;

use super::{Provider, ProviderError, ProviderMetadata, RequestCycleCache};
use crate::clients::PrometheusClient;
use crate::collector::collect;
use crate::metrics::MetricSeriesSnapshot;

/// Fetches snapshots from a Prometheus query API, with caching.
#[derive(Debug)]
pub struct PrometheusProvider {
    client: Arc<PrometheusClient>,
    cache: RequestCycleCache,
    since: String,
    model: Option<String>,
    endpoint: String,
}

impl PrometheusProvider {
    /// Build a provider with a shared connection pool.
    pub fn new(
        base_url: impl Into<String>,
        timeout: f64,
        since: impl Into<String>,
        model: Option<impl Into<String>>,
    ) -> Result<Self, ProviderError> {
        Self::with_cache_ttl(base_url, timeout, since, model, super::DEFAULT_CACHE_TTL)
    }

    /// Build a provider with a custom cache TTL.
    pub fn with_cache_ttl(
        base_url: impl Into<String>,
        timeout: f64,
        since: impl Into<String>,
        model: Option<impl Into<String>>,
        ttl: Duration,
    ) -> Result<Self, ProviderError> {
        let endpoint = base_url.into();
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs_f64(timeout))
            .build()
            .map_err(crate::clients::error::ClientError::from)?;
        let client = Arc::new(PrometheusClient::with_client(
            endpoint.clone(),
            http_client,
        )?);
        Ok(Self {
            client,
            cache: RequestCycleCache::new(ttl),
            since: since.into(),
            model: model.map(Into::into),
            endpoint,
        })
    }
}

#[async_trait::async_trait]
impl Provider for PrometheusProvider {
    async fn fetch_snapshot(&self) -> Result<MetricSeriesSnapshot, ProviderError> {
        if let Some(snapshot) = self.cache.get() {
            return Ok(snapshot);
        }
        let collection = collect(self.client.clone(), &self.since, self.model.as_deref()).await?;
        let snapshot = collection.series;
        self.cache.update(snapshot.clone());
        Ok(snapshot)
    }

    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            id: "prometheus",
            endpoint: self.endpoint.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn provider(server: &MockServer) -> PrometheusProvider {
        PrometheusProvider::new(server.uri(), 1.0, "5m", Option::<&str>::None).unwrap()
    }

    #[tokio::test]
    async fn caches_second_fetch() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/query"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "success",
                "data": {"resultType": "vector", "result": []}
            })))
            .mount(&server)
            .await;

        let provider = provider(&server);
        let _ = provider.fetch_snapshot().await.unwrap();
        let after_first = server.received_requests().await.unwrap_or_default().len();
        let _ = provider.fetch_snapshot().await.unwrap();
        let after_second = server.received_requests().await.unwrap_or_default().len();
        assert_eq!(after_first, after_second);
    }

    #[tokio::test]
    async fn metadata_has_endpoint() {
        let server = MockServer::start().await;
        let provider = provider(&server);
        assert_eq!(provider.metadata().id, "prometheus");
        assert_eq!(provider.metadata().endpoint, server.uri());
    }
}
