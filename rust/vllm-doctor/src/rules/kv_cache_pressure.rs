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
use crate::config::KVCachePressureConfig;
use crate::models::{DiagnosisState, Severity};
use crate::rules::Rule;
use crate::signals::{Signal, SignalGraph};

pub struct KVCachePressureRule {
    cfg: KVCachePressureConfig,
}

impl KVCachePressureRule {
    pub fn new(cfg: KVCachePressureConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for KVCachePressureRule {
    fn id(&self) -> &'static str {
        "kv_cache_pressure"
    }

    fn name(&self) -> &'static str {
        "KV Cache Pressure"
    }

    fn title(&self) -> &'static str {
        "KV cache pressure"
    }

    fn severity(&self) -> Severity {
        Severity::Critical
    }

    fn likely_causes(&self) -> &'static [&'static str] {
        &[
            "Long-context requests holding large KV cache allocations",
            "max_num_seqs or max_num_batched_tokens set too high for available GPU memory",
            "Sudden spike in concurrent requests",
        ]
    }

    fn recommendations(&self) -> &'static [&'static str] {
        &[
            "Reduce max_num_seqs to limit concurrent sequences",
            "Reduce max_num_batched_tokens to cap memory per step",
            "Increase gpu_memory_utilization if GPU memory headroom exists",
            "Route long-context requests to a dedicated replica",
        ]
    }

    fn related_metrics(&self) -> &'static [&'static str] {
        &["vllm:kv_cache_usage_perc", "vllm:num_requests_waiting"]
    }

    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        let Some(cache) = signals.evaluate(Signal::KvCacheUsagePerc) else {
            return DiagnosisState::unknown_signal(Signal::KvCacheUsagePerc);
        };

        if cache < self.cfg.high_cache_usage {
            return DiagnosisState::Healthy;
        }

        DiagnosisState::Stressed(Signal::KvCacheUsagePerc, cache)
    }
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
    fn stressed_when_cache_high() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.95, 0.0))),
            DiagnosisState::Stressed(Signal::KvCacheUsagePerc, 0.95)
        );
    }
}
