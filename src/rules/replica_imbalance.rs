//! Replica imbalance rule.
//!
//! Detects when load is unevenly distributed across the replicas of a deployment.
//!
//! Confidence:
//!   1 signal  -> low
//!   2 signals -> medium
//!   3 signals -> high
use crate::config::Config;
use crate::config::ReplicaImbalanceConfig;
use crate::models::{Confidence, DiagnosisState, EvidenceItem, Severity};
use crate::rules::Rule;
use crate::rules::RuleDefinition;
use crate::rules::templates::{FindingTemplate, TemplateContext};
use crate::signals::{Signal, SignalGraph};
use std::cmp::Ordering;
use std::collections::HashMap;

pub static DEFINITION: RuleDefinition = RuleDefinition {
    id: "replica_imbalance",
    name: "Replica Imbalance",
    title: "Replica imbalance",
    severity: Severity::Warning,
    likely_causes: &[
        "Load balancer not distributing requests evenly (sticky sessions or connection reuse)",
        "A replica is not Ready or recently restarted, so traffic skips it",
        "Long-context requests pinned to a subset of replicas",
        "Autoscaler added replicas that are not yet receiving traffic",
    ],
    recommendations: &[
        "Check the load balancer / service routing and session affinity settings",
        "Verify readiness probes — an unready replica receives no traffic",
        "Compare per-replica latency and restart any unhealthy replica",
        "Confirm newly added replicas are registered with the load balancer",
    ],
    related_metrics: &[
        "vllm:num_requests_running",
        "vllm:num_requests_waiting",
        "vllm:kv_cache_usage_perc",
    ],
    template: &ReplicaImbalanceTemplate as &dyn FindingTemplate,
};

pub struct ReplicaImbalanceRule {
    cfg: ReplicaImbalanceConfig,
}

impl ReplicaImbalanceRule {
    pub fn new(cfg: ReplicaImbalanceConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for ReplicaImbalanceRule {
    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        if signals.replica_label().is_none() {
            return DiagnosisState::Healthy;
        }

        let mut worst_count = 0usize;
        let mut any_evidence = false;
        for model in signals.models() {
            if let Some(evidence) = model_imbalance(signals, model.as_deref(), &self.cfg) {
                any_evidence = true;
                worst_count = worst_count.max(evidence.count);
            }
        }

        if !any_evidence {
            return DiagnosisState::Healthy;
        }

        let confidence = if worst_count >= 3 {
            Confidence::High
        } else if worst_count == 2 {
            Confidence::Medium
        } else {
            Confidence::Low
        };
        DiagnosisState::firing(
            Severity::Warning,
            confidence,
            Signal::ReplicaRunningImbalance,
            worst_count as f64,
        )
    }
}

/// Per-model evidence produced by the imbalance signal counters.
///
/// Shared by the rule (which reads `count` for confidence) and the template
/// (which reads `parts` for evidence) so the per-model counting logic lives in
/// one place.
struct ModelEvidence {
    count: usize,
    signals: Vec<ImbalanceSignal>,
}

struct ImbalanceSignal {
    signal: Signal,
    affected: usize,
    total: usize,
}

/// Count imbalance signals for one model group and build the evidence parts.
///
/// Returns `None` when no signal fired for this model. Each signal is counted
/// independently:
///   1. running spread: busiest replica handles >= imbalance_factor x the
///      least busy (gated by `min_total_running`, or zero-low with hi > 0)
///   2. cache gap: kv_cache_usage_perc max - min >= cache_gap
///   3. waiting skew: one replica queued, another idle
fn model_imbalance(
    graph: &SignalGraph<'_>,
    model: Option<&str>,
    cfg: &ReplicaImbalanceConfig,
) -> Option<ModelEvidence> {
    let running = graph.per_replica(Signal::NumRequestsRunning, model);
    let waiting = graph.per_replica(Signal::NumRequestsWaiting, model);
    let cache = graph.per_replica(Signal::KvCacheUsagePerc, model);

    let mut signals = Vec::new();
    let mut count = 0;

    if running.len() >= 2 {
        let (_, hi_val) = max_entry(&running)?;
        let (_, lo_val) = min_entry(&running)?;
        let total: f64 = running.values().sum();
        if total >= cfg.min_total_running
            && ((lo_val > 0.0 && hi_val >= cfg.imbalance_factor * lo_val)
                || (lo_val == 0.0 && hi_val > 0.0))
        {
            let affected = if lo_val > 0.0 {
                running
                    .values()
                    .filter(|&&value| value >= cfg.imbalance_factor * lo_val)
                    .count()
            } else {
                running.values().filter(|&&value| value > 0.0).count()
            };
            count += 1;
            signals.push(ImbalanceSignal {
                signal: Signal::NumRequestsRunning,
                affected,
                total: running.len(),
            });
        }
    }

    if cache.len() >= 2 {
        let (_, hi_val) = max_entry(&cache)?;
        let (_, lo_val) = min_entry(&cache)?;
        if hi_val - lo_val >= cfg.cache_gap {
            let affected = cache
                .values()
                .filter(|&&value| value - lo_val >= cfg.cache_gap)
                .count();
            count += 1;
            signals.push(ImbalanceSignal {
                signal: Signal::KvCacheUsagePerc,
                affected,
                total: cache.len(),
            });
        }
    }

    if waiting.len() >= 2 {
        let (_, hi_val) = max_entry(&waiting)?;
        let (_, lo_val) = min_entry(&waiting)?;
        if hi_val > 0.0 && lo_val == 0.0 {
            let affected = waiting.values().filter(|&&value| value > 0.0).count();
            count += 1;
            signals.push(ImbalanceSignal {
                signal: Signal::NumRequestsWaiting,
                affected,
                total: waiting.len(),
            });
        }
    }

    if count > 0 {
        Some(ModelEvidence { count, signals })
    } else {
        None
    }
}

/// Return the entry (key, value) with the maximum value in a per-replica map.
fn max_entry(map: &HashMap<String, f64>) -> Option<(String, f64)> {
    map.iter()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(Ordering::Equal))
        .map(|(k, &v)| (k.clone(), v))
}

