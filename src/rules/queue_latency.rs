//! Queue latency rule.
//!
//! Detects when requests are spending too long in the waiting queue before prefill begins.
//!
//! Unlike QueuePressureRule (which counts waiting requests), this rule measures actual
//! queue latency from the histogram — a direct signal of how long clients wait before
//! their request even starts processing.
//!
//! Signals (each matching signal increases confidence):
//!   - queue_time_p95_seconds >= threshold: requests are spending too long queued
//!   - num_requests_waiting > 0: active backlog confirmed
//!
//! Confidence:
//!   high queue time only      → low  (spike may be transient)
//!   high queue time + waiting → high (active backlog confirmed)
use crate::config::Config;
use crate::config::QueueLatencyConfig;
use crate::models::{Confidence, DiagnosisState, Severity};
use crate::rules::Rule;
use crate::rules::RuleDefinition;
use crate::rules::templates::QueueLatencyTemplate;
use crate::signals::{Signal, SignalGraph};

pub static DEFINITION: RuleDefinition = RuleDefinition {
    id: "queue_latency",
    name: "Queue Latency",
    title: "High queue latency",
    severity: Severity::Warning,
    likely_causes: &[
        "Insufficient replica capacity for current request rate",
        "Long-context requests blocking admission of new sequences",
        "Autoscaling has not reacted to traffic increase",
        "KV cache exhaustion limiting sequence admission",
    ],
    recommendations: &[
        "Add replicas or increase concurrency limits",
        "Inspect autoscaling thresholds and reaction time",
        "Correlate with KV cache pressure — reduce max_num_seqs if cache is full",
        "Separate long-context traffic to a dedicated replica",
    ],
    related_metrics: &[
        "vllm:request_queue_time_seconds",
        "vllm:num_requests_waiting",
    ],
    template: &QueueLatencyTemplate as &dyn crate::rules::templates::FindingTemplate,
};

pub struct QueueLatencyRule {
    cfg: QueueLatencyConfig,
}

impl QueueLatencyRule {
    pub fn new(cfg: QueueLatencyConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for QueueLatencyRule {
    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        let Some(queue_time) = signals.evaluate(Signal::QueueTimeP95Seconds) else {
            return DiagnosisState::unknown_signal(Signal::QueueTimeP95Seconds);
        };

        if queue_time < self.cfg.high_queue_time_p95 {
            return DiagnosisState::Healthy;
        }

        let confidence = if waiting_backlog(signals).is_some() {
            Confidence::High
        } else {
            Confidence::Low
        };
        DiagnosisState::firing(
            Severity::Warning,
            confidence,
            Signal::QueueTimeP95Seconds,
            queue_time,
        )
    }
}

/// Active backlog confirmed: requests are waiting in the queue.
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
        Box::new(QueueLatencyRule::new(config.rules.queue_latency.clone())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::DiagnosisState;
    use crate::signals::{Signal, SignalGraph};

    fn rule() -> QueueLatencyRule {
        QueueLatencyRule::new(QueueLatencyConfig {
            high_queue_time_p95: 1.0,
        })
    }

    fn snapshot(queue_time: f64, waiting: f64) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            queue_time_p95_seconds: MetricSeries::from_samples(vec![MetricSample::new(queue_time)]),
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(waiting)]),
            ..Default::default()
        }
    }

    #[test]
    fn healthy_when_queue_time_low() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.5, 5.0))),
            DiagnosisState::Healthy
        );
    }

    #[test]
    fn fires_warning_when_queue_time_high() {
        // waiting=3.0 > 0 → waiting_confirmed=true → High confidence
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(2.0, 3.0))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::High,
                Signal::QueueTimeP95Seconds,
                2.0
            )
        );
    }

    #[test]
    fn low_confidence_when_no_waiting() {
        // waiting=0.0 → waiting_confirmed=false → Low confidence
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(2.0, 0.0))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Low,
                Signal::QueueTimeP95Seconds,
                2.0
            )
        );
    }
}
