//! HTTP clients for scraping vLLM `/metrics` or querying Prometheus.
use async_trait::async_trait;

use crate::core::metrics::series::MetricSample;

pub mod connection;
pub mod error;
pub mod prometheus;
pub mod scrape;

pub use connection::{ConnectionOptions, build_http_client};
pub use error::ClientError;
pub use prometheus::PrometheusClient;
pub use scrape::ScrapeClient;

#[async_trait]
pub trait Client: Send + Sync {
    async fn query(&self, metric_name: &str) -> Result<Vec<MetricSample>, ClientError>;

    async fn query_increase(
        &self,
        metric_name: &str,
        since: &str,
    ) -> Result<Option<Vec<MetricSample>>, ClientError>;

    async fn query_percentile(
        &self,
        metric: &str,
        quantile: f64,
        model: Option<&str>,
        since: &str,
    ) -> Result<Option<f64>, ClientError>;
}

#[derive(Debug)]
pub enum ResolvedClient {
    Scrape(ScrapeClient),
    Prometheus(PrometheusClient),
}

const SCRAPE_CONTENT_TYPES: [&str; 2] = ["text/plain", "application/openmetrics-text"];

/// Probe `url` once; return a `ScrapeClient` if the response looks like a raw
/// Prometheus exposition, otherwise fall back to a `PrometheusClient`.
pub async fn resolve_client(
    url: impl Into<String>,
    timeout: f64,
    opts: &ConnectionOptions,
) -> Result<ResolvedClient, ClientError> {
    let url = url.into();
    let client = build_http_client(timeout, opts)?;

    match client.get(&url).send().await {
        Ok(response) if response.status().is_success() => {
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_lowercase();
            if SCRAPE_CONTENT_TYPES
                .iter()
                .any(|ct| content_type.contains(ct))
            {
                Ok(ResolvedClient::Scrape(ScrapeClient::with_client(
                    url, client,
                )?))
            } else {
                Ok(ResolvedClient::Prometheus(PrometheusClient::with_client(
                    url, client,
                )?))
            }
        }
        Ok(_) => Ok(ResolvedClient::Prometheus(PrometheusClient::with_client(
            url, client,
        )?)),
        Err(e) => Err(ClientError::from(e)),
    }
}

