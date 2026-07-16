//! TTFT bottleneck rule.
//!
//! Detects when time to first token (p95) exceeds the configured threshold.
//! Confidence rises when TPOT is healthy — ruling out a general decode bottleneck
//! — and when requests are queuing, confirming prefill pressure.
use crate::config::Config;
use crate::config::TtftBottleneckConfig;
use crate::models::{ComparisonOperator, Confidence, DiagnosisState, EvidenceItem, Severity};
use crate::rules::Rule;
use crate::rules::RuleDefinition;
use crate::rules::templates::{FindingTemplate, TemplateContext};
use crate::signals::{Signal, SignalGraph};

pub static DEFINITION: RuleDefinition = RuleDefinition {
    id: "ttft_bottleneck",
    name: "High TTFT",
    title: "High time to first token (TTFT)",
    severity: Severity::Warning,
    likely_causes: &[
        "Long input prompts increasing prefill time",
        "Queue pressure delaying prefill start",
        "Chunked prefill not enabled or misconfigured",
        "Insufficient capacity for current prompt load",
    ],
    recommendations: &[
        "Enable or tune chunked prefill (--enable-chunked-prefill)",
        "Reduce max prompt length or filter long requests",
        "Inspect queue depth — consider adding replicas",
        "Separate long-context traffic to dedicated instances",
    ],
    related_metrics: &[
        "ttft_p95_seconds",
        "num_requests_waiting",
        "tpot_p95_seconds",
    ],
    template: &TtftBottleneckTemplate as &dyn FindingTemplate,
};

pub struct TtftBottleneckRule {
    cfg: TtftBottleneckConfig,
}

impl TtftBottleneckRule {
    pub fn new(cfg: TtftBottleneckConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for TtftBottleneckRule {
    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        let Some(ttft) = signals.evaluate(Signal::TtftP95Seconds) else {
            return DiagnosisState::unknown_signal(Signal::TtftP95Seconds);
        };

        if ttft < self.cfg.high_ttft_p95 {
            return DiagnosisState::Healthy;
        }

        let tpot_stable = tpot_stable(signals, &self.cfg).is_some();
        let waiting_confirmed = waiting_backlog(signals).is_some();
        let signals_count = 1 + tpot_stable as i32 + waiting_confirmed as i32;
        let confidence = if signals_count >= 3 {
            Confidence::High
        } else if signals_count == 2 {
            Confidence::Medium
        } else {
            Confidence::Low
        };
        DiagnosisState::firing(Severity::Warning, confidence, Signal::TtftP95Seconds, ttft)
    }
}

/// TPOT is healthy (finite and below the high-TPOT threshold), ruling out a
/// general decode bottleneck.
///
/// Shared by the rule (for confidence) and the template (for evidence).
fn tpot_stable(graph: &SignalGraph<'_>, cfg: &TtftBottleneckConfig) -> Option<f64> {
    graph
        .evaluate(Signal::TpotP95Seconds)
        .filter(|&v| v.is_finite() && v < cfg.high_tpot_p95)
}

/// Active backlog confirmed: requests are waiting in the queue.
///
/// Shared by the rule (for confidence) and the template (for evidence).
fn waiting_backlog(graph: &SignalGraph<'_>) -> Option<f64> {
    graph
        .evaluate(Signal::NumRequestsWaiting)
        .filter(|&w| w > 0.0)
}

pub struct TtftBottleneckTemplate;

impl FindingTemplate for TtftBottleneckTemplate {
    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<EvidenceItem> {
        let ttft = ctx.value;
        let cfg = &ctx.config.rules.ttft_bottleneck;
        let mut items = vec![EvidenceItem::threshold(
            ctx.signal.to_string(),
            ttft,
            cfg.high_ttft_p95,
            Some("s"),
            ComparisonOperator::GreaterThanOrEqual,
        )];
        if let Some(tpot) = ctx.graph.evaluate(Signal::TpotP95Seconds) {
            if tpot.is_finite() {
                items.push(EvidenceItem::value(
                    Signal::TpotP95Seconds.to_string(),
                    tpot,
                    Some("s"),
                ));
            }
        }
        if let Some(waiting) = ctx.graph.evaluate(Signal::NumRequestsWaiting) {
            if waiting > 0.0 {
                items.push(EvidenceItem::value(
                    Signal::NumRequestsWaiting.to_string(),
                    waiting,
                    None::<String>,
                ));
            }
        }
        items
    }
}

pub fn factory(config: &Config) -> (&'static RuleDefinition, Box<dyn Rule>) {
    (
        &DEFINITION,
        Box::new(TtftBottleneckRule::new(
            config.rules.ttft_bottleneck.clone(),
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

    fn rule() -> TtftBottleneckRule {
        TtftBottleneckRule::new(TtftBottleneckConfig {
            high_ttft_p95: 2.0,
            high_tpot_p95: 0.2,
        })
    }

    fn snapshot(ttft: f64, tpot: f64, waiting: f64) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            ttft_p95_seconds: MetricSeries::from_samples(vec![MetricSample::new(ttft)]),
            tpot_p95_seconds: MetricSeries::from_samples(vec![MetricSample::new(tpot)]),
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(waiting)]),
            ..Default::default()
        }
    }

    #[test]
    fn healthy_when_ttft_low() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(1.0, 0.1, 5.0))),
            DiagnosisState::Healthy
        );
    }

    #[test]
    fn fires_warning_when_ttft_high() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(3.0, 0.5, 0.0))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Low,
                Signal::TtftP95Seconds,
                3.0
            )
        );
    }

    #[test]
    fn medium_confidence_when_one_secondary_signal() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(3.0, 0.1, 0.0))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Medium,
                Signal::TtftP95Seconds,
                3.0
            )
        );
    }

    #[test]
    fn high_confidence_when_both_secondary_signals() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(3.0, 0.1, 5.0))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::High,
                Signal::TtftP95Seconds,
                3.0
            )
        );
    }

    #[test]
    fn template_output() {
        let snap = snapshot(2.5, 0.1, 3.0);
        let graph = SignalGraph::new(&snap);
        let config = Config::default();
        let ctx = TemplateContext {
            graph: &graph,
            config: &config,
            signal: Signal::TtftP95Seconds,
            value: 2.5,
        };
        let t = TtftBottleneckTemplate;
        assert_eq!(
            t.evidence(&ctx)[0].summary(),
            "ttft_p95_seconds: 2.50s ≥ threshold 2s"
        );
        let evidence = t.evidence(&ctx);
        assert_eq!(
            evidence[0].summary(),
            "ttft_p95_seconds: 2.50s ≥ threshold 2s"
        );
        assert_eq!(evidence[1].summary(), "tpot_p95_seconds: 0.10s");
        assert_eq!(evidence[2].summary(), "num_requests_waiting: 3");
    }
}
