//! Diagnostic rules.
use crate::core::config::CoreConfig as Config;
use crate::core::models::{DiagnosisState, Finding};
use crate::core::signals::SignalGraph;

pub mod definition;
pub mod error_rate;
pub mod kv_cache_pressure;
pub mod low_throughput;
pub mod preemption_pressure;
pub mod prefix_cache_efficiency;
pub mod queue_latency;
pub mod queue_pressure;
pub mod registry;
pub mod replica_imbalance;
pub mod templates;
pub mod tpot_bottleneck;
pub mod ttft_bottleneck;

pub use definition::RuleDefinition;
pub use registry::{RuleFactory, RuleRegistry};

pub trait Rule {
    fn run(&self, signals: &SignalGraph) -> DiagnosisState;
}

/// Map a rule's judgment state to the final presentation finding.
pub(crate) fn finding_for(
    definition: &RuleDefinition,
    state: DiagnosisState,
    graph: &SignalGraph<'_>,
    config: &Config,
) -> Option<Finding> {
    // Healthy and "could not evaluate" both produce no finding: a rule that
    // cannot read its signal stays quiet rather than emitting noise. Missing
    // signals are explained by report notices (e.g. scrape mode lacks latency).
    let judgment = match state {
        DiagnosisState::Healthy | DiagnosisState::Unknown(_) => return None,
        DiagnosisState::Firing(judgment) => judgment,
    };

    let ctx = crate::core::rules::templates::TemplateContext {
        graph,
        config,
        signal: judgment.signal,
        value: judgment.value,
    };

    Some(Finding {
        severity: judgment.severity,
        confidence: judgment.confidence,
        title: definition.title.to_string(),
        signals: vec![judgment.signal.to_string()],
        evidence: definition.template.evidence(&ctx),
        likely_causes: definition
            .likely_causes
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        recommendations: definition
            .recommendations
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        related_metrics: definition
            .related_metrics
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    })
}

/// Build the default rule registry from configuration thresholds.
pub fn build_registry(config: &Config) -> RuleRegistry {
    RuleRegistry::new()
        .register(queue_pressure::factory, config)
        .register(queue_latency::factory, config)
        .register(kv_cache_pressure::factory, config)
        .register(preemption_pressure::factory, config)
        .register(low_throughput::factory, config)
        .register(error_rate::factory, config)
        .register(ttft_bottleneck::factory, config)
        .register(tpot_bottleneck::factory, config)
        .register(prefix_cache_efficiency::factory, config)
        .register(replica_imbalance::factory, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::CoreConfig as Config;
    use crate::core::metrics::MetricSeriesSnapshot;
    use crate::core::metrics::series::{MetricSample, MetricSeries};
    use crate::core::models::Severity;

    fn snapshot_with_queue(waiting: f64, running: f64) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(waiting)]),
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(running)]),
            ..Default::default()
        }
    }

    #[test]
    fn build_registry_includes_all_ten() {
        let config = Config::default();
        let registry = build_registry(&config);
        let ids: Vec<&str> = registry.definitions().map(|d| d.id).collect();
        assert_eq!(ids.len(), 10);
        assert!(ids.contains(&"queue_pressure"));
        assert!(ids.contains(&"replica_imbalance"));
    }

    #[test]
    fn registry_run_all_returns_result_for_every_rule() {
        let config = Config::default();
        let registry = build_registry(&config);
        let snapshot = snapshot_with_queue(10.0, 60.0);
        let results = registry.run_all(&snapshot, &config);
        assert_eq!(results.len(), 10);
        let queue_result = results.iter().find(|r| r.id == "queue_pressure").unwrap();
        assert!(queue_result.is_significant());
    }

    #[test]
    fn registry_filters_by_severity() {
        let config = Config::default();
        let registry = build_registry(&config);
        let critical: Vec<&str> = registry
            .definitions_by_severity(Severity::Critical)
            .map(|d| d.id)
            .collect();
        assert_eq!(critical, vec!["kv_cache_pressure"]);
    }
}
