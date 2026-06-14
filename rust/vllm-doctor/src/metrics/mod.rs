//! Metrics primitives for the diagnostic engine.
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

pub mod series;
pub mod specs;

pub use series::{Aggregate, MetricSample, MetricSeries};
pub use specs::{Direct, METRIC_SPECS, METRIC_SPECS_BY_OUTPUT, MetricDisplay, MetricSpec, Ratio};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    pub num_requests_running: Option<f64>,
    pub num_requests_waiting: Option<f64>,
    pub kv_cache_usage_perc: Option<f64>,
    pub prompt_tokens_per_second: Option<f64>,
    pub generation_tokens_per_second: Option<f64>,
    pub request_success_total: Option<f64>,
    pub request_error_total: Option<f64>,
    pub request_abort_total: Option<f64>,
    pub ttft_p95_seconds: Option<f64>,
    pub tpot_p95_seconds: Option<f64>,
    pub prefix_cache_hit_rate: Option<f64>,
    pub queue_time_p95_seconds: Option<f64>,
    pub num_preemptions_total: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetricSeriesSnapshot {
    pub num_requests_running: MetricSeries,
    pub num_requests_waiting: MetricSeries,
    pub kv_cache_usage_perc: MetricSeries,
    pub prompt_tokens_per_second: MetricSeries,
    pub generation_tokens_per_second: MetricSeries,
    pub request_success_total: MetricSeries,
    pub request_error_total: MetricSeries,
    pub request_abort_total: MetricSeries,
    pub ttft_p95_seconds: MetricSeries,
    pub tpot_p95_seconds: MetricSeries,
    pub prefix_cache_hit_rate: MetricSeries,
    pub queue_time_p95_seconds: MetricSeries,
    pub num_preemptions_total: MetricSeries,
}

impl MetricSeriesSnapshot {
    pub fn from_raw(raw: std::collections::HashMap<String, MetricSeries>) -> Self {
        let mut snapshot = Self::default();
        for spec in METRIC_SPECS.iter() {
            let series = spec.compute(&raw);
            match spec.output() {
                "num_requests_running" => snapshot.num_requests_running = series,
                "num_requests_waiting" => snapshot.num_requests_waiting = series,
                "kv_cache_usage_perc" => snapshot.kv_cache_usage_perc = series,
                "prompt_tokens_per_second" => snapshot.prompt_tokens_per_second = series,
                "generation_tokens_per_second" => snapshot.generation_tokens_per_second = series,
                "request_success_total" => snapshot.request_success_total = series,
                "request_error_total" => snapshot.request_error_total = series,
                "request_abort_total" => snapshot.request_abort_total = series,
                "ttft_p95_seconds" => snapshot.ttft_p95_seconds = series,
                "tpot_p95_seconds" => snapshot.tpot_p95_seconds = series,
                "prefix_cache_hit_rate" => snapshot.prefix_cache_hit_rate = series,
                "queue_time_p95_seconds" => snapshot.queue_time_p95_seconds = series,
                "num_preemptions_total" => snapshot.num_preemptions_total = series,
                _ => {}
            }
        }
        snapshot
    }

    pub fn to_metrics(&self) -> Metrics {
        Metrics {
            num_requests_running: self.num_requests_running.value(),
            num_requests_waiting: self.num_requests_waiting.value(),
            kv_cache_usage_perc: self.kv_cache_usage_perc.value(),
            prompt_tokens_per_second: self.prompt_tokens_per_second.value(),
            generation_tokens_per_second: self.generation_tokens_per_second.value(),
            request_success_total: self.request_success_total.value(),
            request_error_total: self.request_error_total.value(),
            request_abort_total: self.request_abort_total.value(),
            ttft_p95_seconds: self.ttft_p95_seconds.value(),
            tpot_p95_seconds: self.tpot_p95_seconds.value(),
            prefix_cache_hit_rate: self.prefix_cache_hit_rate.value(),
            queue_time_p95_seconds: self.queue_time_p95_seconds.value(),
            num_preemptions_total: self.num_preemptions_total.value(),
        }
    }
}

