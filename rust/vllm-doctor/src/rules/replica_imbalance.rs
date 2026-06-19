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
use crate::models::{Confidence, DiagnosisState, Severity};
use crate::reports::templates::ReplicaImbalanceTemplate;
use crate::rules::Rule;
use crate::rules::RuleDefinition;
use crate::signals::{Signal, SignalGraph};

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
    template: &ReplicaImbalanceTemplate as &dyn crate::reports::templates::FindingTemplate,
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
        let Some(imbalance) = signals.evaluate(Signal::ReplicaRunningImbalance) else {
            return DiagnosisState::Healthy;
        };

        if imbalance >= self.cfg.critical_factor {
            DiagnosisState::firing(
                Severity::Critical,
                Confidence::High,
                Signal::ReplicaRunningImbalance,
                imbalance,
            )
        } else if imbalance >= self.cfg.imbalance_factor {
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Medium,
                Signal::ReplicaRunningImbalance,
                imbalance,
            )
        } else {
            DiagnosisState::Healthy
        }
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
            critical_factor: 3.0,
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
        let result = rule().run(&SignalGraph::new(&snapshot(
            vec![sample(2.5, &[("pod", "a")]), sample(1.0, &[("pod", "b")])],
            vec![sample(0.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
            vec![sample(0.8, &[("pod", "a")]), sample(0.8, &[("pod", "b")])],
        )));
        assert_eq!(
            result,
            DiagnosisState::firing(
                Severity::Warning,
                Confidence::Medium,
                Signal::ReplicaRunningImbalance,
                2.5
            )
        );
    }

    #[test]
    fn saturated_when_critical_imbalance() {
        let result = rule().run(&SignalGraph::new(&snapshot(
            vec![sample(5.0, &[("pod", "a")]), sample(1.0, &[("pod", "b")])],
            vec![sample(0.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
            vec![sample(0.8, &[("pod", "a")]), sample(0.8, &[("pod", "b")])],
        )));
        assert_eq!(
            result,
            DiagnosisState::firing(
                Severity::Critical,
                Confidence::High,
                Signal::ReplicaRunningImbalance,
                5.0
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
}
