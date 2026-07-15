//! Low throughput rule.
//!
//! Detects when the server is processing requests below expected throughput with
//! no queue pressure. This indicates the server is underutilized — not saturated —
//! which points to low incoming load, poor batching, or misconfigured concurrency.
//!
//! Signals (each matching signal increases confidence):
//!   - prompt_tokens_per_second below threshold: prefill throughput is low
//!   - generation_tokens_per_second below threshold: decode throughput is low
//!   - num_requests_running very low: few active requests, no batching benefit
//!
//! Suppressed when requests are waiting — low throughput with a queue is a
//! capacity problem (queue pressure), not an underutilization problem.
//!
//! Confidence:
//!   both prompt and gen low, or running very low  → medium
//!   only one metric low                           → low
use crate::config::Config;
use crate::config::LowThroughputConfig;
use crate::models::{ComparisonOperator, Confidence, DiagnosisState, EvidenceItem, Severity};
use crate::rules::Rule;
use crate::rules::RuleDefinition;
use crate::rules::templates::{FindingTemplate, TemplateContext};
use crate::signals::{Signal, SignalGraph};

pub static DEFINITION: RuleDefinition = RuleDefinition {
    id: "low_throughput",
    name: "Low Throughput",
    title: "Low throughput",
    severity: Severity::Warning,
    likely_causes: &[
        "Low incoming request rate — server is idle",
        "Poor batching due to few concurrent requests",
        "Suboptimal max_num_seqs or max_num_batched_tokens for current load",
    ],
    recommendations: &[
        "Increase concurrent requests to improve batching efficiency",
        "Review max_num_seqs and max_num_batched_tokens settings",
        "Compare against benchmark baseline to confirm underperformance",
        "Consider consolidating replicas if load is consistently low",
    ],
    related_metrics: &[
        "vllm:prompt_tokens_per_second",
        "vllm:generation_tokens_per_second",
        "vllm:num_requests_running",
    ],
    template: &LowThroughputTemplate as &dyn FindingTemplate,
};

pub struct LowThroughputRule {
    cfg: LowThroughputConfig,
}

