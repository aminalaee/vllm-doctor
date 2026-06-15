//! Replica imbalance rule.
//!
//! Detects when load is unevenly distributed across the replicas of a deployment.
//!
//! Confidence:
//!   1 signal  -> low
//!   2 signals -> medium
//!   3 signals -> high
use std::collections::{HashMap, HashSet};

use crate::config::ReplicaImbalanceConfig;
use crate::models::{Confidence, FindingData};
use crate::rules::Rule;
use crate::signals::{Signal, SignalGraph};

pub struct ReplicaImbalanceRule {
    cfg: ReplicaImbalanceConfig,
}

impl ReplicaImbalanceRule {
    pub fn new(cfg: ReplicaImbalanceConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for ReplicaImbalanceRule {
    fn id(&self) -> &'static str {
        "replica_imbalance"
    }

    fn name(&self) -> &'static str {
        "Replica Imbalance"
    }

    fn title(&self) -> &'static str {
        "Replica imbalance"
    }

    fn severity(&self) -> crate::models::Severity {
        crate::models::Severity::Warning
    }

    fn likely_causes(&self) -> &'static [&'static str] {
        &[
            "Load balancer not distributing requests evenly (sticky sessions or connection reuse)",
            "A replica is not Ready or recently restarted, so traffic skips it",
            "Long-context requests pinned to a subset of replicas",
            "Autoscaler added replicas that are not yet receiving traffic",
        ]
    }

    fn recommendations(&self) -> &'static [&'static str] {
        &[
            "Check the load balancer / service routing and session affinity settings",
            "Verify readiness probes — an unready replica receives no traffic",
            "Compare per-replica latency and restart any unhealthy replica",
            "Confirm newly added replicas are registered with the load balancer",
        ]
    }

    fn related_metrics(&self) -> &'static [&'static str] {
        &[
            "vllm:num_requests_running",
            "vllm:num_requests_waiting",
            "vllm:kv_cache_usage_perc",
        ]
    }

    fn run(&self, signals: &SignalGraph<'_>) -> Option<FindingData> {
        let _label = signals.replica_label()?;

        let mut evidence = Vec::new();
        let mut signals_set: HashSet<String> = HashSet::new();
        let mut worst_count = 0;

        for model in signals.models() {
            let running = signals.per_replica(Signal::NumRequestsRunning, model.as_deref());
            let waiting = signals.per_replica(Signal::NumRequestsWaiting, model.as_deref());
            let cache = signals.per_replica(Signal::KvCacheUsagePerc, model.as_deref());

            let mut parts = Vec::new();
            let mut count = 0;

            if let Some((hi_replica, lo_replica)) = extremes(&running) {
                let total: f64 = running.values().sum();
                let hi = running[hi_replica];
                let lo = running[lo_replica];
                if total >= self.cfg.min_total_running
                    && ((lo > 0.0 && hi >= self.cfg.imbalance_factor * lo)
                        || (lo == 0.0 && hi > 0.0))
                {
                    count += 1;
                    signals_set.insert("Uneven running requests across replicas".to_string());
                    parts.push(format!(
                        "running {hi_replica}={:.0} vs {lo_replica}={:.0}",
                        hi, lo
                    ));
                }
            }

            if let Some((hi_replica, lo_replica)) = extremes(&cache) {
                let hi = cache[hi_replica];
                let lo = cache[lo_replica];
                if hi - lo >= self.cfg.cache_gap {
                    count += 1;
                    signals_set.insert("Uneven KV cache usage across replicas".to_string());
                    parts.push(format!(
                        "cache {hi_replica}={:.0}% vs {lo_replica}={:.0}%",
                        hi * 100.0,
                        lo * 100.0
                    ));
                }
            }

            if let Some((hi_replica, lo_replica)) = extremes(&waiting) {
                let hi = waiting[hi_replica];
                let lo = waiting[lo_replica];
                if hi > 0.0 && lo == 0.0 {
                    count += 1;
                    signals_set.insert(
                        "Requests queued on some replicas while others are idle".to_string(),
                    );
                    parts.push(format!(
                        "waiting {hi_replica}={:.0} vs {lo_replica}={:.0}",
                        hi, lo
                    ));
                }
            }

            if !parts.is_empty() {
                let prefix = model
                    .as_ref()
                    .map_or_else(String::new, |m| format!("{m}: "));
                evidence.push(prefix + &parts.join("; "));
                worst_count = worst_count.max(count);
            }
        }

        if evidence.is_empty() {
            return None;
        }

        let confidence = match worst_count {
            3 => Confidence::High,
            2 => Confidence::Medium,
            _ => Confidence::Low,
        };

        let summary = if evidence.len() == 1 {
            "Load is unevenly distributed across replicas — one replica is doing more work than its peers.".to_string()
        } else {
            format!(
                "{} models have load unevenly distributed across their replicas.",
                evidence.len()
            )
        };

        let mut signals_list: Vec<String> = signals_set.into_iter().collect();
        signals_list.sort();

        Some(FindingData {
            confidence,
            summary,
            signals: signals_list,
            evidence,
            severity: None,
        })
    }
}
fn extremes(values: &HashMap<String, f64>) -> Option<(&String, &String)> {
    let hi = values.iter().max_by(|a, b| a.1.total_cmp(b.1))?;
    let lo = values.iter().min_by(|a, b| a.1.total_cmp(b.1))?;
    Some((hi.0, lo.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};

    fn rule() -> ReplicaImbalanceRule {
        ReplicaImbalanceRule::new(ReplicaImbalanceConfig {
            imbalance_factor: 2.0,
            cache_gap: 0.30,
            min_total_running: 5.0,
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
    fn no_finding_when_balanced() {
        assert!(
            rule()
                .run(&SignalGraph::new(&snapshot(
                    vec![sample(10.0, &[("pod", "a")]), sample(10.0, &[("pod", "b")])],
                    vec![sample(0.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
                    vec![sample(0.8, &[("pod", "a")]), sample(0.8, &[("pod", "b")])],
                )))
                .is_none()
        );
    }

    #[test]
    fn detects_running_imbalance() {
        let finding = rule()
            .run(&SignalGraph::new(&snapshot(
                vec![sample(10.0, &[("pod", "a")]), sample(1.0, &[("pod", "b")])],
                vec![sample(0.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
                vec![sample(0.8, &[("pod", "a")]), sample(0.8, &[("pod", "b")])],
            )))
            .unwrap();
        assert_eq!(finding.confidence, Confidence::Low);
        assert!(
            finding
                .signals
                .contains(&"Uneven running requests across replicas".to_string())
        );
    }

    #[test]
    fn detects_multiple_signals() {
        let finding = rule()
            .run(&SignalGraph::new(&snapshot(
                vec![sample(10.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
                vec![sample(5.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
                vec![sample(0.9, &[("pod", "a")]), sample(0.5, &[("pod", "b")])],
            )))
            .unwrap();
        assert_eq!(finding.confidence, Confidence::High);
    }

    #[test]
    fn skips_single_replica() {
        assert!(
            rule()
                .run(&SignalGraph::new(&snapshot(
                    vec![sample(10.0, &[("pod", "a")]),],
                    vec![sample(0.0, &[("pod", "a")]),],
                    vec![sample(0.8, &[("pod", "a")]),],
                )))
                .is_none()
        );
    }
}
