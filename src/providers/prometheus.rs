//! Prometheus-backed provider.
use super::ProviderError;
use super::client::ClientProvider;
use crate::clients::PrometheusClient;
use crate::clients::connection::ConnectionOptions;
use crate::models::MetricsSource;

/// Fetches snapshots from a Prometheus query API.
pub type PrometheusProvider = ClientProvider<PrometheusClient>;

/// Build a provider with a shared connection pool.
pub fn new(
    base_url: impl Into<String>,
    timeout: f64,
    opts: &ConnectionOptions,
    since: impl Into<String>,
    model: Option<impl Into<String>>,
) -> Result<PrometheusProvider, ProviderError> {
    ClientProvider::new(
        base_url,
        timeout,
        opts,
        since,
        model,
        "prometheus",
        MetricsSource::Prometheus,
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
    use crate::clients::ConnectionOptions;
    use crate::models::MetricsSource;

    fn provider(server: &MockServer) -> PrometheusProvider {
        prometheus_provider_new(
            server.uri(),
            1.0,
            &ConnectionOptions::default(),
            "5m",
            Option::<&str>::None,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn each_fetch_collects_fresh() {
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
        // No caching: a second fetch queries Prometheus again.
        let _ = provider.fetch_snapshot().await.unwrap();
        let after_second = server.received_requests().await.unwrap_or_default().len();
        assert!(after_second > after_first);
    }

    #[tokio::test]
    async fn metadata_has_endpoint() {
        let server = MockServer::start().await;
        let provider = provider(&server);
        assert_eq!(provider.metadata().id, "prometheus");
        assert_eq!(
            provider.metadata().metrics_source,
            MetricsSource::Prometheus
        );
        assert_eq!(provider.metadata().endpoint, server.uri());
    }
}
