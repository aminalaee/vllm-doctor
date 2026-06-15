//! TTFT bottleneck rule.
//!
//! Detects when time to first token (p95) exceeds the configured threshold.
//! Confidence rises when TPOT is healthy — ruling out a general decode bottleneck
//! — and when requests are queuing, confirming prefill pressure.
use crate::config::TtftBottleneckConfig;
use crate::models::{DiagnosisState, Severity};
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

    fn severity(&self) -> Severity {
        Severity::Warning
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

    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        let Some(ttft) = signals.evaluate(Signal::TtftP95Seconds) else {
            return DiagnosisState::unknown_signal(Signal::TtftP95Seconds);
        };

        if ttft < self.cfg.high_ttft_p95 {
            return DiagnosisState::Healthy;
        }

        DiagnosisState::Stressed(Signal::TtftP95Seconds, ttft)
    }
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
    fn stressed_when_ttft_high() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(3.0, 0.5, 0.0))),
            DiagnosisState::Stressed(Signal::TtftP95Seconds, 3.0)
        );
    }
}
