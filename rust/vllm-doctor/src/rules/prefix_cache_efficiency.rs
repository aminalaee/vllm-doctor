//! Prefix cache efficiency rule.
//!
//! Detects when the prefix (KV) cache hit rate is low despite queries being made.
//! A low hit rate means repeated prompt prefixes — system prompts, few-shot examples —
//! are not being reused, causing redundant prefill computation on every request.
//!
//! Signals:
//!   - prefix_cache_hit_rate < threshold: cache queries are not being served from cache
//!
//! Confidence:
//!   large sample + very low rate  → high
//!   otherwise                     → medium
use crate::config::PrefixCacheEfficiencyConfig;
use crate::metrics::MetricSeriesSnapshot;
use crate::models::{Confidence, FindingData};
use crate::rules::Rule;

const HIGH_CONFIDENCE_MAX_RATE: f64 = 0.2;

pub struct PrefixCacheEfficiencyRule {
    cfg: PrefixCacheEfficiencyConfig,
}

impl PrefixCacheEfficiencyRule {
    pub fn new(cfg: PrefixCacheEfficiencyConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for PrefixCacheEfficiencyRule {
    fn id(&self) -> &'static str {
        "prefix_cache_efficiency"
    }

    fn name(&self) -> &'static str {
        "Prefix Cache Efficiency"
    }

    fn title(&self) -> &'static str {
        "Low prefix cache hit rate"
    }

    fn severity(&self) -> crate::models::Severity {
        crate::models::Severity::Warning
    }

    fn likely_causes(&self) -> &'static [&'static str] {
        &[
            "Requests do not share common prefixes (system prompts, few-shot examples)",
            "Prefix caching not enabled (--enable-prefix-caching not set)",
            "Cache eviction too aggressive for the workload",
        ]
    }

    fn recommendations(&self) -> &'static [&'static str] {
        &[
            "Enable prefix caching: add --enable-prefix-caching to vLLM startup",
            "Ensure requests share a common system prompt or few-shot prefix",
            "Review prefix_caching_hash_algo if cache collisions are suspected",
        ]
    }

    fn related_metrics(&self) -> &'static [&'static str] {
        &[
            "vllm:prefix_cache_hits_total",
            "vllm:prefix_cache_queries_total",
        ]
    }

    fn run(&self, metrics: &MetricSeriesSnapshot) -> Option<FindingData> {
        let hit_rate = metrics.prefix_cache_hit_rate.value()?;
        if hit_rate >= self.cfg.min_hit_rate {
            return None;
        }

        Some(FindingData {
            confidence: if hit_rate < HIGH_CONFIDENCE_MAX_RATE {
                Confidence::High
            } else {
                Confidence::Medium
            },
            summary: format!(
                "Prefix cache hit rate is {:.0}% — repeated prompt prefixes are not being reused, causing redundant prefill computation.",
                hit_rate * 100.0
            ),
            signals: Vec::new(),
            evidence: vec![format!("Prefix cache hit rate: {:.0}%", hit_rate * 100.0)],
            severity: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::series::{MetricSample, MetricSeries};

    fn rule() -> PrefixCacheEfficiencyRule {
        PrefixCacheEfficiencyRule::new(PrefixCacheEfficiencyConfig { min_hit_rate: 0.50 })
    }

    fn snapshot(hit_rate: f64) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            prefix_cache_hit_rate: MetricSeries::from_samples(vec![MetricSample::new(hit_rate)]),
            ..Default::default()
        }
    }

    #[test]
    fn no_finding_when_hit_rate_high() {
        assert!(rule().run(&snapshot(0.80)).is_none());
    }

    #[test]
    fn medium_confidence_when_hit_rate_moderately_low() {
        let finding = rule().run(&snapshot(0.30)).unwrap();
        assert_eq!(finding.confidence, Confidence::Medium);
    }

    #[test]
    fn high_confidence_when_hit_rate_very_low() {
        let finding = rule().run(&snapshot(0.10)).unwrap();
        assert_eq!(finding.confidence, Confidence::High);
    }
}
