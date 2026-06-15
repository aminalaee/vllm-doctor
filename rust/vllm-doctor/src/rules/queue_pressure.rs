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
use crate::config::QueuePressureConfig;
use crate::models::{DiagnosisState, Severity};
use crate::rules::Rule;
use crate::signals::{Signal, SignalGraph};

pub struct QueuePressureRule {
    cfg: QueuePressureConfig,
}

impl QueuePressureRule {
    pub fn new(cfg: QueuePressureConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for QueuePressureRule {
    fn id(&self) -> &'static str {
        "queue_pressure"
    }

    fn name(&self) -> &'static str {
        "Queue Pressure"
    }

    fn title(&self) -> &'static str {
        "Queue pressure"
    }

    fn severity(&self) -> Severity {
        Severity::Warning
    }

    fn likely_causes(&self) -> &'static [&'static str] {
        &[
            "Insufficient replica capacity for current traffic",
            "Autoscaling has not reacted yet",
            "Long-context requests consuming disproportionate compute",
        ]
    }

    fn recommendations(&self) -> &'static [&'static str] {
        &[
            "Add replicas or increase concurrency limits",
            "Inspect autoscaling thresholds",
            "Separate long-context traffic to a dedicated replica",
            "Reduce incoming request rate",
        ]
    }

    fn related_metrics(&self) -> &'static [&'static str] {
        &["vllm:num_requests_waiting", "vllm:num_requests_running"]
    }

    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        let Some(waiting) = signals.evaluate(Signal::NumRequestsWaiting) else {
            return DiagnosisState::unknown_signal(Signal::NumRequestsWaiting);
        };

        if waiting <= self.cfg.high_waiting as f64 {
            return DiagnosisState::Healthy;
        }

        DiagnosisState::Stressed(Signal::NumRequestsWaiting, waiting)
    }
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
    fn stressed_when_waiting_high() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(10.0, 10.0))),
            DiagnosisState::Stressed(Signal::NumRequestsWaiting, 10.0)
        );
    }
}
