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

from vllm_doctor.models import Confidence, FindingData, Metrics, Severity
from vllm_doctor.rules.base import Rule
from vllm_doctor.rules.trend import falling

_DEFAULT_MIN_HIT_RATE = 0.5
_HIGH_CONFIDENCE_MAX_RATE = 0.2


class PrefixCacheEfficiencyRule(Rule):
    name = "Prefix Cache Efficiency"
    title = "Low prefix cache hit rate"
    severity = Severity.warning
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

    def __init__(self, min_hit_rate: float = _DEFAULT_MIN_HIT_RATE) -> None:
        self.min_hit_rate = min_hit_rate

    def _run(self, current: Metrics, previous: Metrics | None) -> FindingData | None:
        hit_rate = current.prefix_cache_hit_rate
        if hit_rate is None or hit_rate >= self.min_hit_rate:
            return None

        signals: list[str] = []
        if previous is not None and falling(hit_rate, previous.prefix_cache_hit_rate):
            signals.append(f"Prefix cache hit rate declining ({previous.prefix_cache_hit_rate:.0%} → {hit_rate:.0%})")

        return FindingData(
            confidence=Confidence.high if hit_rate < _HIGH_CONFIDENCE_MAX_RATE else Confidence.medium,
            signals=signals,
            summary=(
                f"Prefix cache hit rate is {hit_rate:.0%} — repeated prompt prefixes "
                "are not being reused, causing redundant prefill computation."
            ),
            evidence=[f"Prefix cache hit rate: {hit_rate:.0%}"],
        )
