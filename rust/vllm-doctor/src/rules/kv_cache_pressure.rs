//! KV cache pressure rule.
//!
//! Detects when GPU KV cache is near exhaustion. When the cache fills up, vLLM
//! cannot admit new sequences — requests stall in the waiting queue even if GPU
//! compute is otherwise available. This is the most common cause of latency spikes
//! under long-context or high-concurrency workloads.
//!
//! Signals (each matching signal increases confidence):
//!   - kv_cache_usage_perc >= threshold: cache is critically full
//!   - num_requests_waiting > 0: cache pressure is already causing queuing
//!
//! Confidence:
//!   cache signal only  → medium (pressure exists, queuing not yet observed)
//!   both signals       → high   (cache is full and actively blocking requests)
use crate::config::Config;
use crate::config::KVCachePressureConfig;
use crate::models::{Confidence, DiagnosisState, Severity};
use crate::reports::templates::KvCachePressureTemplate;
use crate::rules::Rule;
use crate::rules::RuleDefinition;
use crate::signals::{Signal, SignalGraph};

pub static DEFINITION: RuleDefinition = RuleDefinition {
    id: "kv_cache_pressure",
    name: "KV Cache Pressure",
    title: "KV cache pressure",
    severity: Severity::Critical,
    likely_causes: &[
        "Long-context requests holding large KV cache allocations",
        "max_num_seqs or max_num_batched_tokens set too high for available GPU memory",
        "Sudden spike in concurrent requests",
    ],
    recommendations: &[
        "Reduce max_num_seqs to limit concurrent sequences",
        "Reduce max_num_batched_tokens to cap memory per step",
        "Increase gpu_memory_utilization if GPU memory headroom exists",
        "Route long-context requests to a dedicated replica",
    ],
    related_metrics: &["vllm:kv_cache_usage_perc", "vllm:num_requests_waiting"],
    template: &KvCachePressureTemplate as &dyn crate::reports::templates::FindingTemplate,
};

pub struct KVCachePressureRule {
    cfg: KVCachePressureConfig,
}

impl KVCachePressureRule {
    pub fn new(cfg: KVCachePressureConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for KVCachePressureRule {
    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        let Some(cache) = signals.evaluate(Signal::KvCacheUsagePerc) else {
            return DiagnosisState::unknown_signal(Signal::KvCacheUsagePerc);
        };

        if cache < self.cfg.high_cache_usage {
            return DiagnosisState::Healthy;
        }

        // Cache exhaustion is always critical; confidence rises to high once it is
        // actively blocking admission (requests waiting).
        let confidence = if waiting_backlog(signals).is_some() {
            Confidence::High
        } else {
            Confidence::Medium
        };
        DiagnosisState::firing(
            Severity::Critical,
            confidence,
            Signal::KvCacheUsagePerc,
            cache,
        )
    }
}

/// Active backlog confirmed: requests are waiting, blocked by full cache.
///
/// Shared by the rule (for confidence) and the template (for evidence).
pub(crate) fn waiting_backlog(graph: &SignalGraph<'_>) -> Option<f64> {
    graph
        .evaluate(Signal::NumRequestsWaiting)
        .filter(|&w| w > 0.0)
}

pub fn factory(config: &Config) -> (&'static RuleDefinition, Box<dyn Rule>) {
    (
        &DEFINITION,
        Box::new(KVCachePressureRule::new(
            config.rules.kv_cache_pressure.clone(),
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::DiagnosisState;
    use crate::signals::{Signal, SignalGraph};

    fn rule() -> KVCachePressureRule {
        KVCachePressureRule::new(KVCachePressureConfig {
            high_cache_usage: 0.90,
        })
    }

    fn snapshot(cache: f64, waiting: f64) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            kv_cache_usage_perc: MetricSeries::from_samples(vec![MetricSample::new(cache)]),
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(waiting)]),
            ..Default::default()
        }
    }

    #[test]
    fn healthy_when_cache_usage_low() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.5, 5.0))),
            DiagnosisState::Healthy
        );
    }

    #[test]
    fn critical_medium_when_cache_high_no_waiting() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.95, 0.0))),
            DiagnosisState::firing(
                Severity::Critical,
                Confidence::Medium,
                Signal::KvCacheUsagePerc,
                0.95
            )
        );
    }

    #[test]
    fn critical_high_when_cache_high_and_waiting() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.95, 7.0))),
            DiagnosisState::firing(
                Severity::Critical,
                Confidence::High,
                Signal::KvCacheUsagePerc,
                0.95
            )
        );
    }
}
