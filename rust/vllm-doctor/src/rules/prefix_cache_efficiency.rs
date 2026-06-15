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
use crate::models::{DiagnosisState, Severity};
use crate::rules::Rule;
use crate::signals::{Signal, SignalGraph};

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

    fn severity(&self) -> Severity {
        Severity::Warning
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

    fn run(&self, signals: &SignalGraph<'_>) -> DiagnosisState {
        let Some(hit_rate) = signals.evaluate(Signal::PrefixCacheHitRate) else {
            return DiagnosisState::unknown_signal(Signal::PrefixCacheHitRate);
        };

        if hit_rate >= self.cfg.min_hit_rate {
            return DiagnosisState::Healthy;
        }

        DiagnosisState::Stressed(Signal::PrefixCacheHitRate, hit_rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::DiagnosisState;
    use crate::signals::{Signal, SignalGraph};

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
    fn healthy_when_hit_rate_high() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.80))),
            DiagnosisState::Healthy
        );
    }

    #[test]
    fn stressed_when_hit_rate_low() {
        assert_eq!(
            rule().run(&SignalGraph::new(&snapshot(0.30))),
            DiagnosisState::Stressed(Signal::PrefixCacheHitRate, 0.30)
        );
    }
}
