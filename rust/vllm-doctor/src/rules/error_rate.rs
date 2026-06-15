//! Error rate rule.
//!
//! Detects elevated server-side errors or client aborts relative to total requests.
//!
//! vLLM tracks finished requests by reason via `vllm:request_success_total`:
//!   - stop         — completed normally
//!   - error        — server-side failure (OOM, internal error)
//!   - abort        — client disconnected or request cancelled (often due to latency)
//!   - length       — hit max_tokens limit (not an error)
//!   - repetition   — stopped by repetition penalty (not an error)
//!
//! Signals (each matching signal increases confidence):
//!   - error rate high: server is failing requests internally
//!   - abort rate high: clients are giving up, often due to slow responses
//!
//! Confidence:
//!   error high only, or abort high only  → low
//!   both high                            → high
//!
//! Severity is overridden to `Critical` when server-side errors are high.
use crate::config::ErrorRateConfig;
use crate::models::{Confidence, FindingData, Severity};
use crate::rules::Rule;
use crate::signals::{Signal, SignalGraph};

pub struct ErrorRateRule {
    cfg: ErrorRateConfig,
}

impl ErrorRateRule {
    pub fn new(cfg: ErrorRateConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for ErrorRateRule {
    fn id(&self) -> &'static str {
        "error_rate"
    }

    fn name(&self) -> &'static str {
        "Error Rate"
    }

    fn title(&self) -> &'static str {
        "Elevated error rate"
    }

    fn severity(&self) -> Severity {
        Severity::Warning
    }

    fn likely_causes(&self) -> &'static [&'static str] {
        &[
            "Server-side OOM or internal errors under high load",
            "Requests exceeding timeout limits causing client aborts",
            "High latency causing clients to disconnect before completion",
            "Resource exhaustion correlating with KV cache pressure",
        ]
    }

    fn recommendations(&self) -> &'static [&'static str] {
        &[
            "Inspect vLLM server logs for error details",
            "Correlate with KV cache pressure and queue pressure findings",
            "Check client timeout settings relative to observed TTFT and TPOT",
            "Reduce load or add replicas if errors correlate with traffic spikes",
        ]
    }

    fn related_metrics(&self) -> &'static [&'static str] {
        &["vllm:request_success_total"]
    }

    fn run(&self, signals: &SignalGraph<'_>) -> Option<FindingData> {
        let errors = signals.evaluate(Signal::RequestErrorTotal)?;
        let aborts = signals.evaluate(Signal::RequestAbortTotal)?;
        let success = signals.evaluate(Signal::RequestSuccessTotal)?;

        if errors == 0.0 && aborts == 0.0 {
            return None;
        }

        let total = success + errors + aborts;
        if total == 0.0 {
            return None;
        }

        let error_rate = errors / total;
        let abort_rate = aborts / total;

        let errors_high = error_rate >= self.cfg.high_error_rate;
        let aborts_high = abort_rate >= self.cfg.high_abort_rate;

        if !errors_high && !aborts_high {
            return None;
        }

        let mut signals_list = Vec::new();
        let mut evidence = Vec::new();

        if errors_high {
            signals_list.push("Elevated server-side error rate".to_string());
            evidence.push(format!(
                "Error rate: {:.1}% ({:.0} errors out of {:.0} requests, threshold: {:.1}%)",
                error_rate * 100.0,
                errors,
                total,
                self.cfg.high_error_rate * 100.0
            ));
        }
        if aborts_high {
            signals_list.push(
                "Elevated client abort rate — clients disconnecting before response".to_string(),
            );
            evidence.push(format!(
                "Abort rate: {:.1}% ({:.0} aborts out of {:.0} requests, threshold: {:.1}%)",
                abort_rate * 100.0,
                aborts,
                total,
                self.cfg.high_abort_rate * 100.0
            ));
        }

        Some(FindingData {
            confidence: if errors_high && aborts_high {
                Confidence::High
            } else {
                Confidence::Low
            },
            severity: if errors_high {
                Some(Severity::Critical)
            } else {
                None
            },
            summary: "Server is returning errors or clients are aborting at an elevated rate."
                .to_string(),
            signals: signals_list,
            evidence,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};

    fn rule() -> ErrorRateRule {
        ErrorRateRule::new(ErrorRateConfig {
            high_error_rate: 0.05,
            high_abort_rate: 0.10,
        })
    }

    fn snapshot(errors: f64, aborts: f64, success: f64) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            request_error_total: MetricSeries::from_samples(vec![MetricSample::new(errors)]),
            request_abort_total: MetricSeries::from_samples(vec![MetricSample::new(aborts)]),
            request_success_total: MetricSeries::from_samples(vec![MetricSample::new(success)]),
            ..Default::default()
        }
    }

    #[test]
    fn no_finding_when_rates_low() {
        assert!(
            rule()
                .run(&SignalGraph::new(&snapshot(1.0, 1.0, 100.0)))
                .is_none()
        );
    }

    #[test]
    fn low_confidence_when_only_errors_high() {
        let finding = rule()
            .run(&SignalGraph::new(&snapshot(10.0, 0.0, 100.0)))
            .unwrap();
        assert_eq!(finding.confidence, Confidence::Low);
        assert_eq!(finding.severity, Some(Severity::Critical));
    }

    #[test]
    fn high_confidence_when_both_high() {
        let finding = rule()
            .run(&SignalGraph::new(&snapshot(10.0, 15.0, 100.0)))
            .unwrap();
        assert_eq!(finding.confidence, Confidence::High);
    }

    #[test]
    fn severity_warning_when_only_aborts_high() {
        let finding = rule()
            .run(&SignalGraph::new(&snapshot(0.0, 15.0, 100.0)))
            .unwrap();
        assert_eq!(finding.severity, None);
    }
}
