//! Diagnostic rules.
use crate::config::Config;
use crate::metrics::MetricSeriesSnapshot;
use crate::models::{FindingData, Severity};

pub mod error_rate;
pub mod kv_cache_pressure;
pub mod low_throughput;
pub mod preemption_pressure;
pub mod prefix_cache_efficiency;
pub mod queue_latency;
pub mod queue_pressure;
pub mod replica_imbalance;
pub mod tpot_bottleneck;
pub mod ttft_bottleneck;

pub trait Rule {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn severity(&self) -> Severity;
    fn likely_causes(&self) -> &'static [&'static str];
    fn recommendations(&self) -> &'static [&'static str];
    fn related_metrics(&self) -> &'static [&'static str];

    fn run(&self, metrics: &MetricSeriesSnapshot) -> Option<FindingData>;
}

/// The result of running a single rule against a snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleResult {
    pub id: &'static str,
    pub name: &'static str,
    pub title: &'static str,
    pub severity: Severity,
    pub finding: Option<FindingData>,
}

impl RuleResult {
    pub fn is_significant(&self) -> bool {
        self.finding.is_some()
    }
}

/// Build the default rule set from configuration thresholds.
pub fn build_rules(config: &Config) -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(queue_pressure::QueuePressureRule::new(
            config.rules.queue_pressure.clone(),
        )),
        Box::new(queue_latency::QueueLatencyRule::new(
            config.rules.queue_latency.clone(),
        )),
        Box::new(kv_cache_pressure::KVCachePressureRule::new(
            config.rules.kv_cache_pressure.clone(),
        )),
        Box::new(preemption_pressure::PreemptionPressureRule::new(
            config.rules.preemption_pressure.clone(),
        )),
        Box::new(low_throughput::LowThroughputRule::new(
            config.rules.low_throughput.clone(),
        )),
        Box::new(error_rate::ErrorRateRule::new(
            config.rules.error_rate.clone(),
        )),
        Box::new(ttft_bottleneck::TtftBottleneckRule::new(
            config.rules.ttft_bottleneck.clone(),
        )),
        Box::new(tpot_bottleneck::TpotBottleneckRule::new(
            config.rules.tpot_bottleneck.clone(),
        )),
        Box::new(prefix_cache_efficiency::PrefixCacheEfficiencyRule::new(
            config.rules.prefix_cache_efficiency.clone(),
        )),
        Box::new(replica_imbalance::ReplicaImbalanceRule::new(
            config.rules.replica_imbalance.clone(),
        )),
    ]
}

/// Run every rule against the snapshot and collect metadata + findings.
pub fn run_all(rules: &[Box<dyn Rule>], metrics: &MetricSeriesSnapshot) -> Vec<RuleResult> {
    rules
        .iter()
        .map(|rule| RuleResult {
            id: rule.id(),
            name: rule.name(),
            title: rule.title(),
            severity: rule.severity(),
            finding: rule.run(metrics),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::metrics::series::{MetricSample, MetricSeries};

    fn snapshot_with_queue(waiting: f64, running: f64) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(waiting)]),
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(running)]),
            ..Default::default()
        }
    }

    #[test]
    fn build_rules_includes_all_ten() {
        let config = Config::default();
        let rules = build_rules(&config);
        let ids: Vec<&str> = rules.iter().map(|r| r.id()).collect();
        assert_eq!(ids.len(), 10);
        assert!(ids.contains(&"queue_pressure"));
        assert!(ids.contains(&"replica_imbalance"));
    }

    #[test]
    fn run_all_returns_result_for_every_rule() {
        let config = Config::default();
        let rules = build_rules(&config);
        let snapshot = snapshot_with_queue(10.0, 60.0);
        let results = run_all(&rules, &snapshot);
        assert_eq!(results.len(), 10);
        let significant: Vec<&RuleResult> = results.iter().filter(|r| r.is_significant()).collect();
        assert_eq!(significant.len(), 1);
        assert_eq!(significant[0].id, "queue_pressure");
    }
}
