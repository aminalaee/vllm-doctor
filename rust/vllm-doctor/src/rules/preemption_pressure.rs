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
use crate::config::Config;
use crate::config::PreemptionPressureConfig;
use crate::models::{DiagnosisState, Severity};
use crate::rules::Rule;
use crate::rules::RuleDefinition;
use crate::signals::{Signal, SignalGraph};

pub static DEFINITION: RuleDefinition = RuleDefinition {
    id: "preemption_pressure",
    name: "Preemption Pressure",
    title: "Preemption pressure",
    severity: Severity::Warning,
    likely_causes: &[
        "KV cache too small for the concurrent request mix",
        "Long-context requests exhausting cache before shorter ones complete",
        "max_num_seqs set too high relative to available GPU memory",
    ],
    recommendations: &[
        "Reduce max_num_seqs to limit concurrent sequences in GPU memory",
        "Reduce max_num_batched_tokens to lower per-step memory pressure",
        "Increase gpu_memory_utilization if GPU headroom exists",
        "Route long-context requests to a dedicated replica",
    ],
    related_metrics: &["vllm:num_preemptions_total", "vllm:kv_cache_usage_perc"],
};

pub struct PreemptionPressureRule {
    cfg: PreemptionPressureConfig,
}

impl PreemptionPressureRule {
    pub fn new(cfg: PreemptionPressureConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for PreemptionPressureRule {
    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        let Some(preemptions) = signals.evaluate(Signal::NumPreemptionsTotal) else {
            return DiagnosisState::unknown_signal(Signal::NumPreemptionsTotal);
        };

        if preemptions == 0.0 {
            return DiagnosisState::Healthy;
        }

        DiagnosisState::Stressed(Signal::NumPreemptionsTotal, preemptions)
    }
}

pub fn factory(config: &Config) -> (&'static RuleDefinition, Box<dyn Rule>) {
    (
        &DEFINITION,
        Box::new(PreemptionPressureRule::new(
            config.rules.preemption_pressure.clone(),
        )),
    )
}

impl PreemptionPressureRule {
    #[allow(dead_code)]
    fn high_cache_usage(&self) -> f64 {
        self.cfg.high_cache_usage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::DiagnosisState;
    use crate::signals::{Signal, SignalGraph};

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
    fn healthy_when_no_preemptions() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.0, 0.9))),
            DiagnosisState::Healthy
        );
    }

    #[test]
    fn stressed_when_preemptions_present() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(5.0, 0.5))),
            DiagnosisState::Stressed(Signal::NumPreemptionsTotal, 5.0)
        );
    }
}
