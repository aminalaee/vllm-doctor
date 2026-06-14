//! Raw Prometheus exposition text format client.
use std::collections::HashMap;

use reqwest::Client as HttpClient;

use super::Client;
use super::error::ClientError;
use super::scrape::parser::{ScrapeSample, parse_scrape};
use crate::metrics::series::MetricSample;

pub mod parser;

#[derive(Debug)]
pub struct ScrapeClient {
    url: String,
    client: HttpClient,
}

impl ScrapeClient {
    pub fn new(url: impl Into<String>, timeout: f64) -> Result<Self, ClientError> {
        Self::with_client(
            url,
            HttpClient::builder()
                .timeout(std::time::Duration::from_secs_f64(timeout))
                .build()?,
        )
    }

    pub fn with_client(url: impl Into<String>, client: HttpClient) -> Result<Self, ClientError> {
        Ok(Self {
            url: url.into(),
            client,
        })
    }

    fn matches(sample: &ScrapeSample, name: &str, labels: &HashMap<String, String>) -> bool {
        sample.metric == name && labels.iter().all(|(k, v)| sample.labels.get(k) == Some(v))
    }

    fn parse_query(query: &str) -> (&str, HashMap<String, String>) {
        if let Some(pos) = query.find('{') {
            let name = &query[..pos];
            let end = query.rfind('}').unwrap_or(query.len());
            (name.trim(), parse_labels(&query[pos + 1..end]))
        } else {
            (query, HashMap::new())
        }
    }
}

fn parse_labels(input: &str) -> HashMap<String, String> {
    input
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?.trim();
            let value = parts.next()?.trim().trim_matches('"');
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

#[async_trait::async_trait]
impl Client for ScrapeClient {
    async fn query(&self, metric_name: &str) -> Result<Vec<MetricSample>, ClientError> {
        let response = self.client.get(&self.url).send().await?;
        response.error_for_status_ref()?;
        let text = response.text().await?;
        let (name, labels) = Self::parse_query(metric_name);
        Ok(parse_scrape(&text)
            .into_iter()
            .filter(|s| ScrapeClient::matches(s, name, &labels))
            .map(|s| MetricSample {
                labels: s.labels,
                value: s.value,
                timestamp: s.timestamp,
            })
            .collect())
    }

    async fn query_increase(
        &self,
        _metric_name: &str,
        _since: &str,
    ) -> Result<Option<Vec<MetricSample>>, ClientError> {
        Ok(None)
    }

    async fn query_percentile(
        &self,
        _metric: &str,
        _quantile: f64,
        _model: Option<&str>,
        _since: &str,
    ) -> Result<Option<f64>, ClientError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_labels_empty() {
        assert!(parse_labels("").is_empty());
    }

    #[test]
    fn parse_labels_single() {
        let labels = parse_labels("model_name=\"llama\"");
        assert_eq!(labels.get("model_name"), Some(&"llama".to_string()));
    }

    #[test]
    fn parse_labels_multiple() {
        let labels = parse_labels("model_name=\"llama\",pod=\"a\"");
        assert_eq!(labels.len(), 2);
        assert_eq!(labels.get("model_name"), Some(&"llama".to_string()));
        assert_eq!(labels.get("pod"), Some(&"a".to_string()));
    }
}
