//! TPOT bottleneck rule.
//!
//! Detects when time per output token (p95) exceeds the configured threshold.
//! Confidence rises when generation throughput is also low — corroborating decode
//! pressure — and when TTFT is not elevated, isolating the bottleneck to decode
//! rather than prefill or queue saturation.
use crate::config::Config;
use crate::config::TpotBottleneckConfig;
use crate::models::{DiagnosisState, Severity};
use crate::reports::templates::TpotBottleneckTemplate;
use crate::rules::Rule;
use crate::rules::RuleDefinition;
use crate::signals::{Signal, SignalGraph};

pub static DEFINITION: RuleDefinition = RuleDefinition {
    id: "tpot_bottleneck",
    name: "High TPOT",
    title: "High time per output token (TPOT)",
    severity: Severity::Warning,
    likely_causes: &[
        "GPU memory bandwidth saturated during decode",
        "Too many concurrent sequences reducing per-request throughput",
        "Large model size relative to available GPU memory",
        "Insufficient tensor parallelism for current load",
    ],
    recommendations: &[
        "Reduce max concurrent requests (--max-num-seqs)",
        "Increase tensor parallelism to distribute decode across GPUs",
        "Enable speculative decoding to amortize decode cost",
        "Profile GPU memory bandwidth utilization",
    ],
    related_metrics: &[
        "tpot_p95_seconds",
        "generation_tokens_per_second",
        "ttft_p95_seconds",
    ],
    template: &TpotBottleneckTemplate as &dyn crate::reports::templates::FindingTemplate,
};

pub struct TpotBottleneckRule {
    cfg: TpotBottleneckConfig,
}

impl TpotBottleneckRule {
    pub fn new(cfg: TpotBottleneckConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for TpotBottleneckRule {
    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        let Some(tpot) = signals.evaluate(Signal::TpotP95Seconds) else {
            return DiagnosisState::unknown_signal(Signal::TpotP95Seconds);
        };

        if tpot < self.cfg.high_tpot_p95 {
            return DiagnosisState::Healthy;
        }

        DiagnosisState::Stressed(Signal::TpotP95Seconds, tpot)
    }
}

pub fn factory(config: &Config) -> (&'static RuleDefinition, Box<dyn Rule>) {
    (
        &DEFINITION,
        Box::new(TpotBottleneckRule::new(
            config.rules.tpot_bottleneck.clone(),
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

    fn rule() -> TpotBottleneckRule {
        TpotBottleneckRule::new(TpotBottleneckConfig {
            high_tpot_p95: 0.2,
            low_gen_tokens_per_sec: 50.0,
        })
    }

    fn snapshot(tpot: f64, ttft: f64, gen_tps: f64) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            tpot_p95_seconds: MetricSeries::from_samples(vec![MetricSample::new(tpot)]),
            ttft_p95_seconds: MetricSeries::from_samples(vec![MetricSample::new(ttft)]),
            generation_tokens_per_second: MetricSeries::from_samples(vec![MetricSample::new(
                gen_tps,
            )]),
            ..Default::default()
        }
    }

    #[test]
    fn healthy_when_tpot_low() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.1, 1.0, 100.0))),
            DiagnosisState::Healthy
        );
    }

    #[test]
    fn stressed_when_tpot_high() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.3, 5.0, 100.0))),
            DiagnosisState::Stressed(Signal::TpotP95Seconds, 0.3)
        );
    }
}
