//! TTFT bottleneck rule.
//!
//! Detects when time to first token (p95) exceeds the configured threshold.
//! Confidence rises when TPOT is healthy — ruling out a general decode bottleneck
//! — and when requests are queuing, confirming prefill pressure.
use crate::config::TtftBottleneckConfig;
use crate::models::{Confidence, FindingData};
use crate::rules::Rule;
use crate::signals::{Signal, SignalGraph};

pub struct TtftBottleneckRule {
    cfg: TtftBottleneckConfig,
}

impl TtftBottleneckRule {
    pub fn new(cfg: TtftBottleneckConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for TtftBottleneckRule {
    fn id(&self) -> &'static str {
        "ttft_bottleneck"
    }

    fn name(&self) -> &'static str {
        "High TTFT"
    }

    fn title(&self) -> &'static str {
        "High time to first token (TTFT)"
    }

    fn severity(&self) -> crate::models::Severity {
        crate::models::Severity::Warning
    }

    fn likely_causes(&self) -> &'static [&'static str] {
        &[
            "Long input prompts increasing prefill time",
            "Queue pressure delaying prefill start",
            "Chunked prefill not enabled or misconfigured",
            "Insufficient capacity for current prompt load",
        ]
    }

    fn recommendations(&self) -> &'static [&'static str] {
        &[
            "Enable or tune chunked prefill (--enable-chunked-prefill)",
            "Reduce max prompt length or filter long requests",
            "Inspect queue depth — consider adding replicas",
            "Separate long-context traffic to dedicated instances",
        ]
    }

    fn related_metrics(&self) -> &'static [&'static str] {
        &[
            "ttft_p95_seconds",
            "num_requests_waiting",
            "tpot_p95_seconds",
        ]
    }

    fn run(&self, signals: &SignalGraph<'_>) -> Option<FindingData> {
        let ttft = signals.evaluate(Signal::TtftP95Seconds)?;
        if ttft < self.cfg.high_ttft_p95 {
            return None;
        }

        let tpot = signals.evaluate(Signal::TpotP95Seconds);
        let waiting = signals.evaluate(Signal::NumRequestsWaiting);

        let mut signals_list = vec![format!(
            "TTFT p95 ({:.2}s) exceeds threshold ({}s)",
            ttft, self.cfg.high_ttft_p95
        )];
        let mut evidence = vec![format!("TTFT p95: {:.3}s", ttft)];

        let tpot_stable = tpot.is_some_and(|v| v.is_finite() && v < self.cfg.high_tpot_p95);
        if let Some(v) = tpot {
            evidence.push(format!("TPOT p95: {:.3}s", v));
        }
        if tpot_stable {
            signals_list.push(format!(
                "TPOT p95 ({:.2}s) is stable — decode is not the bottleneck",
                tpot.unwrap_or(0.0)
            ));
        }
        let waiting_confirmed = waiting.is_some_and(|v| v > 0.0);
        if let Some(w) = waiting {
            if w > 0.0 {
                signals_list.push(format!(
                    "{} requests queued — prefill pressure confirmed",
                    w as i64
                ));
            }
            evidence.push(format!("Waiting requests: {}", w as i64));
        }

        let signal_count = 1 + usize::from(tpot_stable) + usize::from(waiting_confirmed);
        let confidence = match signal_count {
            3 => Confidence::High,
            2 => Confidence::Medium,
            _ => Confidence::Low,
        };

        Some(FindingData {
            confidence,
            summary: "Requests are waiting too long before receiving the first token. This typically indicates prefill or queue pressure.".to_string(),
            signals: signals_list,
            evidence,
            severity: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};

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
    fn no_finding_when_ttft_low() {
        assert!(
            rule()
                .run(&SignalGraph::new(&snapshot(1.0, 0.1, 5.0)))
                .is_none()
        );
    }

    #[test]
    fn low_confidence_when_ttft_high_only() {
        let finding = rule()
            .run(&SignalGraph::new(&snapshot(3.0, 0.5, 0.0)))
            .unwrap();
        assert_eq!(finding.confidence, Confidence::Low);
    }

    #[test]
    fn high_confidence_when_ttft_high_with_stable_tpot_and_waiting() {
        let finding = rule()
            .run(&SignalGraph::new(&snapshot(3.0, 0.1, 5.0)))
            .unwrap();
        assert_eq!(finding.confidence, Confidence::High);
    }
}
