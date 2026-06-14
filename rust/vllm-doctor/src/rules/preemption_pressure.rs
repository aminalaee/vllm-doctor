//! Preemption pressure rule.
//!
//! Detects when vLLM has preempted sequences due to KV cache exhaustion.
//!
//! Preemption happens when a running sequence must be evicted from GPU KV cache to free
//! space for another. The evicted sequence is re-computed later, wasting GPU cycles and
//! adding latency. Any preemptions indicate the server ran out of KV cache at least once.
//!
//! Signals (each matching signal increases confidence):
//!   - num_preemptions_total > 0: preemptions have occurred
//!   - kv_cache_usage_perc >= threshold: cache is currently under pressure
//!
//! Confidence:
//!   preemptions only               → medium (happened at some point, may not be ongoing)
//!   preemptions + high cache usage → high   (actively under memory pressure)
use crate::config::PreemptionPressureConfig;
use crate::metrics::MetricSeriesSnapshot;
use crate::models::{Confidence, FindingData};
use crate::rules::Rule;

pub struct PreemptionPressureRule {
    cfg: PreemptionPressureConfig,
}

impl PreemptionPressureRule {
    pub fn new(cfg: PreemptionPressureConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for PreemptionPressureRule {
    fn id(&self) -> &'static str {
        "preemption_pressure"
    }

    fn name(&self) -> &'static str {
        "Preemption Pressure"
    }

    fn title(&self) -> &'static str {
        "Preemption pressure"
    }

    fn severity(&self) -> crate::models::Severity {
        crate::models::Severity::Warning
    }

    fn likely_causes(&self) -> &'static [&'static str] {
        &[
            "KV cache too small for the concurrent request mix",
            "Long-context requests exhausting cache before shorter ones complete",
            "max_num_seqs set too high relative to available GPU memory",
        ]
    }

    fn recommendations(&self) -> &'static [&'static str] {
        &[
            "Reduce max_num_seqs to limit concurrent sequences in GPU memory",
            "Reduce max_num_batched_tokens to lower per-step memory pressure",
            "Increase gpu_memory_utilization if GPU headroom exists",
            "Route long-context requests to a dedicated replica",
        ]
    }

    fn related_metrics(&self) -> &'static [&'static str] {
        &["vllm:num_preemptions_total", "vllm:kv_cache_usage_perc"]
    }

    fn run(&self, metrics: &MetricSeriesSnapshot) -> Option<FindingData> {
        let preemptions = metrics.num_preemptions_total.value()?;
        if preemptions == 0.0 {
            return None;
        }

        let mut evidence = vec![format!("Preemptions total: {preemptions:.0}")];
        let mut signals = Vec::new();

        let cache = metrics.kv_cache_usage_perc.value();
        let cache_high = cache.is_some_and(|v| v >= self.cfg.high_cache_usage);
        if cache_high {
            signals.push("KV cache under pressure while preemptions are occurring".to_string());
            evidence.push(format!(
                "GPU KV cache usage: {:.0}% (threshold: {:.0}%)",
                cache.unwrap() * 100.0,
                self.cfg.high_cache_usage * 100.0
            ));
        }

        Some(FindingData {
            confidence: if cache_high {
                Confidence::High
            } else {
                Confidence::Medium
            },
            summary: format!(
                "vLLM has preempted {preemptions:.0} sequences — KV cache exhaustion is forcing sequences to be re-computed."
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

    fn rule() -> PreemptionPressureRule {
        PreemptionPressureRule::new(PreemptionPressureConfig {
            high_cache_usage: 0.80,
        })
    }

    fn snapshot(preemptions: f64, cache: f64) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_preemptions_total: MetricSeries::from_samples(vec![MetricSample::new(preemptions)]),
            kv_cache_usage_perc: MetricSeries::from_samples(vec![MetricSample::new(cache)]),
            ..Default::default()
        }
    }

    #[test]
    fn no_finding_when_no_preemptions() {
        assert!(rule().run(&snapshot(0.0, 0.9)).is_none());
    }

    #[test]
    fn medium_confidence_when_preemptions_but_cache_low() {
        let finding = rule().run(&snapshot(5.0, 0.5)).unwrap();
        assert_eq!(finding.confidence, Confidence::Medium);
    }

    #[test]
    fn high_confidence_when_preemptions_and_cache_high() {
        let finding = rule().run(&snapshot(5.0, 0.9)).unwrap();
        assert_eq!(finding.confidence, Confidence::High);
    }
}
