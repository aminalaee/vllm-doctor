//! Signal Layer: Translate raw metric snapshots into domain-specific signals.
use std::collections::{HashMap, HashSet};

use crate::metrics::{MODEL_LABEL, MetricSeriesSnapshot};

/// Signals represent a meaningful property of the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Signal {
    NumRequestsRunning,
    NumRequestsWaiting,
    KvCacheUsagePerc,
    PromptTokensPerSecond,
    GenerationTokensPerSecond,
    RequestSuccessTotal,
    RequestErrorTotal,
    RequestAbortTotal,
    TtftP95Seconds,
    TpotP95Seconds,
    PrefixCacheHitRate,
    QueueTimeP95Seconds,
    NumPreemptionsTotal,
    TotalRequests,
    ErrorRate,
    AbortRate,
    ReplicaRunningImbalance,
}

impl std::fmt::Display for Signal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::NumRequestsRunning => "num_requests_running",
            Self::NumRequestsWaiting => "num_requests_waiting",
            Self::KvCacheUsagePerc => "kv_cache_usage_perc",
            Self::PromptTokensPerSecond => "prompt_tokens_per_second",
            Self::GenerationTokensPerSecond => "generation_tokens_per_second",
            Self::RequestSuccessTotal => "request_success_total",
            Self::RequestErrorTotal => "request_error_total",
            Self::RequestAbortTotal => "request_abort_total",
            Self::TtftP95Seconds => "ttft_p95_seconds",
            Self::TpotP95Seconds => "tpot_p95_seconds",
            Self::PrefixCacheHitRate => "prefix_cache_hit_rate",
            Self::QueueTimeP95Seconds => "queue_time_p95_seconds",
            Self::NumPreemptionsTotal => "num_preemptions_total",
            Self::TotalRequests => "total_requests",
            Self::ErrorRate => "error_rate",
            Self::AbortRate => "abort_rate",
            Self::ReplicaRunningImbalance => "replica_running_imbalance",
        };
        write!(f, "{s}")
    }
}

pub struct SignalGraph<'a> {
    snapshot: &'a MetricSeriesSnapshot,
}

impl<'a> SignalGraph<'a> {
    pub fn new(snapshot: &'a MetricSeriesSnapshot) -> Self {
        Self { snapshot }
    }

