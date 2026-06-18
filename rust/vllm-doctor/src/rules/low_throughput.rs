//! Low throughput rule.
//!
//! Detects when the server is processing requests below expected throughput with
//! no queue pressure. This indicates the server is underutilized — not saturated —
//! which points to low incoming load, poor batching, or misconfigured concurrency.
//!
//! Signals (each matching signal increases confidence):
//!   - prompt_tokens_per_second below threshold: prefill throughput is low
//!   - generation_tokens_per_second below threshold: decode throughput is low
//!   - num_requests_running very low: few active requests, no batching benefit
//!
//! Suppressed when requests are waiting — low throughput with a queue is a
//! capacity problem (queue pressure), not an underutilization problem.
//!
//! Confidence:
//!   both prompt and gen low, or running very low  → medium
//!   only one metric low                           → low
use crate::config::Config;
use crate::config::LowThroughputConfig;
use crate::models::{DiagnosisState, Severity};
use crate::reports::templates::LowThroughputTemplate;
use crate::rules::Rule;
use crate::rules::RuleDefinition;
use crate::signals::{Signal, SignalGraph};

pub static DEFINITION: RuleDefinition = RuleDefinition {
    id: "low_throughput",
    name: "Low Throughput",
    title: "Low throughput",
    severity: Severity::Warning,
    likely_causes: &[
        "Low incoming request rate — server is idle",
        "Poor batching due to few concurrent requests",
        "Suboptimal max_num_seqs or max_num_batched_tokens for current load",
    ],
    recommendations: &[
        "Increase concurrent requests to improve batching efficiency",
        "Review max_num_seqs and max_num_batched_tokens settings",
        "Compare against benchmark baseline to confirm underperformance",
        "Consider consolidating replicas if load is consistently low",
    ],
    related_metrics: &[
        "vllm:prompt_tokens_per_second",
        "vllm:generation_tokens_per_second",
        "vllm:num_requests_running",
    ],
    template: &LowThroughputTemplate as &dyn crate::reports::templates::FindingTemplate,
};

pub struct LowThroughputRule {
    cfg: LowThroughputConfig,
}

impl LowThroughputRule {
    pub fn new(cfg: LowThroughputConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for LowThroughputRule {
    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        let prompt = signals.evaluate(Signal::PromptTokensPerSecond);
        let gen_tps = signals.evaluate(Signal::GenerationTokensPerSecond);
        if prompt.is_none() && gen_tps.is_none() {
            return DiagnosisState::unknown_signal(Signal::PromptTokensPerSecond);
        }

        let prompt_low = prompt.is_some_and(|v| v < self.cfg.low_prompt_tps);
        let gen_low = gen_tps.is_some_and(|v| v < self.cfg.low_gen_tps);

        if !prompt_low && !gen_low {
            return DiagnosisState::Healthy;
        }

        let waiting = signals.evaluate(Signal::NumRequestsWaiting).unwrap_or(0.0);
        if waiting > 0.0 {
            return DiagnosisState::Healthy;
        }

        if prompt_low {
            DiagnosisState::Stressed(Signal::PromptTokensPerSecond, prompt.unwrap_or(0.0))
        } else {
            DiagnosisState::Stressed(Signal::GenerationTokensPerSecond, gen_tps.unwrap_or(0.0))
        }
    }
}

pub fn factory(config: &Config) -> (&'static RuleDefinition, Box<dyn Rule>) {
    (
        &DEFINITION,
        Box::new(LowThroughputRule::new(config.rules.low_throughput.clone())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::DiagnosisState;
    use crate::signals::{Signal, SignalGraph};

    fn rule() -> LowThroughputRule {
        LowThroughputRule::new(LowThroughputConfig {
            low_prompt_tps: 10.0,
            low_gen_tps: 50.0,
            low_running: 2,
        })
    }

    fn snapshot(prompt: f64, gen_tps: f64, running: f64, waiting: f64) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            prompt_tokens_per_second: MetricSeries::from_samples(vec![MetricSample::new(prompt)]),
            generation_tokens_per_second: MetricSeries::from_samples(vec![MetricSample::new(
                gen_tps,
            )]),
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(running)]),
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(waiting)]),
            ..Default::default()
        }
    }

    #[test]
    fn healthy_when_throughput_high() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(100.0, 100.0, 5.0, 0.0))),
            DiagnosisState::Healthy
        );
    }

    #[test]
    fn healthy_when_waiting_exists() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(5.0, 5.0, 5.0, 1.0))),
            DiagnosisState::Healthy
        );
    }

    #[test]
    fn stressed_when_prompt_low() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(5.0, 100.0, 5.0, 0.0))),
            DiagnosisState::Stressed(Signal::PromptTokensPerSecond, 5.0)
        );
    }

    #[test]
    fn stressed_when_gen_low() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(100.0, 20.0, 5.0, 0.0))),
            DiagnosisState::Stressed(Signal::GenerationTokensPerSecond, 20.0)
        );
    }

    #[test]
    fn stressed_when_both_low() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(5.0, 20.0, 5.0, 0.0))),
            DiagnosisState::Stressed(Signal::PromptTokensPerSecond, 5.0)
        );
    }
}
