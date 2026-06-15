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
