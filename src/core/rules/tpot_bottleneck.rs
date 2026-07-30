//! TPOT bottleneck rule.
//!
//! Detects when time per output token (p95) exceeds the configured threshold.
//! Confidence rises when generation throughput is also low — corroborating decode
//! pressure — and when TTFT is not elevated, isolating the bottleneck to decode
//! rather than prefill or queue saturation.
use crate::core::config::CoreConfig as Config;
use crate::core::config::TpotBottleneckConfig;
use crate::core::models::{ComparisonOperator, Confidence, DiagnosisState, EvidenceItem, Severity};
use crate::core::rules::Rule;
use crate::core::rules::RuleDefinition;
use crate::core::rules::templates::{FindingTemplate, TemplateContext};
use crate::core::signals::{Signal, SignalGraph};

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
    template: &TpotBottleneckTemplate as &dyn FindingTemplate,
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

        let gen_low_v = gen_low(signals, &self.cfg).is_some();
        let ttft_normal_v = ttft_normal(signals).is_some();
        let signals_count = 1 + gen_low_v as i32 + ttft_normal_v as i32;
        let confidence = if signals_count >= 3 {
            Confidence::High
        } else if signals_count == 2 {
            Confidence::Medium
        } else {
            Confidence::Low
        };
        DiagnosisState::firing(Severity::Warning, confidence, Signal::TpotP95Seconds, tpot)
    }
}

/// Generation throughput is below the configured threshold, corroborating
/// decode pressure.
///
/// Shared by the rule (for confidence) and the template (for evidence).
fn gen_low(graph: &SignalGraph<'_>, cfg: &TpotBottleneckConfig) -> Option<f64> {
    graph
        .evaluate(Signal::GenerationTokensPerSecond)
        .filter(|&v| v.is_finite() && v < cfg.low_gen_tokens_per_sec)
}

/// TTFT is not elevated (finite and below 2.0s), isolating the bottleneck to
/// decode rather than prefill or queue saturation.
///
/// Shared by the rule (for confidence) and the template (for evidence).
fn ttft_normal(graph: &SignalGraph<'_>) -> Option<f64> {
    graph
        .evaluate(Signal::TtftP95Seconds)
        .filter(|&v| v.is_finite() && v < 2.0)
}

pub struct TpotBottleneckTemplate;

impl FindingTemplate for TpotBottleneckTemplate {
    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<EvidenceItem> {
        let tpot = ctx.value;
        let cfg = &ctx.config.rules.tpot_bottleneck;
        let mut items = vec![EvidenceItem::threshold(
            ctx.signal.to_string(),
            tpot,
            cfg.high_tpot_p95,
            Some("s"),
            ComparisonOperator::GreaterThanOrEqual,
        )];
        if let Some(gen_tps) = ctx.graph.evaluate(Signal::GenerationTokensPerSecond) {
            if gen_tps.is_finite() {
                items.push(EvidenceItem::value(
                    Signal::GenerationTokensPerSecond.to_string(),
                    gen_tps,
                    Some("tok/s"),
                ));
            }
        }
        if let Some(ttft) = ctx.graph.evaluate(Signal::TtftP95Seconds) {
            if ttft.is_finite() {
                items.push(EvidenceItem::value(
                    Signal::TtftP95Seconds.to_string(),
                    ttft,
                    Some("s"),
                ));
            }
        }
        items
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
    use crate::core::metrics::MetricSeriesSnapshot;
    use crate::core::metrics::series::{MetricSample, MetricSeries};
    use crate::core::models::DiagnosisState;
    use crate::core::signals::{Signal, SignalGraph};

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
    fn fires_warning_when_tpot_high() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.3, 5.0, 100.0))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Low,
                Signal::TpotP95Seconds,
                0.3
            )
        );
    }

    #[test]
    fn medium_confidence_when_one_secondary_signal() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.3, 5.0, 20.0))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Medium,
                Signal::TpotP95Seconds,
                0.3
            )
        );
    }

    #[test]
    fn high_confidence_when_both_secondary_signals() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.3, 1.0, 20.0))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::High,
                Signal::TpotP95Seconds,
                0.3
            )
        );
    }

    #[test]
    fn template_output() {
        let snap = snapshot(0.4, 1.0, 30.0);
        let graph = SignalGraph::new(&snap);
        let config = Config::default();
        let ctx = TemplateContext {
            graph: &graph,
            config: &config,
            signal: Signal::TpotP95Seconds,
            value: 0.4,
        };
        let t = TpotBottleneckTemplate;
        assert_eq!(
            t.evidence(&ctx)[0].summary(),
            "tpot_p95_seconds: 0.40s ≥ threshold 0.20s"
        );
        let evidence = t.evidence(&ctx);
        assert_eq!(
            evidence[0].summary(),
            "tpot_p95_seconds: 0.40s ≥ threshold 0.20s"
        );
        assert_eq!(
            evidence[1].summary(),
            "generation_tokens_per_second: 30 tok/s"
        );
        assert_eq!(evidence[2].summary(), "ttft_p95_seconds: 1s");
    }
}
