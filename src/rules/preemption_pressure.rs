//! Preemption pressure rule.
//!
//! Detects when vLLM has preempted sequences due to KV cache exhaustion.
//!
//! Preemption happens when a running sequence must be evicted from GPU KV cache to free
//! space for another. The evicted sequence is re-computed later, wasting GPU cycles and
//! adding latency. Any preemptions indicate the server ran out of KV cache at least once.
//!
//! Signals (each matching signal increases confidence):
//!   - num_preemptions_total > 0: preemptions have occurred
//!   - kv_cache_usage_perc >= threshold: cache is currently under pressure
//!
//! Confidence:
//!   preemptions only               → medium (happened at some point, may not be ongoing)
//!   preemptions + high cache usage → high   (actively under memory pressure)
use crate::config::Config;
use crate::config::PreemptionPressureConfig;
use crate::models::{Confidence, DiagnosisState, Severity};
use crate::rules::Rule;
use crate::rules::RuleDefinition;
use crate::rules::templates::{FindingTemplate, GenericTemplate, TemplateContext};
use crate::signals::{Signal, SignalGraph};

pub static DEFINITION: RuleDefinition = RuleDefinition {
    id: "preemption_pressure",
    name: "Preemption Pressure",
    title: "Preemption pressure",
    severity: Severity::Warning,
    likely_causes: &[
        "KV cache too small for the concurrent request mix",
        "Long-context requests exhausting cache before shorter ones complete",
        "max_num_seqs set too high relative to available GPU memory",
    ],
    recommendations: &[
        "Reduce max_num_seqs to limit concurrent sequences in GPU memory",
        "Reduce max_num_batched_tokens to lower per-step memory pressure",
        "Increase gpu_memory_utilization if GPU headroom exists",
        "Route long-context requests to a dedicated replica",
    ],
    related_metrics: &["vllm:num_preemptions_total", "vllm:kv_cache_usage_perc"],
    template: &PreemptionPressureTemplate as &dyn FindingTemplate,
};

pub struct PreemptionPressureRule {
    cfg: PreemptionPressureConfig,
}

impl PreemptionPressureRule {
    pub fn new(cfg: PreemptionPressureConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for PreemptionPressureRule {
    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        let Some(preemptions) = signals.evaluate(Signal::NumPreemptionsTotal) else {
            return DiagnosisState::unknown_signal(Signal::NumPreemptionsTotal);
        };

        if preemptions == 0.0 {
            return DiagnosisState::Healthy;
        }

        let confidence = if cache_high(signals, &self.cfg).is_some() {
            Confidence::High
        } else {
            Confidence::Medium
        };
        DiagnosisState::firing(
            Severity::Warning,
            confidence,
            Signal::NumPreemptionsTotal,
            preemptions,
        )
    }
}

/// KV cache is currently under pressure (usage at or above the high threshold).
///
/// Shared by the rule (for confidence) and the template (for evidence).
fn cache_high(graph: &SignalGraph<'_>, cfg: &PreemptionPressureConfig) -> Option<f64> {
    graph
        .evaluate(Signal::KvCacheUsagePerc)
        .filter(|&c| c >= cfg.high_cache_usage)
}

pub struct PreemptionPressureTemplate;

impl FindingTemplate for PreemptionPressureTemplate {
    fn summary(&self, ctx: &TemplateContext<'_>) -> String {
        let preemptions =
            Some(ctx.value).or_else(|| ctx.graph.evaluate(Signal::NumPreemptionsTotal));
        let Some(preemptions) = preemptions else {
            return GenericTemplate.summary(ctx);
        };
        format!(
            "vLLM has preempted {preemptions:.0} sequences — \
             KV cache exhaustion is forcing sequences to be re-computed."
        )
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let preemptions =
            Some(ctx.value).or_else(|| ctx.graph.evaluate(Signal::NumPreemptionsTotal));
        let Some(preemptions) = preemptions else {
            return GenericTemplate.evidence(ctx);
        };
        let mut lines = vec![format!("Preemptions total: {preemptions:.0}")];
        let cfg = &ctx.config.rules.preemption_pressure;
        if let Some(cache) = cache_high(ctx.graph, cfg) {
            lines.push(format!(
                "GPU KV cache usage: {:.0}% (threshold: {:.0}%)",
                cache * 100.0,
                cfg.high_cache_usage * 100.0,
            ));
        }
        lines
    }
}

pub fn factory(config: &Config) -> (&'static RuleDefinition, Box<dyn Rule>) {
    (
        &DEFINITION,
        Box::new(PreemptionPressureRule::new(
            config.rules.preemption_pressure.clone(),
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::DiagnosisState;
    use crate::signals::{Signal, SignalGraph};

    fn rule() -> PreemptionPressureRule {
        PreemptionPressureRule::new(PreemptionPressureConfig {
            high_cache_usage: 0.80,
        })
    }

    fn snapshot(preemptions: f64, cache: f64) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_preemptions_total: MetricSeries::from_samples(vec![MetricSample::new(preemptions)]),
            kv_cache_usage_perc: MetricSeries::from_samples(vec![MetricSample::new(cache)]),
            ..Default::default()
        }
    }

    #[test]
    fn healthy_when_no_preemptions() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.0, 0.9))),
            DiagnosisState::Healthy
        );
    }

    #[test]
    fn fires_warning_when_preemptions_present() {
        // cache=0.5 < high_cache_usage=0.80 → cache_high=false → Medium confidence
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(5.0, 0.5))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Medium,
                Signal::NumPreemptionsTotal,
                5.0
            )
        );
    }

    #[test]
    fn high_confidence_when_cache_high() {
        // cache=0.90 >= high_cache_usage=0.80 → cache_high=true → High confidence
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(5.0, 0.90))),
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::High,
                Signal::NumPreemptionsTotal,
                5.0
            )
        );
    }

    fn ctx<'a>(graph: &'a SignalGraph<'a>, config: &'a Config) -> TemplateContext<'a> {
        TemplateContext {
            graph,
            config,
            signal: Signal::NumPreemptionsTotal,
            value: 5.0,
        }
    }

    #[test]
    fn template_output_order() {
        let snap = snapshot(5.0, 0.85);
        let graph = SignalGraph::new(&snap);
        let config = Config::default();
        let t = PreemptionPressureTemplate;
        assert_eq!(
            t.summary(&ctx(&graph, &config)),
            "vLLM has preempted 5 sequences — \
             KV cache exhaustion is forcing sequences to be re-computed."
        );
        let evidence = t.evidence(&ctx(&graph, &config));
        assert_eq!(evidence[0], "Preemptions total: 5");
        assert_eq!(evidence[1], "GPU KV cache usage: 85% (threshold: 80%)");
    }

    #[test]
    fn template_no_cache_line_when_below_threshold() {
        let snap = snapshot(5.0, 0.50);
        let graph = SignalGraph::new(&snap);
        let config = Config::default();
        let evidence = PreemptionPressureTemplate.evidence(&ctx(&graph, &config));
        assert_eq!(evidence, vec!["Preemptions total: 5"]);
    }
}
