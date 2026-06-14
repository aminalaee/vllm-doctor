//! Prometheus query API client.
use reqwest::Client as HttpClient;

use super::error::ClientError;
use super::{Client, label_selector};
use crate::metrics::series::MetricSample;

#[derive(Debug)]
pub struct PrometheusClient {
    base_url: String,
    client: HttpClient,
}

impl PrometheusClient {
    pub fn new(base_url: impl Into<String>, timeout: f64) -> Result<Self, ClientError> {
        Self::with_client(
            base_url,
            HttpClient::builder()
                .timeout(std::time::Duration::from_secs_f64(timeout))
                .build()?,
        )
    }

    pub fn with_client(
        base_url: impl Into<String>,
        client: HttpClient,
    ) -> Result<Self, ClientError> {
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client,
        })
    }

    async fn get(
        &self,
        url: &str,
        params: &[(String, String)],
    ) -> Result<serde_json::Value, ClientError> {
        let response = self.client.get(url).query(params).send().await?;
        response.error_for_status_ref()?;
        Ok(response.json().await?)
    }

    fn parse_samples(result: &[serde_json::Value]) -> Vec<MetricSample> {
        result
            .iter()
            .filter_map(|r| {
                let labels = r
                    .get("metric")?
                    .as_object()?
                    .iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                    .collect();
                let value = r.get("value")?.get(1)?.as_str()?.parse().ok()?;
                let timestamp = r.get("value")?.get(0)?.as_f64();
                Some(MetricSample {
                    labels,
                    value,
                    timestamp,
                })
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl Client for PrometheusClient {
    async fn query(&self, metric_name: &str) -> Result<Vec<MetricSample>, ClientError> {
        let url = format!("{}/api/v1/query", self.base_url);
        let params = [("query".to_string(), metric_name.to_string())];
        let data = self.get(&url, &params).await?;

        if data.get("status").and_then(|v| v.as_str()) != Some("success") {
            return Err(ClientError::Query(
                data.get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error")
                    .to_string(),
            ));
        }

        let result = data
            .get("data")
            .and_then(|d| d.get("result"))
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(Self::parse_samples(&result))
    }

    async fn query_increase(
        &self,
        metric_name: &str,
        since: &str,
    ) -> Result<Option<Vec<MetricSample>>, ClientError> {
        let expr = format!("increase({metric_name}[{since}])");
        let samples = self.query(&expr).await?;
        Ok(Some(samples))
    }

    async fn query_percentile(
        &self,
        metric: &str,
        quantile: f64,
        model: Option<&str>,
        since: &str,
    ) -> Result<Option<f64>, ClientError> {
        let sel = label_selector(model);
        let expr = format!(
            "histogram_quantile({quantile}, sum by (le) (rate({metric}_bucket{sel}[{since}])))"
        );
        let samples = self.query(&expr).await?;
        Ok(samples.into_iter().next().and_then(|s| {
            if s.value.is_finite() {
                Some(s.value)
            } else {
                None
            }
        }))
    }
}
