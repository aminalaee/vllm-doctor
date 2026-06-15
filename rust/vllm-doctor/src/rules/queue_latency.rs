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
use crate::config::QueueLatencyConfig;
use crate::models::{DiagnosisState, Severity};
use crate::rules::Rule;
use crate::signals::{Signal, SignalGraph};

pub struct QueueLatencyRule {
    cfg: QueueLatencyConfig,
}

impl QueueLatencyRule {
    pub fn new(cfg: QueueLatencyConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for QueueLatencyRule {
    fn id(&self) -> &'static str {
        "queue_latency"
    }

    fn name(&self) -> &'static str {
        "Queue Latency"
    }

    fn title(&self) -> &'static str {
        "High queue latency"
    }

    fn severity(&self) -> Severity {
        Severity::Warning
    }

    fn likely_causes(&self) -> &'static [&'static str] {
        &[
            "Insufficient replica capacity for current request rate",
            "Long-context requests blocking admission of new sequences",
            "Autoscaling has not reacted to traffic increase",
            "KV cache exhaustion limiting sequence admission",
        ]
    }

    fn recommendations(&self) -> &'static [&'static str] {
        &[
            "Add replicas or increase concurrency limits",
            "Inspect autoscaling thresholds and reaction time",
            "Correlate with KV cache pressure — reduce max_num_seqs if cache is full",
            "Separate long-context traffic to a dedicated replica",
        ]
    }

    fn related_metrics(&self) -> &'static [&'static str] {
        &[
            "vllm:request_queue_time_seconds",
            "vllm:num_requests_waiting",
        ]
    }

    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        let Some(queue_time) = signals.evaluate(Signal::QueueTimeP95Seconds) else {
            return DiagnosisState::unknown_signal(Signal::QueueTimeP95Seconds);
        };

        if queue_time < self.cfg.high_queue_time_p95 {
            return DiagnosisState::Healthy;
        }

        DiagnosisState::Stressed(Signal::QueueTimeP95Seconds, queue_time)
    }
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
    fn stressed_when_queue_time_high() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(2.0, 3.0))),
            DiagnosisState::Stressed(Signal::QueueTimeP95Seconds, 2.0)
        );
    }
}
