//! Prometheus-backed provider.
use std::time::Duration;

use super::ProviderError;
use super::cached::CachedProvider;
use crate::clients::PrometheusClient;

/// Fetches snapshots from a Prometheus query API, with caching.
pub type PrometheusProvider = CachedProvider<PrometheusClient>;

/// Build a provider with a shared connection pool.
pub fn new(
    base_url: impl Into<String>,
    timeout: f64,
    since: impl Into<String>,
    model: Option<impl Into<String>>,
) -> Result<PrometheusProvider, ProviderError> {
    with_cache_ttl(base_url, timeout, since, model, super::DEFAULT_CACHE_TTL)
}

/// Build a provider with a custom cache TTL.
pub fn with_cache_ttl(
    base_url: impl Into<String>,
    timeout: f64,
    since: impl Into<String>,
    model: Option<impl Into<String>>,
    ttl: Duration,
) -> Result<PrometheusProvider, ProviderError> {
    CachedProvider::with_cache_ttl(
        base_url,
        timeout,
        since,
        model,
        "prometheus",
        ttl,
        PrometheusClient::with_client,
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::super::Provider;
    use super::PrometheusProvider;
    use super::new as prometheus_provider_new;

    fn provider(server: &MockServer) -> PrometheusProvider {
        prometheus_provider_new(server.uri(), 1.0, "5m", Option::<&str>::None).unwrap()
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
