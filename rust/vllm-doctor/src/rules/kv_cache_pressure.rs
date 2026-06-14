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
use crate::metrics::MetricSeriesSnapshot;
use crate::models::{Confidence, FindingData};
use crate::rules::Rule;

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

    fn severity(&self) -> crate::models::Severity {
        crate::models::Severity::Critical
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

    fn run(&self, metrics: &MetricSeriesSnapshot) -> Option<FindingData> {
        let cache = metrics.kv_cache_usage_perc.value()?;
        if cache < self.cfg.high_cache_usage {
            return None;
        }

        let waiting = metrics.num_requests_waiting.value();
        let waiting_high = waiting.is_some_and(|v| v > 0.0);

        let mut signals = Vec::new();
        let mut evidence = vec![format!(
            "GPU KV cache usage: {:.0}% (threshold: {:.0}%)",
            cache * 100.0,
            self.cfg.high_cache_usage * 100.0
        )];
        if waiting_high {
            signals.push("Cache saturation blocking new request admission".to_string());
            evidence.push(format!(
                "Waiting requests: {:.0} (blocked by full cache)",
                waiting.unwrap()
            ));
        }

        Some(FindingData {
            confidence: if waiting_high {
                Confidence::High
            } else {
                Confidence::Medium
            },
            summary: format!(
                "GPU KV cache at {:.0}% — new requests cannot be admitted until sequences complete.",
                cache * 100.0
            ),
            signals,
            evidence,
            severity: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::series::{MetricSample, MetricSeries};

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
    fn no_finding_when_cache_usage_low() {
        assert!(rule().run(&snapshot(0.5, 5.0)).is_none());
    }

    #[test]
    fn medium_confidence_when_cache_high_but_no_waiting() {
        let finding = rule().run(&snapshot(0.95, 0.0)).unwrap();
        assert_eq!(finding.confidence, Confidence::Medium);
    }

    #[test]
    fn high_confidence_when_cache_high_and_waiting() {
        let finding = rule().run(&snapshot(0.95, 5.0)).unwrap();
        assert_eq!(finding.confidence, Confidence::High);
        assert_eq!(rule().severity(), crate::models::Severity::Critical);
    }
}
