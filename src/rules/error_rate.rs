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
use crate::models::{Confidence, DiagnosisState, Severity};
use crate::rules::Rule;
use crate::rules::RuleDefinition;
use crate::rules::templates::{FindingTemplate, TemplateContext};
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
    template: &ErrorRateTemplate as &dyn FindingTemplate,
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

        let errors_high = error_rate_high(signals, &self.cfg).is_some();
        let aborts_high = abort_rate_high(signals, &self.cfg).is_some();

        if !errors_high && !aborts_high {
            return DiagnosisState::Healthy;
        }

        // Signal/value: errors take precedence over aborts when errors are high.
        let confidence = if errors_high && aborts_high {
            Confidence::High
        } else {
            Confidence::Low
        };
        let severity = if errors_high {
            Severity::Critical
        } else {
            Severity::Warning
        };
        let (signal, value) = if errors_high {
            (Signal::ErrorRate, error_rate)
        } else {
            (Signal::AbortRate, abort_rate)
        };
        DiagnosisState::firing(severity, confidence, signal, value)
    }
}

/// Server-side error rate is at or above the high threshold.
///
/// Shared by the rule (for confidence/severity) and the template (for evidence).
fn error_rate_high(graph: &SignalGraph<'_>, cfg: &ErrorRateConfig) -> Option<f64> {
    let errors = graph.evaluate(Signal::RequestErrorTotal)?;
    let aborts = graph.evaluate(Signal::RequestAbortTotal)?;
    let success = graph.evaluate(Signal::RequestSuccessTotal)?;
    let total = errors + aborts + success;
    if total == 0.0 {
        return None;
    }
    let rate = errors / total;
    (rate >= cfg.high_error_rate).then_some(rate)
}

/// Client abort rate is at or above the high threshold.
///
/// Shared by the rule (for confidence/severity) and the template (for evidence).
fn abort_rate_high(graph: &SignalGraph<'_>, cfg: &ErrorRateConfig) -> Option<f64> {
    let errors = graph.evaluate(Signal::RequestErrorTotal)?;
    let aborts = graph.evaluate(Signal::RequestAbortTotal)?;
    let success = graph.evaluate(Signal::RequestSuccessTotal)?;
    let total = errors + aborts + success;
    if total == 0.0 {
        return None;
    }
    let rate = aborts / total;
    (rate >= cfg.high_abort_rate).then_some(rate)
}

/// Total request count used to derive error/abort rates.
fn request_totals(graph: &SignalGraph<'_>) -> Option<(f64, f64, f64)> {
    let errors = graph.evaluate(Signal::RequestErrorTotal)?;
    let aborts = graph.evaluate(Signal::RequestAbortTotal)?;
    let success = graph.evaluate(Signal::RequestSuccessTotal)?;
    Some((errors, aborts, success))
}

pub struct ErrorRateTemplate;

impl FindingTemplate for ErrorRateTemplate {
    fn summary(&self, _ctx: &TemplateContext<'_>) -> String {
        "Server is returning errors or clients are aborting at an elevated rate.".into()
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let cfg = &ctx.config.rules.error_rate;
        let Some((errors, aborts, success)) = request_totals(ctx.graph) else {
            return vec![];
        };
        let total = errors + aborts + success;
        if total == 0.0 {
            return vec![];
        }
        let error_rate = errors / total;
        let abort_rate = aborts / total;
        let mut lines = vec![];
        if error_rate_high(ctx.graph, cfg).is_some() {
            lines.push(format!(
                "Error rate: {:.1}% ({errors:.0} errors out of {total:.0} requests, \
                 threshold: {:.1}%)",
                error_rate * 100.0,
                cfg.high_error_rate * 100.0,
            ));
        }
        if abort_rate_high(ctx.graph, cfg).is_some() {
            lines.push(format!(
                "Abort rate: {:.1}% ({aborts:.0} aborts out of {total:.0} requests, \
                 threshold: {:.1}%)",
                abort_rate * 100.0,
                cfg.high_abort_rate * 100.0,
            ));
        }
        lines
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
    fn critical_when_errors_high() {
        // errors=10/110≈0.091 >= 0.05 → errors_high; aborts=0 → aborts_high=false
        // confidence=Low (only one signal high), severity=Critical
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(10.0, 0.0, 100.0))),
            DiagnosisState::firing(
                Severity::Critical,
                Confidence::Low,
                Signal::ErrorRate,
                10.0 / 110.0
            )
        );
    }

    #[test]
    fn warning_when_aborts_high() {
        // aborts=15/115≈0.130 >= 0.10 → aborts_high; errors=0 → errors_high=false
        // confidence=Low (only one signal high), severity=Warning
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.0, 15.0, 100.0))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Low,
                Signal::AbortRate,
                15.0 / 115.0
            )
        );
    }

    #[test]
    fn high_confidence_when_both_high() {
        // errors=10/115≈0.087 >= 0.05 → errors_high; aborts=15/115≈0.130 >= 0.10 → aborts_high
        // both high → confidence=High, severity=Critical (errors_high)
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(10.0, 15.0, 90.0))),
            DiagnosisState::firing(
                Severity::Critical,
                Confidence::High,
                Signal::ErrorRate,
                10.0 / 115.0
            )
        );
    }

    #[test]
    fn template_output() {
        // 1 error, 1 abort, 18 success -> total 20. error_rate = 0.05 (>= 0.05),
        // abort_rate = 0.05 (not >= 0.10).
        let snap = snapshot(1.0, 1.0, 18.0);
        let graph = SignalGraph::new(&snap);
        let config = Config::default();
        let ctx = TemplateContext {
            graph: &graph,
            config: &config,
            signal: Signal::RequestErrorTotal,
            value: 1.0,
        };
        let t = ErrorRateTemplate;
        assert_eq!(
            t.summary(&ctx),
            "Server is returning errors or clients are aborting at an elevated rate."
        );
        let evidence = t.evidence(&ctx);
        assert_eq!(
            evidence[0],
            "Error rate: 5.0% (1 errors out of 20 requests, threshold: 5.0%)"
        );
    }

    #[test]
    fn template_includes_abort_line_when_high() {
        let snap = snapshot(3.0, 3.0, 14.0);
        let graph = SignalGraph::new(&snap);
        let config = Config::default();
        let ctx = TemplateContext {
            graph: &graph,
            config: &config,
            signal: Signal::RequestErrorTotal,
            value: 3.0,
        };
        // total = 20, error_rate = 0.15, abort_rate = 0.15.
        let evidence = ErrorRateTemplate.evidence(&ctx);
        assert_eq!(
            evidence[0],
            "Error rate: 15.0% (3 errors out of 20 requests, threshold: 5.0%)"
        );
        assert_eq!(
            evidence[1],
            "Abort rate: 15.0% (3 aborts out of 20 requests, threshold: 10.0%)"
        );
    }
}
