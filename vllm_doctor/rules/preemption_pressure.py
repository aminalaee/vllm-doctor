"""
Preemption pressure rule.

Detects when vLLM has preempted sequences due to KV cache exhaustion.

Preemption happens when a running sequence must be evicted from GPU KV cache to free
space for another. The evicted sequence is re-computed later, wasting GPU cycles and
adding latency. Any preemptions indicate the server ran out of KV cache at least once.

Signals (each matching signal increases confidence):
  - num_preemptions_total > 0: preemptions have occurred
  - kv_cache_usage_perc >= threshold: cache is currently under pressure

Confidence:
  preemptions only               → medium (happened at some point, may not be ongoing)
  preemptions + high cache usage → high   (actively under memory pressure)
"""

from typing import TYPE_CHECKING

from vllm_doctor.models import Confidence, FindingData, Metrics, Severity
from vllm_doctor.rules.base import Rule

if TYPE_CHECKING:
    from vllm_doctor.config import RulesConfig

_DEFAULT_HIGH_CACHE_USAGE = 0.80


class PreemptionPressureRule(Rule):
    name = "Preemption Pressure"
    title = "Preemption pressure"
    severity = Severity.warning
    likely_causes = [
        "KV cache too small for the concurrent request mix",
        "Long-context requests exhausting cache before shorter ones complete",
        "max_num_seqs set too high relative to available GPU memory",
    ]
    recommendations = [
        "Reduce max_num_seqs to limit concurrent sequences in GPU memory",
        "Reduce max_num_batched_tokens to lower per-step memory pressure",
        "Increase gpu_memory_utilization if GPU headroom exists",
        "Route long-context requests to a dedicated replica",
    ]
    related_metrics = ["vllm:num_preemptions_total", "vllm:kv_cache_usage_perc"]

    def __init__(self, high_cache_usage: float = _DEFAULT_HIGH_CACHE_USAGE) -> None:
        self.high_cache_usage = high_cache_usage

    @classmethod
    def from_config(cls, config: "RulesConfig") -> "PreemptionPressureRule":
        return cls(high_cache_usage=config.preemption_pressure.high_cache_usage)

    def _run(self, current: Metrics, previous: Metrics | None) -> FindingData | None:
        preemptions = current.num_preemptions_total
        if preemptions is None or preemptions == 0:
            return None

        evidence = [f"Preemptions total: {preemptions:.0f}"]
        signals: list[str] = []

        cache_high = current.kv_cache_usage_perc is not None and current.kv_cache_usage_perc >= self.high_cache_usage
        if cache_high:
            signals.append("KV cache under pressure while preemptions are occurring")
            evidence.append(
                f"GPU KV cache usage: {current.kv_cache_usage_perc:.0%} (threshold: {self.high_cache_usage:.0%})"
            )

        return FindingData(
            confidence=Confidence.high if cache_high else Confidence.medium,
            summary=(
                f"vLLM has preempted {preemptions:.0f} sequences — "
                "KV cache exhaustion is forcing sequences to be re-computed."
            ),
            signals=signals,
            evidence=evidence,
        )