    pub fn snapshot(&self) -> &'a MetricSeriesSnapshot {
        self.snapshot
    }

    /// Evaluate a signal. Returns `None` when the underlying metric is absent.
    pub fn evaluate(&self, signal: Signal) -> Option<f64> {
        match signal {
            Signal::NumRequestsRunning => self.snapshot.num_requests_running.value(),
            Signal::NumRequestsWaiting => self.snapshot.num_requests_waiting.value(),
            Signal::KvCacheUsagePerc => self.snapshot.kv_cache_usage_perc.value(),
            Signal::PromptTokensPerSecond => self.snapshot.prompt_tokens_per_second.value(),
            Signal::GenerationTokensPerSecond => self.snapshot.generation_tokens_per_second.value(),
            Signal::RequestSuccessTotal => self.snapshot.request_success_total.value(),
            Signal::RequestErrorTotal => self.snapshot.request_error_total.value(),
            Signal::RequestAbortTotal => self.snapshot.request_abort_total.value(),
            Signal::TtftP95Seconds => self.snapshot.ttft_p95_seconds.value(),
            Signal::TpotP95Seconds => self.snapshot.tpot_p95_seconds.value(),
            Signal::PrefixCacheHitRate => self.snapshot.prefix_cache_hit_rate.value(),
            Signal::QueueTimeP95Seconds => self.snapshot.queue_time_p95_seconds.value(),
            Signal::NumPreemptionsTotal => self.snapshot.num_preemptions_total.value(),
            Signal::TotalRequests => {
                let success = self.evaluate(Signal::RequestSuccessTotal)?;
                let errors = self.evaluate(Signal::RequestErrorTotal)?;
                let aborts = self.evaluate(Signal::RequestAbortTotal)?;
                Some(success + errors + aborts)
            }
            Signal::ErrorRate => {
                let total = self.evaluate(Signal::TotalRequests)?;
                if total == 0.0 {
                    Some(0.0)
                } else {
                    Some(self.evaluate(Signal::RequestErrorTotal)? / total)
                }
            }
            Signal::AbortRate => {
                let total = self.evaluate(Signal::TotalRequests)?;
                if total == 0.0 {
                    Some(0.0)
                } else {
                    Some(self.evaluate(Signal::RequestAbortTotal)? / total)
                }
            }
            Signal::ReplicaRunningImbalance => {
                let _label = self.replica_label()?;
                let mut worst: Option<f64> = None;
                for model in self.models() {
                    let values = self.per_replica(Signal::NumRequestsRunning, model.as_deref());
                    if values.len() < 2 {
                        continue;
                    }
                    let _total: f64 = values.values().sum();
                    let hi = values.values().cloned().max_by(|a, b| a.total_cmp(b))?;
                    let lo = values.values().cloned().min_by(|a, b| a.total_cmp(b))?;
                    if hi == lo {
                        continue;
                    }
                    let imbalance = if lo == 0.0 { hi } else { hi / lo };
                    worst = worst.map(|w| w.max(imbalance)).or(Some(imbalance));
                }
                worst
            }
        }
    }

    /// Evaluate a signal, falling back to `default` when absent.
    pub fn evaluate_or(&self, signal: Signal, default: f64) -> f64 {
        self.evaluate(signal).unwrap_or(default)
    }

    /// Evaluate a signal, returning `default` when absent or non-finite.
    pub fn evaluate_finite(&self, signal: Signal, default: f64) -> f64 {
        self.evaluate(signal)
            .and_then(|v| if v.is_finite() { Some(v) } else { None })
            .unwrap_or(default)
    }

    /// Return the replica-identifying label, if any.
    pub fn replica_label(&self) -> Option<&str> {
        crate::metrics::detect_replica_label(self.snapshot)
    }

    /// Return the set of model names seen across running/waiting/cache series.
    pub fn models(&self) -> Vec<Option<String>> {
        let mut values = HashSet::new();
        let mut labeled = false;
        for series in [
            &self.snapshot.num_requests_running,
            &self.snapshot.num_requests_waiting,
            &self.snapshot.kv_cache_usage_perc,
        ] {
            for sample in &series.samples {
                if let Some(model) = sample.labels.get(MODEL_LABEL) {
                    labeled = true;
                    values.insert(model.clone());
                }
            }
        }
        if labeled {
            values.into_iter().map(Some).collect()
        } else {
            vec![None]
        }
    }

    /// Return per-replica values for a signal within one model group.
    ///
    /// Only `NumRequestsRunning`, `NumRequestsWaiting`, and `KvCacheUsagePerc`
    /// support replica grouping; other signals return an empty map.
    pub fn per_replica(&self, signal: Signal, model: Option<&str>) -> HashMap<String, f64> {
        let label = match self.replica_label() {
            Some(l) => l,
            None => return HashMap::new(),
        };

        let series = match signal {
            Signal::NumRequestsRunning => &self.snapshot.num_requests_running,
            Signal::NumRequestsWaiting => &self.snapshot.num_requests_waiting,
            Signal::KvCacheUsagePerc => &self.snapshot.kv_cache_usage_perc,
            _ => return HashMap::new(),
        };

        let scoped = match model {
            Some(m) => {
                let mut labels = HashMap::new();
                labels.insert(MODEL_LABEL.to_string(), m.to_string());
                series.filter(&labels)
            }
            None => series.clone(),
        };

        scoped
            .by(label)
            .into_iter()
            .filter_map(|(k, v)| v.filter(|v| v.is_finite()).map(|v| (k, v)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::series::{MetricSample, MetricSeries};

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

    fn snapshot_with_running(samples: Vec<MetricSample>) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(samples),
            ..Default::default()
        }
    }

    fn balanced_snapshot() -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![
                sample(10.0, &[("pod", "a"), ("model_name", "llama")]),
                sample(10.0, &[("pod", "b"), ("model_name", "llama")]),
            ]),
            ..Default::default()
        }
    }

    fn imbalanced_snapshot() -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![
                sample(10.0, &[("pod", "a"), ("model_name", "llama")]),
                sample(30.0, &[("pod", "b"), ("model_name", "llama")]),
            ]),
            num_requests_waiting: MetricSeries::from_samples(vec![
                sample(4.0, &[("pod", "a"), ("model_name", "llama")]),
                sample(6.0, &[("pod", "b"), ("model_name", "llama")]),
            ]),
            kv_cache_usage_perc: MetricSeries::from_samples(vec![
                sample(0.5, &[("pod", "a"), ("model_name", "llama")]),
                sample(0.9, &[("pod", "b"), ("model_name", "llama")]),
            ]),
            ..Default::default()
        }
    }

    fn zero_low_snapshot() -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![
                sample(0.0, &[("pod", "a"), ("model_name", "llama")]),
                sample(10.0, &[("pod", "b"), ("model_name", "llama")]),
            ]),
            ..Default::default()
        }
    }

    #[test]
    fn display_all_signals() {
        let signals = [
            Signal::NumRequestsRunning,
            Signal::NumRequestsWaiting,
            Signal::KvCacheUsagePerc,
            Signal::PromptTokensPerSecond,
            Signal::GenerationTokensPerSecond,
            Signal::RequestSuccessTotal,
            Signal::RequestErrorTotal,
            Signal::RequestAbortTotal,
            Signal::TtftP95Seconds,
            Signal::TpotP95Seconds,
            Signal::PrefixCacheHitRate,
            Signal::QueueTimeP95Seconds,
            Signal::NumPreemptionsTotal,
            Signal::TotalRequests,
            Signal::ErrorRate,
            Signal::AbortRate,
            Signal::ReplicaRunningImbalance,
        ];
        for signal in signals {
            assert!(!signal.to_string().is_empty());
        }
    }

    #[test]
    fn evaluate_returns_value_for_gauges() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_running: MetricSeries::scalar(5.0),
            num_requests_waiting: MetricSeries::scalar(3.0),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        assert_eq!(graph.evaluate(Signal::NumRequestsRunning), Some(5.0));
        assert_eq!(graph.evaluate(Signal::NumRequestsWaiting), Some(3.0));
    }

    #[test]
    fn evaluate_returns_none_for_missing_metric() {
        let snapshot = MetricSeriesSnapshot::default();
        let graph = SignalGraph::new(&snapshot);
        assert_eq!(graph.evaluate(Signal::NumRequestsRunning), None);
        assert_eq!(graph.evaluate(Signal::ReplicaRunningImbalance), None);
    }

    #[test]
    fn evaluate_or_uses_default() {
        let snapshot = MetricSeriesSnapshot::default();
        let graph = SignalGraph::new(&snapshot);
        assert_eq!(graph.evaluate_or(Signal::NumRequestsRunning, 7.0), 7.0);
    }

    #[test]
    fn evaluate_finite_skips_non_finite() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![
                MetricSample::new(f64::NAN),
                MetricSample::new(4.0),
            ]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        assert_eq!(graph.evaluate_finite(Signal::NumRequestsRunning, 1.0), 1.0);
    }

    #[test]
    fn total_requests_sums_components() {
        let snapshot = MetricSeriesSnapshot {
            request_success_total: MetricSeries::scalar(8.0),
            request_error_total: MetricSeries::scalar(1.0),
            request_abort_total: MetricSeries::scalar(1.0),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        assert_eq!(graph.evaluate(Signal::TotalRequests), Some(10.0));
    }

    #[test]
    fn total_requests_none_when_component_missing() {
        let snapshot = MetricSeriesSnapshot {
            request_success_total: MetricSeries::scalar(8.0),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        assert_eq!(graph.evaluate(Signal::TotalRequests), None);
    }

    #[test]
    fn error_rate_zero_when_no_total() {
        let snapshot = MetricSeriesSnapshot::default();
        let graph = SignalGraph::new(&snapshot);
        assert_eq!(graph.evaluate(Signal::ErrorRate), None);
    }

    #[test]
    fn error_rate_computes_ratio() {
        let snapshot = MetricSeriesSnapshot {
            request_success_total: MetricSeries::scalar(90.0),
            request_error_total: MetricSeries::scalar(10.0),
            request_abort_total: MetricSeries::scalar(0.0),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        assert!((graph.evaluate(Signal::ErrorRate).unwrap() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn abort_rate_computes_ratio() {
        let snapshot = MetricSeriesSnapshot {
            request_success_total: MetricSeries::scalar(90.0),
            request_error_total: MetricSeries::scalar(0.0),
            request_abort_total: MetricSeries::scalar(10.0),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        assert!((graph.evaluate(Signal::AbortRate).unwrap() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn replica_running_imbalance_detects_imbalance() {
        let snapshot = imbalanced_snapshot();
        let graph = SignalGraph::new(&snapshot);
        let imbalance = graph.evaluate(Signal::ReplicaRunningImbalance).unwrap();
        assert!((imbalance - 3.0).abs() < 1e-9);
    }

    #[test]
    fn replica_running_imbalance_none_for_balanced() {
        let snapshot = balanced_snapshot();
        let graph = SignalGraph::new(&snapshot);
        assert_eq!(graph.evaluate(Signal::ReplicaRunningImbalance), None);
    }

    #[test]
    fn replica_running_imbalance_none_for_single_replica() {
        let snapshot =
            snapshot_with_running(vec![sample(5.0, &[("pod", "a"), ("model_name", "llama")])]);
        let graph = SignalGraph::new(&snapshot);
        assert_eq!(graph.evaluate(Signal::ReplicaRunningImbalance), None);
    }

    #[test]
    fn replica_running_imbalance_zero_low_path() {
        let snapshot = zero_low_snapshot();
        let graph = SignalGraph::new(&snapshot);
        let imbalance = graph.evaluate(Signal::ReplicaRunningImbalance).unwrap();
        assert!((imbalance - 10.0).abs() < 1e-9);
    }

    #[test]
    fn replica_label_detected_from_pod() {
        let snapshot = imbalanced_snapshot();
        let graph = SignalGraph::new(&snapshot);
        assert_eq!(graph.replica_label(), Some("pod"));
    }

    #[test]
    fn replica_label_none_for_single_replica() {
        let snapshot = snapshot_with_running(vec![sample(1.0, &[("pod", "a")])]);
        let graph = SignalGraph::new(&snapshot);
        assert_eq!(graph.replica_label(), None);
    }

    #[test]
    fn models_collects_model_names() {
        let snapshot = imbalanced_snapshot();
        let graph = SignalGraph::new(&snapshot);
        let models = graph.models();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0], Some("llama".to_string()));
    }

    #[test]
    fn models_returns_none_when_no_labels() {
        let snapshot = snapshot_with_running(vec![MetricSample::new(1.0)]);
        let graph = SignalGraph::new(&snapshot);
        assert_eq!(graph.models(), vec![None]);
    }

    #[test]
    fn per_replica_groups_by_label() {
        let snapshot = imbalanced_snapshot();
        let graph = SignalGraph::new(&snapshot);
        let by_replica = graph.per_replica(Signal::NumRequestsRunning, Some("llama"));
        assert_eq!(by_replica.len(), 2);
        assert!((by_replica["a"] - 10.0).abs() < 1e-9);
        assert!((by_replica["b"] - 30.0).abs() < 1e-9);
    }

    #[test]
    fn per_replica_empty_without_replica_label() {
        let snapshot = snapshot_with_running(vec![
            sample(1.0, &[("model_name", "llama")]),
            sample(2.0, &[("model_name", "llama")]),
        ]);
        let graph = SignalGraph::new(&snapshot);
        assert!(
            graph
                .per_replica(Signal::NumRequestsRunning, Some("llama"))
                .is_empty()
        );
    }

    #[test]
    fn per_replica_empty_for_unsupported_signal() {
        let snapshot = imbalanced_snapshot();
        let graph = SignalGraph::new(&snapshot);
        assert!(
            graph
                .per_replica(Signal::TtftP95Seconds, Some("llama"))
                .is_empty()
        );
    }

    #[test]
    fn per_replica_filters_by_model() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![
                sample(1.0, &[("pod", "a"), ("model_name", "llama")]),
                sample(2.0, &[("pod", "b"), ("model_name", "mistral")]),
            ]),
            num_requests_waiting: MetricSeries::from_samples(vec![
                sample(1.0, &[("pod", "a"), ("model_name", "llama")]),
                sample(2.0, &[("pod", "b"), ("model_name", "mistral")]),
            ]),
            kv_cache_usage_perc: MetricSeries::from_samples(vec![
                sample(0.1, &[("pod", "a"), ("model_name", "llama")]),
                sample(0.2, &[("pod", "b"), ("model_name", "mistral")]),
            ]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let by_replica = graph.per_replica(Signal::NumRequestsRunning, Some("llama"));
        assert_eq!(by_replica.len(), 1);
        assert!((by_replica["a"] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn per_replica_supports_waiting_and_cache() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![
                sample(1.0, &[("pod", "a"), ("model_name", "llama")]),
                sample(2.0, &[("pod", "b"), ("model_name", "llama")]),
            ]),
            num_requests_waiting: MetricSeries::from_samples(vec![sample(
                4.0,
                &[("pod", "a"), ("model_name", "llama")],
            )]),
            kv_cache_usage_perc: MetricSeries::from_samples(vec![sample(
                0.5,
                &[("pod", "a"), ("model_name", "llama")],
            )]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        assert!(
            !graph
                .per_replica(Signal::NumRequestsWaiting, Some("llama"))
                .is_empty()
        );
        assert!(
            !graph
                .per_replica(Signal::KvCacheUsagePerc, Some("llama"))
                .is_empty()
        );
    }

    #[test]
    fn per_replica_skips_non_finite_values() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![
                sample(f64::NAN, &[("pod", "a"), ("model_name", "llama")]),
                sample(2.0, &[("pod", "b"), ("model_name", "llama")]),
            ]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let by_replica = graph.per_replica(Signal::NumRequestsRunning, Some("llama"));
        assert_eq!(by_replica.len(), 1);
        assert!((by_replica["b"] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn snapshot_returns_inner_snapshot() {
        let snapshot = imbalanced_snapshot();
        let graph = SignalGraph::new(&snapshot);
        assert_eq!(graph.snapshot().num_requests_running.value(), Some(40.0));
    }
}
