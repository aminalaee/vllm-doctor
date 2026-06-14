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
use crate::metrics::MetricSeriesSnapshot;
use crate::models::{Confidence, FindingData};
use crate::rules::Rule;

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

    fn severity(&self) -> crate::models::Severity {
        crate::models::Severity::Warning
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

    fn run(&self, metrics: &MetricSeriesSnapshot) -> Option<FindingData> {
        let queue_time = metrics.queue_time_p95_seconds.value()?;
        if !queue_time.is_finite() || queue_time < self.cfg.high_queue_time_p95 {
            return None;
        }

        let mut evidence = vec![format!(
            "Queue time p95: {:.3}s (threshold: {}s)",
            queue_time, self.cfg.high_queue_time_p95
        )];
        let mut signals = Vec::new();

        let waiting = metrics.num_requests_waiting.value();
        let waiting_confirmed = waiting.is_some_and(|v| v > 0.0);
        if waiting_confirmed {
            signals.push(format!(
                "{} requests queued — active backlog confirmed",
                waiting.unwrap() as i64
            ));
            evidence.push(format!("Waiting requests: {}", waiting.unwrap() as i64));
        }

        Some(FindingData {
            confidence: if waiting_confirmed {
                Confidence::High
            } else {
                Confidence::Low
            },
            summary: format!(
                "Requests are waiting {:.2}s (p95) in the queue before prefill begins — the server cannot admit requests fast enough.",
                queue_time
            ),
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
    fn no_finding_when_queue_time_low() {
        assert!(rule().run(&snapshot(0.5, 5.0)).is_none());
    }

    #[test]
    fn low_confidence_when_queue_time_high_but_no_waiting() {
        let finding = rule().run(&snapshot(2.0, 0.0)).unwrap();
        assert_eq!(finding.confidence, Confidence::Low);
    }

    #[test]
    fn high_confidence_when_queue_time_high_and_waiting_exists() {
        let finding = rule().run(&snapshot(2.0, 3.0)).unwrap();
        assert_eq!(finding.confidence, Confidence::High);
        assert!(finding.signals[0].contains("3"));
    }
}