impl LowThroughputRule {
    pub fn new(cfg: LowThroughputConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for LowThroughputRule {
    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        let prompt = signals.evaluate(Signal::PromptTokensPerSecond);
        let gen_tps = signals.evaluate(Signal::GenerationTokensPerSecond);
        if prompt.is_none() && gen_tps.is_none() {
            return DiagnosisState::unknown_signal(Signal::PromptTokensPerSecond);
        }

        let prompt_low_v = prompt_low(signals, &self.cfg);
        let gen_low_v = gen_low(signals, &self.cfg);

        if prompt_low_v.is_none() && gen_low_v.is_none() {
            return DiagnosisState::Healthy;
        }

        let waiting = signals.evaluate(Signal::NumRequestsWaiting).unwrap_or(0.0);
        if waiting > 0.0 {
            return DiagnosisState::Healthy;
        }

        let running = signals.evaluate(Signal::NumRequestsRunning).unwrap_or(0.0);
        if running == 0.0 && waiting == 0.0 {
            return DiagnosisState::Healthy;
        }

        let running_low_v = running_low(signals, &self.cfg);
        let confidence =
            if (prompt_low_v.is_some() && gen_low_v.is_some()) || running_low_v.is_some() {
                Confidence::Medium
            } else {
                Confidence::Low
            };
        if prompt_low_v.is_some() {
            DiagnosisState::firing(
                Severity::Warning,
                confidence,
                Signal::PromptTokensPerSecond,
                prompt.unwrap_or(0.0),
            )
        } else {
            DiagnosisState::firing(
                Severity::Warning,
                confidence,
                Signal::GenerationTokensPerSecond,
                gen_tps.unwrap_or(0.0),
            )
        }
    }
}

/// Prompt token throughput is below the configured threshold.
///
/// Shared by the rule (for confidence) and the template (for evidence).
fn prompt_low(graph: &SignalGraph<'_>, cfg: &LowThroughputConfig) -> Option<f64> {
    graph
        .evaluate(Signal::PromptTokensPerSecond)
        .filter(|&v| v < cfg.low_prompt_tps)
}

/// Generation token throughput is below the configured threshold.
///
/// Shared by the rule (for confidence) and the template (for evidence).
fn gen_low(graph: &SignalGraph<'_>, cfg: &LowThroughputConfig) -> Option<f64> {
    graph
        .evaluate(Signal::GenerationTokensPerSecond)
        .filter(|&v| v < cfg.low_gen_tps)
}

/// Running concurrency is very low, indicating underutilization.
///
/// Shared by the rule (for confidence) and the template (for evidence).
fn running_low(graph: &SignalGraph<'_>, cfg: &LowThroughputConfig) -> Option<f64> {
    graph
        .evaluate(Signal::NumRequestsRunning)
        .filter(|&r| r < cfg.low_running as f64)
}

pub struct LowThroughputTemplate;

impl FindingTemplate for LowThroughputTemplate {
    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<EvidenceItem> {
        let cfg = &ctx.config.rules.low_throughput;
        let mut items = vec![];
        if let Some(prompt) = prompt_low(ctx.graph, cfg) {
            items.push(EvidenceItem::threshold(
                Signal::PromptTokensPerSecond.to_string(),
                prompt,
                cfg.low_prompt_tps,
                None::<String>,
                ComparisonOperator::LessThan,
            ));
        }
        if let Some(gen_tps) = gen_low(ctx.graph, cfg) {
            items.push(EvidenceItem::threshold(
                Signal::GenerationTokensPerSecond.to_string(),
                gen_tps,
                cfg.low_gen_tps,
                None::<String>,
                ComparisonOperator::LessThan,
            ));
        }
        if let Some(running) = running_low(ctx.graph, cfg) {
            items.push(EvidenceItem::value(
                Signal::NumRequestsRunning.to_string(),
                running,
                None::<String>,
            ));
        }
        items
    }
}

pub fn factory(config: &Config) -> (&'static RuleDefinition, Box<dyn Rule>) {
    (
        &DEFINITION,
        Box::new(LowThroughputRule::new(config.rules.low_throughput.clone())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::DiagnosisState;
    use crate::signals::{Signal, SignalGraph};

    fn rule() -> LowThroughputRule {
        LowThroughputRule::new(LowThroughputConfig {
            low_prompt_tps: 10.0,
            low_gen_tps: 50.0,
            low_running: 2,
        })
    }

    fn snapshot(prompt: f64, gen_tps: f64, running: f64, waiting: f64) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            prompt_tokens_per_second: MetricSeries::from_samples(vec![MetricSample::new(prompt)]),
            generation_tokens_per_second: MetricSeries::from_samples(vec![MetricSample::new(
                gen_tps,
            )]),
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(running)]),
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(waiting)]),
            ..Default::default()
        }
    }

    #[test]
    fn healthy_when_throughput_high() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(100.0, 100.0, 5.0, 0.0))),
            DiagnosisState::Healthy
        );
    }

    #[test]
    fn healthy_when_waiting_exists() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(5.0, 5.0, 5.0, 1.0))),
            DiagnosisState::Healthy
        );
    }

    #[test]
    fn healthy_when_idle() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.0, 0.0, 0.0, 0.0))),
            DiagnosisState::Healthy
        );
    }

    #[test]
    fn warns_when_prompt_low() {
        // prompt=5 < 10 → prompt_low; gen=100 >= 50 → not gen_low; running=5 >= 2 → not running_low
        // only one signal low → Low confidence
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(5.0, 100.0, 5.0, 0.0))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Low,
                Signal::PromptTokensPerSecond,
                5.0
            )
        );
    }

    #[test]
    fn warns_when_gen_low() {
        // prompt=100 >= 10 → not prompt_low; gen=20 < 50 → gen_low; running=5 >= 2 → not running_low
        // only one signal low → Low confidence
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(100.0, 20.0, 5.0, 0.0))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Low,
                Signal::GenerationTokensPerSecond,
                20.0
            )
        );
    }

    #[test]
    fn warns_when_both_low() {
        // prompt=5 < 10 → prompt_low; gen=20 < 50 → gen_low; running=5 >= 2 → not running_low
        // both prompt and gen low → Medium confidence
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(5.0, 20.0, 5.0, 0.0))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Medium,
                Signal::PromptTokensPerSecond,
                5.0
            )
        );
    }

    #[test]
    fn medium_confidence_when_running_low() {
        // prompt=5 < 10 → prompt_low; gen=100 >= 50 → not gen_low; running=1 < 2 → running_low
        // running_low → Medium confidence
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(5.0, 100.0, 1.0, 0.0))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Medium,
                Signal::PromptTokensPerSecond,
                5.0
            )
        );
    }

    #[test]
    fn template_output() {
        let snap = snapshot(5.0, 20.0, 1.0, 0.0);
        let graph = SignalGraph::new(&snap);
        let config = Config::default();
        let ctx = TemplateContext {
            graph: &graph,
            config: &config,
            signal: Signal::PromptTokensPerSecond,
            value: 5.0,
        };
        let t = LowThroughputTemplate;
        assert_eq!(
            t.evidence(&ctx)[0].summary(),
            "prompt_tokens_per_second: 5 < threshold 10"
        );
        let evidence = t.evidence(&ctx);
        assert_eq!(
            evidence[0].summary(),
            "prompt_tokens_per_second: 5 < threshold 10"
        );
        assert_eq!(
            evidence[1].summary(),
            "generation_tokens_per_second: 20 < threshold 50"
        );
        assert_eq!(evidence[2].summary(), "num_requests_running: 1");
    }
}
