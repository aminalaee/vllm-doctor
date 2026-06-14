//! Replica imbalance rule.
//!
//! Detects when load is unevenly distributed across the replicas of a deployment —
//! one replica overloaded while its peers sit idle. This points at the routing layer
//! rather than the model: an uneven load balancer, an unready replica receiving no
//! traffic, or long-context requests pinned to a subset of pods.
//!
//! Because one vLLM deployment serves one model, replicas are grouped by the
//! `model_name` label and compared only against peers serving the same model. This
//! keeps the comparison correct on a shared Prometheus that scrapes several
//! deployments, with or without a model filter. A group with a single replica is
//! skipped (nothing to compare).
//!
//! Signals (each matching signal increases confidence), evaluated per model group:
//!   - running spread: busiest replica handles >= imbalance_factor x the least busy
//!     (gated by a minimum total running load to avoid firing on noise)
//!   - cache gap: kv_cache_usage_perc max - min >= cache_gap
//!   - waiting skew: one replica has queued requests while another has none
//!
//! Confidence:
//!   1 signal  -> low
//!   2 signals -> medium
//!   3 signals -> high
use std::collections::{HashMap, HashSet};

use crate::config::ReplicaImbalanceConfig;
use crate::metrics::series::MetricSeries;
use crate::metrics::{MetricSeriesSnapshot, detect_replica_label};
use crate::models::{Confidence, FindingData};
use crate::rules::Rule;

const MODEL_LABEL: &str = "model_name";

fn models(series: [&MetricSeries; 3]) -> Vec<Option<String>> {
    let mut values = HashSet::new();
    let mut labeled = false;
    for s in series {
        for sample in &s.samples {
            if let Some(model) = sample.labels.get(MODEL_LABEL) {
                labeled = true;
                values.insert(model.clone());
            }
        }
    }
    if labeled {
        let mut values: Vec<Option<String>> = values.into_iter().map(Some).collect();
        values.sort();
        values
    } else {
        vec![None]
    }
}

fn per_replica(series: &MetricSeries, model: Option<&str>, label: &str) -> HashMap<String, f64> {
    let scoped = match model {
        Some(m) => {
            let mut labels = HashMap::new();
            labels.insert(MODEL_LABEL.to_string(), m.to_string());
            series.filter(&labels)
        }
        None => series.clone(),
    };
    scoped
        .by(label)
        .into_iter()
        .filter_map(|(k, v)| v.filter(|v| v.is_finite()).map(|v| (k, v)))
        .collect()
}

fn extremes(values: &HashMap<String, f64>) -> Option<(&String, &String)> {
    let hi = values.iter().max_by(|a, b| a.1.total_cmp(b.1))?;
    let lo = values.iter().min_by(|a, b| a.1.total_cmp(b.1))?;
    Some((hi.0, lo.0))
}

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

    fn run(&self, metrics: &MetricSeriesSnapshot) -> Option<FindingData> {
        let label = detect_replica_label(metrics)?;

        let running_series = &metrics.num_requests_running;
        let waiting_series = &metrics.num_requests_waiting;
        let cache_series = &metrics.kv_cache_usage_perc;

        let mut evidence = Vec::new();
        let mut signals: HashSet<String> = HashSet::new();
        let mut worst_count = 0;

        for model in models([running_series, waiting_series, cache_series]) {
            let running = per_replica(running_series, model.as_deref(), label);
            let waiting = per_replica(waiting_series, model.as_deref(), label);
            let cache = per_replica(cache_series, model.as_deref(), label);

            let mut parts = Vec::new();
            let mut count = 0;

            if running.len() >= 2 {
                if let Some((hi, lo)) = extremes(&running) {
                    let total: f64 = running.values().sum();
                    if total >= self.cfg.min_total_running
                        && ((running[lo] > 0.0
                            && running[hi] >= self.cfg.imbalance_factor * running[lo])
                            || (running[lo] == 0.0 && running[hi] > 0.0))
                    {
                        count += 1;
                        signals.insert("Uneven running requests across replicas".to_string());
                        parts.push(format!(
                            "running {hi}={:.0} vs {lo}={:.0}",
                            running[hi], running[lo]
                        ));
                    }
                }
            }

            if cache.len() >= 2 {
                if let Some((hi, lo)) = extremes(&cache) {
                    if cache[hi] - cache[lo] >= self.cfg.cache_gap {
                        count += 1;
                        signals.insert("Uneven KV cache usage across replicas".to_string());
                        parts.push(format!(
                            "cache {:.0}% vs {:.0}%",
                            cache[hi] * 100.0,
                            cache[lo] * 100.0
                        ));
                    }
                }
            }

            if waiting.len() >= 2 {
                if let Some((hi, lo)) = extremes(&waiting) {
                    if waiting[hi] > 0.0 && waiting[lo] == 0.0 {
                        count += 1;
                        signals.insert(
                            "Requests queued on some replicas while others are idle".to_string(),
                        );
                        parts.push(format!(
                            "waiting {hi}={:.0} vs {lo}={:.0}",
                            waiting[hi], waiting[lo]
                        ));
                    }
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

        let mut signals: Vec<String> = signals.into_iter().collect();
        signals.sort();

        Some(FindingData {
            confidence,
            summary,
            signals,
            evidence,
            severity: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
                .run(&snapshot(
                    vec![sample(10.0, &[("pod", "a")]), sample(10.0, &[("pod", "b")])],
                    vec![sample(0.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
                    vec![sample(0.8, &[("pod", "a")]), sample(0.8, &[("pod", "b")])],
                ))
                .is_none()
        );
    }

    #[test]
    fn detects_running_imbalance() {
        let finding = rule()
            .run(&snapshot(
                vec![sample(10.0, &[("pod", "a")]), sample(1.0, &[("pod", "b")])],
                vec![sample(0.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
                vec![sample(0.8, &[("pod", "a")]), sample(0.8, &[("pod", "b")])],
            ))
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
            .run(&snapshot(
                vec![sample(10.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
                vec![sample(5.0, &[("pod", "a")]), sample(0.0, &[("pod", "b")])],
                vec![sample(0.9, &[("pod", "a")]), sample(0.5, &[("pod", "b")])],
            ))
            .unwrap();
        assert_eq!(finding.confidence, Confidence::High);
    }

    #[test]
    fn skips_single_replica() {
        assert!(
            rule()
                .run(&snapshot(
                    vec![sample(10.0, &[("pod", "a")])],
                    vec![sample(0.0, &[("pod", "a")])],
                    vec![sample(0.8, &[("pod", "a")])],
                ))
                .is_none()
        );
    }
}
