//! Raw vLLM `/metrics` scrape provider.
use std::time::Duration;

use super::ProviderError;
use super::cached::CachedProvider;
use crate::clients::ScrapeClient;

/// Fetches snapshots by scraping a vLLM `/metrics` endpoint, with caching.
pub type ScrapeProvider = CachedProvider<ScrapeClient>;

/// Build a provider with a shared connection pool.
pub fn new(
    url: impl Into<String>,
    timeout: f64,
    since: impl Into<String>,
    model: Option<impl Into<String>>,
) -> Result<ScrapeProvider, ProviderError> {
    with_cache_ttl(url, timeout, since, model, super::DEFAULT_CACHE_TTL)
}

/// Build a provider with a custom cache TTL.
pub fn with_cache_ttl(
    url: impl Into<String>,
    timeout: f64,
    since: impl Into<String>,
    model: Option<impl Into<String>>,
    ttl: Duration,
) -> Result<ScrapeProvider, ProviderError> {
    CachedProvider::with_cache_ttl(
        url,
        timeout,
        since,
        model,
        "scrape",
        ttl,
        ScrapeClient::with_client,
    )
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::super::Provider;
    use super::ScrapeProvider;
    use super::new as scrape_provider_new;

    const SAMPLE_METRICS: &str = "# TYPE vllm:num_requests_running gauge\nvllm:num_requests_running{model_name=\"llama\"} 10.0\n";

    fn provider(server: &MockServer) -> ScrapeProvider {
        scrape_provider_new(
            format!("{}/metrics", server.uri()),
            1.0,
            "5m",
            Option::<&str>::None,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn caches_second_fetch() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/metrics"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(SAMPLE_METRICS)
                    .insert_header("content-type", "text/plain"),
            )
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
        assert_eq!(provider.metadata().id, "scrape");
        assert!(provider.metadata().endpoint.contains("/metrics"));
    }
}
