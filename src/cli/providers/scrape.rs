//! Raw vLLM `/metrics` scrape provider.
use super::ProviderError;
use super::client::ClientProvider;
use crate::cli::clients::ScrapeClient;
use crate::cli::clients::connection::ConnectionOptions;
use crate::core::models::MetricsSource;

/// Fetches snapshots by scraping a vLLM `/metrics` endpoint.
pub type ScrapeProvider = ClientProvider<ScrapeClient>;

/// Build a provider with a shared connection pool.
pub fn new(
    url: impl Into<String>,
    timeout: f64,
    opts: &ConnectionOptions,
    since: impl Into<String>,
    model: Option<impl Into<String>>,
) -> Result<ScrapeProvider, ProviderError> {
    ClientProvider::new(
        url,
        timeout,
        opts,
        since,
        model,
        "scrape",
        MetricsSource::DirectScrape,
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
    use crate::cli::clients::ConnectionOptions;
    use crate::core::models::MetricsSource;

    const SAMPLE_METRICS: &str = "# TYPE vllm:num_requests_running gauge\nvllm:num_requests_running{model_name=\"llama\"} 10.0\n";

    fn provider(server: &MockServer) -> ScrapeProvider {
        scrape_provider_new(
            format!("{}/metrics", server.uri()),
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
        assert!(after_second > after_first);
    }

    #[tokio::test]
    async fn metadata_has_endpoint() {
        let server = MockServer::start().await;
        let provider = provider(&server);
        assert_eq!(provider.metadata().id, "scrape");
        assert_eq!(
            provider.metadata().metrics_source,
            MetricsSource::DirectScrape
        );
        assert!(provider.metadata().endpoint.contains("/metrics"));
    }
}