pub const REPLICA_LABELS: [&str; 8] = [
    "pod",
    "pod_name",
    "kubernetes_pod_name",
    "instance",
    "host",
    "hostname",
    "server",
    "endpoint",
];

pub const MODEL_LABEL: &str = "model_name";

/// All distinct values of `label` across every sample in the snapshot.
pub fn label_values(snapshot: &MetricSeriesSnapshot, label: &str) -> HashSet<String> {
    let mut values = HashSet::new();
    for series in snapshot.fields() {
        for sample in &series.samples {
            if let Some(value) = sample.labels.get(label) {
                values.insert(value.clone());
            }
        }
    }
    values
}

/// Pick the first known label that has >1 distinct values across the metric series.
///
/// Returns `None` when the snapshot looks like a single-replica deployment.
pub fn detect_replica_label(snapshot: &MetricSeriesSnapshot) -> Option<&str> {
    REPLICA_LABELS
        .into_iter()
        .find(|&label| label_values(snapshot, label).len() > 1)
}

impl MetricSeriesSnapshot {
    fn fields(&self) -> [&MetricSeries; 13] {
        [
            &self.num_requests_running,
            &self.num_requests_waiting,
            &self.kv_cache_usage_perc,
            &self.prompt_tokens_per_second,
            &self.generation_tokens_per_second,
            &self.request_success_total,
            &self.request_error_total,
            &self.request_abort_total,
            &self.ttft_p95_seconds,
            &self.tpot_p95_seconds,
            &self.prefix_cache_hit_rate,
            &self.queue_time_p95_seconds,
            &self.num_preemptions_total,
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn sample(value: f64, labels: &[(&str, &str)]) -> MetricSample {
        MetricSample {
            labels: labels
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            value,
            timestamp: None,
        }
    }

    #[test]
    fn snapshot_from_raw_applies_aggregation_specs() {
        let mut raw = HashMap::new();
        raw.insert(
            "kv_cache_usage_perc".to_string(),
            MetricSeries {
                samples: vec![sample(0.5, &[("pod", "a")]), sample(0.9, &[("pod", "b")])],
                aggregate_by: Aggregate::Sum,
            },
        );
        raw.insert("prefix_hits".to_string(), MetricSeries::scalar(80.0));
        raw.insert("prefix_queries".to_string(), MetricSeries::scalar(100.0));

        let snapshot = MetricSeriesSnapshot::from_raw(raw);
        assert_eq!(snapshot.kv_cache_usage_perc.value(), Some(0.9));

        let metrics = snapshot.to_metrics();
        assert_eq!(metrics.prefix_cache_hit_rate, Some(0.8));
    }

    #[test]
    fn label_values_collects_across_series() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_running: MetricSeries {
                samples: vec![sample(1.0, &[("pod", "a")]), sample(2.0, &[("pod", "b")])],
                aggregate_by: Aggregate::Sum,
            },
            num_requests_waiting: MetricSeries {
                samples: vec![sample(3.0, &[("pod", "c")])],
                aggregate_by: Aggregate::Sum,
            },
            ..Default::default()
        };

        let values = label_values(&snapshot, "pod");
        assert_eq!(values.len(), 3);
        assert!(values.contains("a"));
        assert!(values.contains("b"));
        assert!(values.contains("c"));
    }

    #[test]
    fn detect_replica_label_finds_pod() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_running: MetricSeries {
                samples: vec![sample(1.0, &[("pod", "a")]), sample(2.0, &[("pod", "b")])],
                aggregate_by: Aggregate::Sum,
            },
            ..Default::default()
        };
        assert_eq!(detect_replica_label(&snapshot), Some("pod"));
    }

    #[test]
    fn detect_replica_label_returns_none_for_single_replica() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_running: MetricSeries {
                samples: vec![sample(1.0, &[("pod", "a")])],
                aggregate_by: Aggregate::Sum,
            },
            ..Default::default()
        };
        assert_eq!(detect_replica_label(&snapshot), None);
    }
}
