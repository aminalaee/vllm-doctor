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
use crate::core::config::CoreConfig as Config;
use crate::core::config::PreemptionPressureConfig;
use crate::core::models::{ComparisonOperator, Confidence, DiagnosisState, EvidenceItem, Severity};
use crate::core::rules::Rule;
use crate::core::rules::RuleDefinition;
use crate::core::rules::templates::{FindingTemplate, GenericTemplate, TemplateContext};
use crate::core::signals::{Signal, SignalGraph};

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
    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<EvidenceItem> {
        let preemptions =
            Some(ctx.value).or_else(|| ctx.graph.evaluate(Signal::NumPreemptionsTotal));
        let Some(preemptions) = preemptions else {
            return GenericTemplate.evidence(ctx);
        };
        let mut items = vec![EvidenceItem::value(
            Signal::NumPreemptionsTotal.to_string(),
            preemptions,
            None::<String>,
        )];
        let cfg = &ctx.config.rules.preemption_pressure;
        if let Some(cache) = cache_high(ctx.graph, cfg) {
            items.push(EvidenceItem::threshold(
                Signal::KvCacheUsagePerc.to_string(),
                cache,
                cfg.high_cache_usage,
                None::<String>,
                ComparisonOperator::GreaterThanOrEqual,
            ));
        }
        items
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
    use crate::core::metrics::MetricSeriesSnapshot;
    use crate::core::metrics::series::{MetricSample, MetricSeries};
    use crate::core::models::DiagnosisState;
    use crate::core::signals::{Signal, SignalGraph};

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
            t.evidence(&ctx(&graph, &config))[0].summary(),
            "num_preemptions_total: 5"
        );
        let evidence = t.evidence(&ctx(&graph, &config));
        assert_eq!(evidence[0].summary(), "num_preemptions_total: 5");
        assert_eq!(
            evidence[1].summary(),
            "kv_cache_usage_perc: 0.85 ≥ threshold 0.80"
        );
    }

    #[test]
    fn template_no_cache_line_when_below_threshold() {
        let snap = snapshot(5.0, 0.50);
        let graph = SignalGraph::new(&snap);
        let config = Config::default();
        let evidence = PreemptionPressureTemplate.evidence(&ctx(&graph, &config));
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].summary(), "num_preemptions_total: 5");
    }
}
