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
use crate::metrics::MetricSeriesSnapshot;
use crate::models::{Confidence, FindingData};
use crate::rules::Rule;

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

    fn severity(&self) -> crate::models::Severity {
        crate::models::Severity::Warning
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

    fn run(&self, metrics: &MetricSeriesSnapshot) -> Option<FindingData> {
        let waiting = metrics.num_requests_waiting.value()?;
        if waiting <= self.cfg.high_waiting as f64 {
            return None;
        }

        let running = metrics.num_requests_running.value();
        let running_high = running.is_some_and(|v| v > self.cfg.high_running as f64);

        let mut signals = Vec::new();
        let mut evidence = vec![format!(
            "Waiting requests: {waiting:.0} (threshold: {})",
            self.cfg.high_waiting
        )];
        if running_high {
            signals.push("Queue pressure compounding with server saturation".to_string());
            evidence.push(format!(
                "Running requests: {:.0} (threshold: {})",
                running.unwrap(),
                self.cfg.high_running
            ));
        }

        Some(FindingData {
            confidence: if running_high {
                Confidence::High
            } else {
                Confidence::Low
            },
            summary: "Requests are queuing faster than the server can process them.".to_string(),
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
    fn no_finding_when_waiting_low() {
        assert!(rule().run(&snapshot(3.0, 60.0)).is_none());
    }

    #[test]
    fn low_confidence_when_waiting_high_but_running_low() {
        let finding = rule().run(&snapshot(10.0, 10.0)).unwrap();
        assert_eq!(finding.confidence, Confidence::Low);
        assert!(finding.evidence[0].contains("Waiting requests: 10"));
    }

    #[test]
    fn high_confidence_when_both_high() {
        let finding = rule().run(&snapshot(10.0, 60.0)).unwrap();
        assert_eq!(finding.confidence, Confidence::High);
        assert_eq!(finding.signals.len(), 1);
    }
}
