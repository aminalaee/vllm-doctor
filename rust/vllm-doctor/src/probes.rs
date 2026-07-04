//! Probes: raw Prometheus queries that feed the collector.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::clients::Client;
use crate::clients::error::ClientError;
use crate::clients::label_selector;
use crate::metrics::series::MetricSeries;

/// Probes are generated from the metric table in `crate::metrics`.
pub use crate::metrics::PROBES;

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
    pub(crate) fn new(kind: ProbeKind, metric: impl Into<String>) -> Self {
        Self {
            kind,
            metric: metric.into(),
            quantile: 0.0,
            labels: HashMap::new(),
        }
    }

    pub(crate) fn with_quantile(mut self, quantile: f64) -> Self {
        self.quantile = quantile;
        self
    }

    pub(crate) fn with_labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels = labels;
        self
    }
}

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
                None => client.query(&expr).await?,
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
