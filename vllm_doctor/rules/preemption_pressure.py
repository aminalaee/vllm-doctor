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

from vllm_doctor.models import Confidence, Finding, MetricSnapshot, Severity
from vllm_doctor.rules.base import Rule

_DEFAULT_HIGH_CACHE_USAGE = 0.80


class PreemptionPressureRule(Rule):
    @property
    def name(self) -> str:
        return "Preemption Pressure"

    def __init__(self, high_cache_usage: float = _DEFAULT_HIGH_CACHE_USAGE) -> None:
        self.high_cache_usage = high_cache_usage

    def evaluate(self, snapshot: MetricSnapshot) -> list[Finding]:
        preemptions = snapshot.metrics.num_preemptions_total
        if preemptions is None or preemptions == 0:
            return []

        evidence = [f"Preemptions total: {preemptions:.0f}"]
        signals: list[str] = []

        cache_high = (
            snapshot.metrics.kv_cache_usage_perc is not None
            and snapshot.metrics.kv_cache_usage_perc >= self.high_cache_usage
        )
        if cache_high:
            signals.append("KV cache under pressure while preemptions are occurring")
            evidence.append(
                f"GPU KV cache usage: {snapshot.metrics.kv_cache_usage_perc:.0%} "
                f"(threshold: {self.high_cache_usage:.0%})"
            )

        confidence = Confidence.high if cache_high else Confidence.medium

        return [
            Finding(
                severity=Severity.warning,
                confidence=confidence,
                title="Preemption pressure",
                summary=(
                    f"vLLM has preempted {preemptions:.0f} sequences — "
                    "KV cache exhaustion is forcing sequences to be re-computed."
                ),
                signals=signals,
                evidence=evidence,
                likely_causes=[
                    "KV cache too small for the concurrent request mix",
                    "Long-context requests exhausting cache before shorter ones complete",
                    "max_num_seqs set too high relative to available GPU memory",
                ],
                recommendations=[
                    "Reduce max_num_seqs to limit concurrent sequences in GPU memory",
                    "Reduce max_num_batched_tokens to lower per-step memory pressure",
                    "Increase gpu_memory_utilization if GPU headroom exists",
                    "Route long-context requests to a dedicated replica",
                ],
                related_metrics=[
                    "vllm:num_preemptions_total",
                    "vllm:kv_cache_usage_perc",
                ],
            )
        ]
