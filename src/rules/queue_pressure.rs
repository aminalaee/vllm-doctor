//! Queue pressure rule.
//!
//! Detects when requests are accumulating faster than the server can process them.
//!
//! Signals (each matching signal increases confidence):
//!   - num_requests_waiting > threshold: requests are queued, server is backlogged
//!   - num_requests_running at high concurrency: server is saturated, not idle
//!
//! Confidence:
//!   1 signal → low
//!   2 signals → high
use crate::config::Config;
use crate::config::QueuePressureConfig;
use crate::models::{Confidence, DiagnosisState, Severity};
use crate::rules::Rule;
use crate::rules::RuleDefinition;
use crate::rules::templates::{FindingTemplate, TemplateContext};
use crate::signals::{Signal, SignalGraph};

pub static DEFINITION: RuleDefinition = RuleDefinition {
    id: "queue_pressure",
    name: "Queue Pressure",
    title: "Queue pressure",
    severity: Severity::Warning,
    likely_causes: &[
        "Insufficient replica capacity for current traffic",
        "Autoscaling has not reacted yet",
        "Long-context requests consuming disproportionate compute",
    ],
    recommendations: &[
        "Add replicas or increase concurrency limits",
        "Inspect autoscaling thresholds",
        "Separate long-context traffic to a dedicated replica",
        "Reduce incoming request rate",
    ],
    related_metrics: &["vllm:num_requests_waiting", "vllm:num_requests_running"],
    template: &QueuePressureTemplate as &dyn FindingTemplate,
};

pub struct QueuePressureRule {
    cfg: QueuePressureConfig,
}

impl QueuePressureRule {
    pub fn new(cfg: QueuePressureConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for QueuePressureRule {
    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        let Some(waiting) = signals.evaluate(Signal::NumRequestsWaiting) else {
            return DiagnosisState::unknown_signal(Signal::NumRequestsWaiting);
        };

        if waiting <= self.cfg.high_waiting as f64 {
            return DiagnosisState::Healthy;
        }

        let confidence = if running_high(signals, &self.cfg).is_some() {
            Confidence::High
        } else {
            Confidence::Low
        };
        DiagnosisState::firing(
            Severity::Warning,
            confidence,
            Signal::NumRequestsWaiting,
            waiting,
        )
    }
}

/// Running concurrency exceeds the high-running threshold.
///
/// Shared by the rule (for confidence) and the template (for evidence) so the
/// threshold comparison lives in one place.
fn running_high(graph: &SignalGraph<'_>, cfg: &QueuePressureConfig) -> Option<f64> {
    graph
        .evaluate(Signal::NumRequestsRunning)
        .filter(|&r| r > cfg.high_running as f64)
}

pub struct QueuePressureTemplate;

impl FindingTemplate for QueuePressureTemplate {
    fn summary(&self, _ctx: &TemplateContext<'_>) -> String {
        "Requests are queuing faster than the server can process them.".into()
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let waiting = ctx.value;
        let cfg = &ctx.config.rules.queue_pressure;
        let mut lines = vec![format!(
            "Waiting requests: {waiting:.0} (threshold: {high_waiting})",
            high_waiting = cfg.high_waiting,
        )];
        if let Some(running) = running_high(ctx.graph, cfg) {
            lines.push(format!(
                "Running requests: {running:.0} (threshold: {high_running})",
                high_running = cfg.high_running,
            ));
        }
        lines
    }
}

pub fn factory(config: &Config) -> (&'static RuleDefinition, Box<dyn Rule>) {
    (
        &DEFINITION,
        Box::new(QueuePressureRule::new(config.rules.queue_pressure.clone())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::DiagnosisState;
    use crate::signals::{Signal, SignalGraph};

    fn rule() -> QueuePressureRule {
        QueuePressureRule::new(QueuePressureConfig {
            high_waiting: 5,
            high_running: 50,
        })
    }

    fn snapshot(waiting: f64, running: f64) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(waiting)]),
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(running)]),
            ..Default::default()
        }
    }

    #[test]
    fn healthy_when_waiting_low() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(3.0, 60.0))),
            DiagnosisState::Healthy
        );
    }

    #[test]
    fn fires_warning_when_waiting_high() {
        // running=10 < high_running=50 → running_high=false → Low confidence
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(10.0, 10.0))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Low,
                Signal::NumRequestsWaiting,
                10.0
            )
        );
    }

    #[test]
    fn high_confidence_when_running_high() {
        // running=60 > high_running=50 → running_high=true → High confidence
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(10.0, 60.0))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::High,
                Signal::NumRequestsWaiting,
                10.0
            )
        );
    }

    fn ctx<'a>(graph: &'a SignalGraph<'a>, config: &'a Config, value: f64) -> TemplateContext<'a> {
        TemplateContext {
            graph,
            config,
            signal: Signal::NumRequestsWaiting,
            value,
        }
    }

    #[test]
    fn template_output() {
        let snap = snapshot(8.0, 60.0);
        let graph = SignalGraph::new(&snap);
        let config = Config::default();
        let t = QueuePressureTemplate;
        assert_eq!(
            t.summary(&ctx(&graph, &config, 8.0)),
            "Requests are queuing faster than the server can process them."
        );
        let evidence = t.evidence(&ctx(&graph, &config, 8.0));
        assert_eq!(evidence[0], "Waiting requests: 8 (threshold: 5)");
        assert_eq!(evidence[1], "Running requests: 60 (threshold: 50)");
    }

    #[test]
    fn template_no_running_line_when_below_threshold() {
        let snap = snapshot(8.0, 10.0);
        let graph = SignalGraph::new(&snap);
        let config = Config::default();
        let evidence = QueuePressureTemplate.evidence(&ctx(&graph, &config, 8.0));
        assert_eq!(evidence, vec!["Waiting requests: 8 (threshold: 5)"]);
    }
}
