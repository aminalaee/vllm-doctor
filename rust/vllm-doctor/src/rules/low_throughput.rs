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
use crate::config::LowThroughputConfig;
use crate::metrics::MetricSeriesSnapshot;
use crate::models::{Confidence, FindingData};
use crate::rules::Rule;

pub struct LowThroughputRule {
    cfg: LowThroughputConfig,
}

impl LowThroughputRule {
    pub fn new(cfg: LowThroughputConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for LowThroughputRule {
    fn id(&self) -> &'static str {
        "low_throughput"
    }

    fn name(&self) -> &'static str {
        "Low Throughput"
    }

    fn title(&self) -> &'static str {
        "Low throughput"
    }

    fn severity(&self) -> crate::models::Severity {
        crate::models::Severity::Warning
    }

    fn likely_causes(&self) -> &'static [&'static str] {
        &[
            "Low incoming request rate — server is idle",
            "Poor batching due to few concurrent requests",
            "Suboptimal max_num_seqs or max_num_batched_tokens for current load",
        ]
    }

    fn recommendations(&self) -> &'static [&'static str] {
        &[
            "Increase concurrent requests to improve batching efficiency",
            "Review max_num_seqs and max_num_batched_tokens settings",
            "Compare against benchmark baseline to confirm underperformance",
            "Consider consolidating replicas if load is consistently low",
        ]
    }

    fn related_metrics(&self) -> &'static [&'static str] {
        &[
            "vllm:prompt_tokens_per_second",
            "vllm:generation_tokens_per_second",
            "vllm:num_requests_running",
        ]
    }

    fn run(&self, metrics: &MetricSeriesSnapshot) -> Option<FindingData> {
        let prompt = metrics.prompt_tokens_per_second.value();
        let gen_tps = metrics.generation_tokens_per_second.value();
        if prompt.is_none() && gen_tps.is_none() {
            return None;
        }

        let prompt_low = prompt.is_some_and(|v| v < self.cfg.low_prompt_tps);
        let gen_low = gen_tps.is_some_and(|v| v < self.cfg.low_gen_tps);

        if !prompt_low && !gen_low {
            return None;
        }

        let waiting = metrics.num_requests_waiting.value();
        if waiting.is_some_and(|v| v > 0.0) {
            return None;
        }

        let mut signals = Vec::new();
        let mut evidence = Vec::new();

        if prompt_low && gen_low {
            signals.push(
                "Both prefill and decode throughput below threshold — server underutilized"
                    .to_string(),
            );
        } else if prompt_low {
            signals.push("Prefill throughput below threshold".to_string());
        } else {
            signals.push("Decode throughput below threshold".to_string());
        }

        if let Some(p) = prompt {
            evidence.push(format!(
                "Prompt tokens/s: {p:.1} (threshold: {})",
                self.cfg.low_prompt_tps
            ));
        }
        if let Some(g) = gen_tps {
            evidence.push(format!(
                "Generation tokens/s: {g:.1} (threshold: {})",
                self.cfg.low_gen_tps
            ));
        }

        let running = metrics.num_requests_running.value();
        let running_low = running.is_some_and(|v| v < self.cfg.low_running as f64);
        if running_low {
            signals.push("Very few active requests — no batching benefit".to_string());
            evidence.push(format!("Requests running: {:.0}", running.unwrap()));
        }

        Some(FindingData {
            confidence: if (prompt_low && gen_low) || running_low {
                Confidence::Medium
            } else {
                Confidence::Low
            },
            summary:
                "Server is processing requests below expected throughput with no queue pressure."
                    .to_string(),
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
    fn no_finding_when_throughput_high() {
        assert!(rule().run(&snapshot(100.0, 100.0, 5.0, 0.0)).is_none());
    }

    #[test]
    fn no_finding_when_waiting_exists() {
        assert!(rule().run(&snapshot(5.0, 5.0, 5.0, 1.0)).is_none());
    }

    #[test]
    fn medium_confidence_when_both_low() {
        let finding = rule().run(&snapshot(5.0, 20.0, 5.0, 0.0)).unwrap();
        assert_eq!(finding.confidence, Confidence::Medium);
    }

    #[test]
    fn low_confidence_when_only_one_low() {
        let finding = rule().run(&snapshot(100.0, 20.0, 5.0, 0.0)).unwrap();
        assert_eq!(finding.confidence, Confidence::Low);
    }

    #[test]
    fn running_low_boosts_confidence() {
        let finding = rule().run(&snapshot(100.0, 20.0, 1.0, 0.0)).unwrap();
        assert_eq!(finding.confidence, Confidence::Medium);
    }
}