/// Build a Prometheus label selector such as `{model_name="llama"}`.
pub fn label_selector(model: Option<&str>, extra: &[(&str, &str)]) -> String {
    let mut labels: Vec<(&str, &str)> = Vec::new();
    if let Some(m) = model {
        labels.push(("model_name", m));
    }
    labels.extend_from_slice(extra);
    if labels.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = labels
        .into_iter()
        .map(|(k, v)| format!("{k}=\"{}\"", v.replace('\\', "\\\\").replace('\"', "\\\"")))
        .collect();
    format!("{{{}}}", parts.join(","))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const SAMPLE_METRICS: &str = "# TYPE vllm:num_requests_running gauge\nvllm:num_requests_running{model_name=\"llama\"} 10.0\n";

    #[test]
    fn label_selector_empty_when_no_model() {
        assert_eq!(label_selector(None, &[]), "");
    }

    #[test]
    fn label_selector_includes_model_name() {
        assert_eq!(label_selector(Some("llama"), &[]), "{model_name=\"llama\"}");
    }

    #[test]
    fn label_selector_includes_extra_labels() {
        assert_eq!(
            label_selector(None, &[("finished_reason", "stop")]),
            "{finished_reason=\"stop\"}"
        );
    }

    #[test]
    fn label_selector_escapes_special_characters() {
        assert_eq!(
            label_selector(Some("a\\\"b"), &[("k", "v")]),
            r#"{model_name="a\\\"b",k="v"}"#
        );
    }

    #[tokio::test]
    async fn resolve_client_returns_scrape_for_text_plain() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(SAMPLE_METRICS)
                    .insert_header("content-type", "text/plain"),
            )
            .mount(&server)
            .await;

        let resolved = resolve_client(server.uri(), 1.0, &ConnectionOptions::default())
            .await
            .unwrap();
        assert!(matches!(resolved, ResolvedClient::Scrape(_)));
    }

    #[tokio::test]
    async fn resolve_client_returns_scrape_for_openmetrics() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(SAMPLE_METRICS)
                    .insert_header(
                        "content-type",
                        "application/openmetrics-text; version=1.0.0; charset=utf-8",
                    ),
            )
            .mount(&server)
            .await;

        let resolved = resolve_client(server.uri(), 1.0, &ConnectionOptions::default())
            .await
            .unwrap();
        assert!(matches!(resolved, ResolvedClient::Scrape(_)));
    }

    #[tokio::test]
    async fn resolve_client_returns_prometheus_for_non_text_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"status": "ok"}))
                    .insert_header("content-type", "application/json"),
            )
            .mount(&server)
            .await;

        let resolved = resolve_client(server.uri(), 1.0, &ConnectionOptions::default())
            .await
            .unwrap();
        assert!(matches!(resolved, ResolvedClient::Prometheus(_)));
    }

    #[tokio::test]
    async fn resolve_client_returns_error_on_connection_refused() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let result = resolve_client(
            format!("http://127.0.0.1:{port}"),
            1.0,
            &ConnectionOptions::default(),
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn prometheus_client_parses_instant_query() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/query"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "success",
                "data": {
                    "resultType": "vector",
                    "result": [
                        {
                            "metric": {"model_name": "llama"},
                            "value": [1234567890.0, "10.0"]
                        }
                    ]
                }
            })))
            .mount(&server)
            .await;

        let client = PrometheusClient::new(server.uri(), 1.0).unwrap();
        let samples = client.query("vllm:num_requests_running").await.unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].value, 10.0);
        assert_eq!(
            samples[0].labels.get("model_name"),
            Some(&"llama".to_string())
        );
    }

    #[tokio::test]
    async fn prometheus_client_returns_error_on_failed_query() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/query"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "error",
                "error": "bad request"
            })))
            .mount(&server)
            .await;

        let client = PrometheusClient::new(server.uri(), 1.0).unwrap();
        let err = client.query("up").await.unwrap_err();
        assert!(matches!(err, ClientError::Query(ref msg) if msg == "bad request"));
    }

    #[tokio::test]
    async fn scrape_client_parses_text_format() {
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

        let client = ScrapeClient::new(format!("{}/metrics", server.uri()), 1.0).unwrap();
        let samples = client.query("vllm:num_requests_running").await.unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].value, 10.0);
        assert_eq!(
            samples[0].labels.get("model_name"),
            Some(&"llama".to_string())
        );
    }

    #[tokio::test]
    async fn scrape_client_filters_by_label_selector() {
        let text = "# TYPE vllm:num_requests_running gauge\nvllm:num_requests_running{model_name=\"llama\"} 10.0\nvllm:num_requests_running{model_name=\"qwen\"} 5.0\n";
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/metrics"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(text)
                    .insert_header("content-type", "text/plain"),
            )
            .mount(&server)
            .await;

        let client = ScrapeClient::new(format!("{}/metrics", server.uri()), 1.0).unwrap();
        let samples = client
            .query("vllm:num_requests_running{model_name=\"llama\"}")
            .await
            .unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(
            samples[0].labels.get("model_name"),
            Some(&"llama".to_string())
        );
    }

    #[tokio::test]
    async fn prometheus_client_query_increase_wraps_expression() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/query"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "success",
                "data": {"resultType": "vector", "result": []}
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = PrometheusClient::new(server.uri(), 1.0).unwrap();
        client.query_increase("counter", "5m").await.unwrap();
    }

    #[tokio::test]
    async fn prometheus_client_query_percentile_builds_expression() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/query"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "success",
                "data": {
                    "resultType": "vector",
                    "result": [{"metric": {}, "value": [0.0, "0.95"]}]
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = PrometheusClient::new(server.uri(), 1.0).unwrap();
        let value = client
            .query_percentile("latency", 0.99, Some("llama"), "5m")
            .await
            .unwrap();
        assert_eq!(value, Some(0.95));
    }
}
