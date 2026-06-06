"""
Prefix cache efficiency rule.

Detects when the prefix (KV) cache hit rate is low despite queries being made.
A low hit rate means repeated prompt prefixes — system prompts, few-shot examples —
are not being reused, causing redundant prefill computation on every request.

Signals:
  - prefix_cache_hit_rate < threshold: cache queries are not being served from cache

Confidence:
  large sample + very low rate  → high
  otherwise                     → medium
"""

from vllm_doctor.config import PrefixCacheEfficiencyConfig
from vllm_doctor.metrics import MetricSeriesSnapshot
from vllm_doctor.models import Confidence, FindingData, Severity
from vllm_doctor.rules.base import Rule

_HIGH_CONFIDENCE_MAX_RATE = 0.2


class PrefixCacheEfficiencyRule(Rule[PrefixCacheEfficiencyConfig]):
    id = "prefix_cache_efficiency"
    name = "Prefix Cache Efficiency"
    title = "Low prefix cache hit rate"
    severity = Severity.warning
    config_attr = "prefix_cache_efficiency"
    config_cls = PrefixCacheEfficiencyConfig
    likely_causes = [
        "Requests do not share common prefixes (system prompts, few-shot examples)",
        "Prefix caching not enabled (--enable-prefix-caching not set)",
        "Cache eviction too aggressive for the workload",
    ]
    recommendations = [
        "Enable prefix caching: add --enable-prefix-caching to vLLM startup",
        "Ensure requests share a common system prompt or few-shot prefix",
        "Review prefix_caching_hash_algo if cache collisions are suspected",
    ]
    related_metrics = ["vllm:prefix_cache_hits_total", "vllm:prefix_cache_queries_total"]

    def run(self, metrics: MetricSeriesSnapshot) -> FindingData | None:
        hit_rate = metrics.prefix_cache_hit_rate.value()
        if hit_rate is None or hit_rate >= self.cfg.min_hit_rate:
            return None

        return FindingData(
            confidence=Confidence.high if hit_rate < _HIGH_CONFIDENCE_MAX_RATE else Confidence.medium,
            summary=(
                f"Prefix cache hit rate is {hit_rate:.0%} — repeated prompt prefixes "
                "are not being reused, causing redundant prefill computation."
            ),
            evidence=[f"Prefix cache hit rate: {hit_rate:.0%}"],
        )