/// Return the entry (key, value) with the minimum value in a per-replica map.
fn min_entry(map: &HashMap<String, f64>) -> Option<(String, f64)> {
    map.iter()
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(Ordering::Equal))
        .map(|(k, &v)| (k.clone(), v))
}

pub struct ReplicaImbalanceTemplate;

impl FindingTemplate for ReplicaImbalanceTemplate {
    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<EvidenceItem> {
        let cfg = &ctx.config.rules.replica_imbalance;
        let mut items = vec![];
        for model in ctx.graph.models() {
            if let Some(evidence) = model_imbalance(ctx.graph, model.as_deref(), cfg) {
                for sig in &evidence.signals {
                    items.push(EvidenceItem::ReplicaDistribution {
                        affected: sig.affected,
                        total: sig.total,
                        metric: sig.signal.to_string(),
                        model: model.clone(),
                    });
                }
            }
        }
        if items.is_empty() {
            items.push(EvidenceItem::text("Replica imbalance detected"));
        }
        items
    }
}

pub fn factory(config: &Config) -> (&'static RuleDefinition, Box<dyn Rule>) {
    (
        &DEFINITION,
        Box::new(ReplicaImbalanceRule::new(
            config.rules.replica_imbalance.clone(),
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

    fn rule() -> ReplicaImbalanceRule {
        ReplicaImbalanceRule::new(ReplicaImbalanceConfig {
            imbalance_factor: 2.0,
            cache_gap: 0.30,
            min_total_running: 0.0,
        })
    }

    fn sample(value: f64, labels: &[(&str, &str)]) -> MetricSample {
        let mut s = MetricSample::new(value);
        for (k, v) in labels {
            s = s.with_label(*k, *v);
        }
        s
    }

    fn snapshot(
        running: Vec<MetricSample>,
        waiting: Vec<MetricSample>,
        cache: Vec<MetricSample>,
    ) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(running),
            num_requests_waiting: MetricSeries::from_samples(waiting),
            kv_cache_usage_perc: MetricSeries::from_samples(cache),
            ..Default::default()
        }
    }

    #[test]
    fn healthy_when_balanced() {
        let result = rule().run(&SignalGraph::new(&snapshot(
            vec![sample(10.0, &[("pod", "a")]), sample(10.0, &[("pod", "b")])],
            vec![sample(0.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
            vec![sample(0.8, &[("pod", "a")]), sample(0.8, &[("pod", "b")])],
        )));
        assert_eq!(result, DiagnosisState::Healthy);
    }
    #[test]
    fn stressed_when_running_imbalance() {
        // running a=2.5 vs b=1.0 -> running spread signal (count=1)
        // cache gap=0, waiting both 0 -> no other signals
        // worst_count=1 -> Low confidence, Warning severity
        let result = rule().run(&SignalGraph::new(&snapshot(
            vec![sample(2.5, &[("pod", "a")]), sample(1.0, &[("pod", "b")])],
            vec![sample(0.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
            vec![sample(0.8, &[("pod", "a")]), sample(0.8, &[("pod", "b")])],
        )));
        assert_eq!(
            result,
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Low,
                Signal::ReplicaRunningImbalance,
                1.0
            )
        );
    }

    #[test]
    fn medium_confidence_when_two_signals() {
        // running a=5.0 vs b=1.0 -> running spread (count=1)
        // cache a=0.9 vs b=0.5 -> gap=0.4 >= 0.30 (count=1)
        // waiting both 0 -> no waiting skew
        // worst_count=2 -> Medium confidence
        let result = rule().run(&SignalGraph::new(&snapshot(
            vec![sample(5.0, &[("pod", "a")]), sample(1.0, &[("pod", "b")])],
            vec![sample(0.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
            vec![sample(0.9, &[("pod", "a")]), sample(0.5, &[("pod", "b")])],
        )));
        assert_eq!(
            result,
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Medium,
                Signal::ReplicaRunningImbalance,
                2.0
            )
        );
    }

    #[test]
    fn high_confidence_when_three_signals() {
        // running a=5.0 vs b=1.0 -> running spread (count=1)
        // cache a=0.9 vs b=0.5 -> gap=0.4 >= 0.30 (count=1)
        // waiting a=3.0 vs b=0.0 -> waiting skew (count=1)
        // worst_count=3 -> High confidence
        let result = rule().run(&SignalGraph::new(&snapshot(
            vec![sample(5.0, &[("pod", "a")]), sample(1.0, &[("pod", "b")])],
            vec![sample(3.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
            vec![sample(0.9, &[("pod", "a")]), sample(0.5, &[("pod", "b")])],
        )));
        assert_eq!(
            result,
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::High,
                Signal::ReplicaRunningImbalance,
                3.0
            )
        );
    }

    #[test]
    fn saturated_when_critical_imbalance() {
        // running a=5.0 vs b=1.0 -> running spread (count=1)
        // cache gap=0, waiting both 0 -> no other signals
        // worst_count=1 -> Low confidence (severity is always Warning now)
        let result = rule().run(&SignalGraph::new(&snapshot(
            vec![sample(5.0, &[("pod", "a")]), sample(1.0, &[("pod", "b")])],
            vec![sample(0.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
            vec![sample(0.8, &[("pod", "a")]), sample(0.8, &[("pod", "b")])],
        )));
        assert_eq!(
            result,
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Low,
                Signal::ReplicaRunningImbalance,
                1.0
            )
        );
    }
    #[test]
    fn healthy_when_single_replica() {
        let result = rule().run(&SignalGraph::new(&snapshot(
            vec![sample(10.0, &[("pod", "a")])],
            vec![sample(0.0, &[("pod", "a")])],
            vec![sample(0.8, &[("pod", "a")])],
        )));
        assert_eq!(result, DiagnosisState::Healthy);
    }

    #[test]
    fn fires_on_cache_gap_without_running_imbalance() {
        // running a=10 vs b=10 -> balanced, no running spread signal
        // cache a=0.9 vs b=0.5 -> gap=0.4 >= 0.30 -> cache gap signal (count=1)
        // waiting both 0 -> no waiting skew
        // worst_count=1 -> Low confidence
        // Previously the top-level ReplicaRunningImbalance gate returned Healthy;
        // now the cache gap is counted independently.
        let result = rule().run(&SignalGraph::new(&snapshot(
            vec![sample(10.0, &[("pod", "a")]), sample(10.0, &[("pod", "b")])],
            vec![sample(0.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
            vec![sample(0.9, &[("pod", "a")]), sample(0.5, &[("pod", "b")])],
        )));
        assert_eq!(
            result,
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Low,
                Signal::ReplicaRunningImbalance,
                1.0
            )
        );
    }

    #[test]
    fn template_finds_extremes() {
        let snap = snapshot(
            vec![sample(10.0, &[("pod", "a")]), sample(2.0, &[("pod", "b")])],
            vec![],
            vec![],
        );
        let graph = SignalGraph::new(&snap);
        let config = Config::default();
        let ctx = TemplateContext {
            graph: &graph,
            config: &config,
            signal: Signal::ReplicaRunningImbalance,
            value: 5.0,
        };
        let evidence = ReplicaImbalanceTemplate.evidence(&ctx);
        assert_eq!(evidence.len(), 1);
        assert!(matches!(
            &evidence[0],
            EvidenceItem::ReplicaDistribution {
                affected: 1,
                total: 2,
                metric,
                model: None,
            } if metric == "num_requests_running"
        ));
    }

    #[test]
    fn template_counts_affected_replicas_and_keeps_model_separate() {
        let snap = snapshot(
            vec![
                sample(10.0, &[("pod", "a"), ("model_name", "llama")]),
                sample(10.0, &[("pod", "b"), ("model_name", "llama")]),
                sample(1.0, &[("pod", "c"), ("model_name", "llama")]),
            ],
            vec![],
            vec![],
        );
        let graph = SignalGraph::new(&snap);
        let config = Config::default();
        let ctx = TemplateContext {
            graph: &graph,
            config: &config,
            signal: Signal::ReplicaRunningImbalance,
            value: 10.0,
        };

        let evidence = ReplicaImbalanceTemplate.evidence(&ctx);

        assert!(matches!(
            &evidence[0],
            EvidenceItem::ReplicaDistribution {
                affected: 2,
                total: 3,
                metric,
                model: Some(model),
            } if metric == "num_requests_running" && model == "llama"
        ));
    }
}
