//! Probes: raw Prometheus queries that feed the collector.
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

use crate::clients::Client;
use crate::clients::error::ClientError;
use crate::clients::label_selector;
use crate::metrics::series::MetricSeries;

const NUM_REQUESTS_RUNNING: &str = "vllm:num_requests_running";
const NUM_REQUESTS_WAITING: &str = "vllm:num_requests_waiting";
const GPU_CACHE_USAGE_PERC: &str = "vllm:kv_cache_usage_perc";
const REQUEST_SUCCESS_TOTAL: &str = "vllm:request_success_total";
const PROMPT_TOKENS_PER_SECOND: &str = "vllm:prompt_tokens_per_second";
const GENERATION_TOKENS_PER_SECOND: &str = "vllm:generation_tokens_per_second";
const TIME_TO_FIRST_TOKEN_SECONDS: &str = "vllm:time_to_first_token_seconds";
const TIME_PER_OUTPUT_TOKEN_SECONDS: &str = "vllm:request_time_per_output_token_seconds";
const PREFIX_CACHE_HITS_TOTAL: &str = "vllm:prefix_cache_hits_total";
const PREFIX_CACHE_QUERIES_TOTAL: &str = "vllm:prefix_cache_queries_total";
const REQUEST_QUEUE_TIME_SECONDS: &str = "vllm:request_queue_time_seconds";
const NUM_PREEMPTIONS_TOTAL: &str = "vllm:num_preemptions_total";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    Gauge,
    Increase,
    Percentile,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    pub kind: ProbeKind,
    pub metric: String,
    pub quantile: f64,
    pub labels: HashMap<String, String>,
}

impl Probe {
    fn new(kind: ProbeKind, metric: impl Into<String>) -> Self {
        Self {
            kind,
            metric: metric.into(),
            quantile: 0.0,
            labels: HashMap::new(),
        }
    }

    fn with_quantile(mut self, quantile: f64) -> Self {
        self.quantile = quantile;
        self
    }

    fn with_labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels = labels;
        self
    }
}

pub static PROBES: LazyLock<[(&str, Probe); 14]> =
    LazyLock::new(|| {
        [
            (
                "num_requests_running",
                Probe::new(ProbeKind::Gauge, NUM_REQUESTS_RUNNING),
            ),
            (
                "num_requests_waiting",
                Probe::new(ProbeKind::Gauge, NUM_REQUESTS_WAITING),
            ),
            (
                "kv_cache_usage_perc",
                Probe::new(ProbeKind::Gauge, GPU_CACHE_USAGE_PERC),
            ),
            (
                "prompt_tokens_per_second",
                Probe::new(ProbeKind::Gauge, PROMPT_TOKENS_PER_SECOND),
            ),
            (
                "generation_tokens_per_second",
                Probe::new(ProbeKind::Gauge, GENERATION_TOKENS_PER_SECOND),
            ),
            (
                "request_success_total",
                Probe::new(ProbeKind::Increase, REQUEST_SUCCESS_TOTAL).with_labels(HashMap::from(
                    [("finished_reason".to_string(), "stop".to_string())],
                )),
            ),
            (
                "request_error_total",
                Probe::new(ProbeKind::Increase, REQUEST_SUCCESS_TOTAL).with_labels(HashMap::from(
                    [("finished_reason".to_string(), "error".to_string())],
                )),
            ),
            (
                "request_abort_total",
                Probe::new(ProbeKind::Increase, REQUEST_SUCCESS_TOTAL).with_labels(HashMap::from(
                    [("finished_reason".to_string(), "abort".to_string())],
                )),
            ),
            (
                "ttft_p95_seconds",
                Probe::new(ProbeKind::Percentile, TIME_TO_FIRST_TOKEN_SECONDS).with_quantile(0.95),
            ),
            (
                "tpot_p95_seconds",
                Probe::new(ProbeKind::Percentile, TIME_PER_OUTPUT_TOKEN_SECONDS)
                    .with_quantile(0.95),
            ),
            (
                "queue_time_p95_seconds",
                Probe::new(ProbeKind::Percentile, REQUEST_QUEUE_TIME_SECONDS).with_quantile(0.95),
            ),
            (
                "num_preemptions_total",
                Probe::new(ProbeKind::Increase, NUM_PREEMPTIONS_TOTAL),
            ),
            (
                "prefix_hits",
                Probe::new(ProbeKind::Increase, PREFIX_CACHE_HITS_TOTAL),
            ),
            (
                "prefix_queries",
                Probe::new(ProbeKind::Increase, PREFIX_CACHE_QUERIES_TOTAL),
            ),
        ]
    });

fn probe_expr(probe: &Probe, model: Option<&str>) -> String {
    let extra: Vec<(&str, &str)> = probe
        .labels
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    format!("{}{}", probe.metric, label_selector(model, &extra))
}

