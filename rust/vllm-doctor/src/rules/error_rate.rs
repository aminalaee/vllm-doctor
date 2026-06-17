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
use crate::config::Config;
use crate::config::ErrorRateConfig;
use crate::models::{DiagnosisState, Severity};
use crate::reports::templates::ErrorRateTemplate;
use crate::rules::Rule;
use crate::rules::RuleDefinition;
use crate::signals::{Signal, SignalGraph};

pub static DEFINITION: RuleDefinition = RuleDefinition {
    id: "error_rate",
    name: "Error Rate",
    title: "Elevated error rate",
    severity: Severity::Warning,
    likely_causes: &[
        "Server-side OOM or internal errors under high load",
        "Requests exceeding timeout limits causing client aborts",
        "High latency causing clients to disconnect before completion",
        "Resource exhaustion correlating with KV cache pressure",
    ],
    recommendations: &[
        "Inspect vLLM server logs for error details",
        "Correlate with KV cache pressure and queue pressure findings",
        "Check client timeout settings relative to observed TTFT and TPOT",
        "Reduce load or add replicas if errors correlate with traffic spikes",
    ],
    related_metrics: &["vllm:request_success_total"],
    template: &ErrorRateTemplate as &dyn crate::reports::templates::FindingTemplate,
};

pub struct ErrorRateRule {
    cfg: ErrorRateConfig,
}

impl ErrorRateRule {
    pub fn new(cfg: ErrorRateConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for ErrorRateRule {
    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        let Some(errors) = signals.evaluate(Signal::RequestErrorTotal) else {
            return DiagnosisState::unknown_signal(Signal::RequestErrorTotal);
        };
        let Some(aborts) = signals.evaluate(Signal::RequestAbortTotal) else {
            return DiagnosisState::unknown_signal(Signal::RequestAbortTotal);
        };
        let Some(success) = signals.evaluate(Signal::RequestSuccessTotal) else {
            return DiagnosisState::unknown_signal(Signal::RequestSuccessTotal);
        };

        if errors == 0.0 && aborts == 0.0 {
            return DiagnosisState::Healthy;
        }

        let total = success + errors + aborts;
        if total == 0.0 {
            return DiagnosisState::Healthy;
        }

        let error_rate = errors / total;
        let abort_rate = aborts / total;

        let errors_high = error_rate >= self.cfg.high_error_rate;
        let aborts_high = abort_rate >= self.cfg.high_abort_rate;

        if !errors_high && !aborts_high {
            return DiagnosisState::Healthy;
        }

        if errors_high {
            DiagnosisState::Saturated(Signal::ErrorRate, error_rate)
        } else if aborts_high {
            DiagnosisState::Stressed(Signal::AbortRate, abort_rate)
        } else {
            DiagnosisState::Healthy
        }
    }
}

pub fn factory(config: &Config) -> (&'static RuleDefinition, Box<dyn Rule>) {
    (
        &DEFINITION,
        Box::new(ErrorRateRule::new(config.rules.error_rate.clone())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::DiagnosisState;
    use crate::signals::{Signal, SignalGraph};

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
    fn healthy_when_rates_low() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(1.0, 1.0, 100.0))),
            DiagnosisState::Healthy
        );
    }

    #[test]
    fn saturated_when_errors_high() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(10.0, 0.0, 100.0))),
            DiagnosisState::Saturated(Signal::ErrorRate, 10.0 / 110.0)
        );
    }

    #[test]
    fn stressed_when_aborts_high() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.0, 15.0, 100.0))),
            DiagnosisState::Stressed(Signal::AbortRate, 15.0 / 115.0)
        );
    }
}
