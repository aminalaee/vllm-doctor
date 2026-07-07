//! Collector: turn raw probe results into the snapshot used by rules.
use std::sync::Arc;

use crate::clients::Client;
use crate::clients::error::ClientError;
use crate::metrics::METRIC_SPECS;
use crate::metrics::MetricSeriesSnapshot;
use crate::probes::run_probes;

/// Collect metrics for the requested window and optional model filter.
pub async fn collect(
    client: Arc<dyn Client + Send + Sync>,
    since: &str,
    model: Option<&str>,
) -> Result<MetricSeriesSnapshot, ClientError> {
    let since = if since == "now" { "5m" } else { since };
    let needed: std::collections::HashSet<String> = METRIC_SPECS
        .iter()
        .flat_map(|spec| spec.probe_names())
        .collect();
    let raw = run_probes(client, needed, since, model).await?;
    let series = MetricSeriesSnapshot::from_raw(raw);
    Ok(series)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::clients::Client;
    use crate::metrics::series::MetricSample;

    #[derive(Default)]
    struct StubClient {
        queries: Mutex<Vec<String>>,
        return_value: f64,
    }

    #[async_trait::async_trait]
    impl Client for StubClient {
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
    async fn returns_metrics() {
        let client = Arc::new(StubClient {
            return_value: 10.0,
            ..Default::default()
        });
        let snapshot = collect(client, "1h", None).await.unwrap();
        assert_eq!(snapshot.num_requests_running.value(), Some(10.0));
        assert_eq!(snapshot.num_requests_waiting.value(), Some(10.0));
        assert_eq!(snapshot.kv_cache_usage_perc.value(), Some(10.0));
    }

    #[tokio::test]
    async fn now_defaults_to_5m() {
        let client = Arc::new(StubClient::default());
        let _ = collect(client.clone(), "now", None).await.unwrap();
        let queries = client.queries.lock().unwrap();
        assert!(queries.iter().any(|q| q.contains("[5m]")));
    }

    #[tokio::test]
    async fn missing_metrics_are_none() {
        let client = Arc::new(StubClient {
            return_value: 0.0,
            ..Default::default()
        });
        let snapshot = collect(client, "1h", None).await.unwrap();
        assert_eq!(snapshot.num_requests_running.value(), Some(0.0));
        assert_eq!(snapshot.prefix_cache_hit_rate.value(), None);
    }
}