async fn run_probe(
    client: Arc<dyn Client + Send + Sync>,
    probe: &Probe,
    since: &str,
    model: Option<&str>,
) -> Result<MetricSeries, ClientError> {
    let expr = probe_expr(probe, model);
    match probe.kind {
        ProbeKind::Gauge => Ok(MetricSeries::from_samples(client.query(&expr).await?)),
        ProbeKind::Increase => {
            let samples = match client.query_increase(&expr, since).await? {
                Some(samples) => samples,
                None => client.query(&expr).await.unwrap_or_default(),
            };
            Ok(MetricSeries::from_samples(samples))
        }
        ProbeKind::Percentile => {
            let value = client
                .query_percentile(&probe.metric, probe.quantile, model, since)
                .await?;
            Ok(value.map_or_else(MetricSeries::empty, MetricSeries::scalar))
        }
    }
}

/// Run the requested probes concurrently and return their raw series.
pub async fn run_probes(
    client: Arc<dyn Client + Send + Sync>,
    names: HashSet<String>,
    since: &str,
    model: Option<&str>,
) -> Result<HashMap<String, MetricSeries>, ClientError> {
    let probes_by_name: HashMap<&str, &Probe> =
        PROBES.iter().map(|(name, probe)| (*name, probe)).collect();

    let since = since.to_string();
    let model = model.map(String::from);

    let mut set = tokio::task::JoinSet::new();
    for name in names {
        let probe = probes_by_name
            .get(name.as_str())
            .copied()
            .expect("unknown probe name");
        let client = Arc::clone(&client);
        let since = since.clone();
        let model = model.clone();
        set.spawn(async move {
            let model_ref = model.as_deref();
            (name, run_probe(client, probe, &since, model_ref).await)
        });
    }

    let mut results = HashMap::new();
    while let Some(res) = set.join_next().await {
        let (name, series) = res.expect("probe task panicked");
        results.insert(name, series?);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use crate::metrics::MetricSample;

    use super::*;

    #[derive(Default)]
    struct RecordingClient {
        queries: Mutex<Vec<String>>,
        return_value: f64,
    }

    #[async_trait::async_trait]
    impl Client for RecordingClient {
        async fn query(&self, metric_name: &str) -> Result<Vec<MetricSample>, ClientError> {
            self.queries.lock().unwrap().push(metric_name.to_string());
            Ok(vec![MetricSample::new(self.return_value)])
        }

        async fn query_increase(
            &self,
            metric_name: &str,
            since: &str,
        ) -> Result<Option<Vec<MetricSample>>, ClientError> {
            self.queries
                .lock()
                .unwrap()
                .push(format!("increase({metric_name}[{since}])"));
            Ok(Some(vec![MetricSample::new(self.return_value)]))
        }

        async fn query_percentile(
            &self,
            metric: &str,
            _quantile: f64,
            _model: Option<&str>,
            since: &str,
        ) -> Result<Option<f64>, ClientError> {
            self.queries
                .lock()
                .unwrap()
                .push(format!("histogram_quantile({metric}[{since}])"));
            Ok(Some(self.return_value))
        }
    }

    #[tokio::test]
    async fn gauge_probe_uses_query() {
        let client = Arc::new(RecordingClient {
            return_value: 10.0,
            ..Default::default()
        });
        let raw = run_probes(
            client.clone(),
            HashSet::from(["num_requests_running".to_string()]),
            "1h",
            None,
        )
        .await
        .unwrap();
        assert_eq!(raw["num_requests_running"].value(), Some(10.0));
        let queries = client.queries.lock().unwrap();
        assert_eq!(queries.len(), 1);
        assert!(queries[0].contains("num_requests_running"));
        assert!(!queries[0].contains("increase("));
    }

    #[tokio::test]
    async fn counter_probe_uses_increase_fallback() {
        let client = Arc::new(RecordingClient {
            return_value: 5.0,
            ..Default::default()
        });
        let raw = run_probes(
            client.clone(),
            HashSet::from(["num_preemptions_total".to_string()]),
            "1h",
            None,
        )
        .await
        .unwrap();
        assert_eq!(raw["num_preemptions_total"].value(), Some(5.0));
        let queries = client.queries.lock().unwrap();
        assert!(queries[0].contains("increase("));
    }

    #[tokio::test]
    async fn model_label_included_in_query() {
        let client = Arc::new(RecordingClient::default());
        let _ = run_probes(
            client.clone(),
            HashSet::from(["num_requests_running".to_string()]),
            "1h",
            Some("meta-llama/Llama-3.1-8B"),
        )
        .await
        .unwrap();
        let queries = client.queries.lock().unwrap();
        assert!(queries[0].contains("meta-llama"));
    }
}
