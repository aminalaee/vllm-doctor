"""
KV cache pressure rule.

Detects when GPU KV cache is near exhaustion. When the cache fills up, vLLM
cannot admit new sequences — requests stall in the waiting queue even if GPU
compute is otherwise available. This is the most common cause of latency spikes
under long-context or high-concurrency workloads.

Signals (each matching signal increases confidence):
  - kv_cache_usage_perc >= threshold: cache is critically full
  - num_requests_waiting > 0: cache pressure is already causing queuing

Confidence:
  cache signal only  → medium (pressure exists, queuing not yet observed)
  both signals       → high   (cache is full and actively blocking requests)
"""

from typing import TYPE_CHECKING

from vllm_doctor.models import Confidence, FindingData, Metrics, Severity
from vllm_doctor.rules.base import Rule

if TYPE_CHECKING:
    from vllm_doctor.config import RulesConfig
from vllm_doctor.rules.trend import rising

DEFAULT_HIGH_CACHE_USAGE = 0.90


class KVCachePressureRule(Rule):
    name = "KV Cache Pressure"
    title = "KV cache pressure"
    severity = Severity.critical
    likely_causes = [
        "Long-context requests holding large KV cache allocations",
        "max_num_seqs or max_num_batched_tokens set too high for available GPU memory",
        "Sudden spike in concurrent requests",
    ]
    recommendations = [
        "Reduce max_num_seqs to limit concurrent sequences",
        "Reduce max_num_batched_tokens to cap memory per step",
        "Increase gpu_memory_utilization if GPU memory headroom exists",
        "Route long-context requests to a dedicated replica",
    ]
    related_metrics = ["vllm:kv_cache_usage_perc", "vllm:num_requests_waiting"]

    def __init__(self, high_cache_usage: float = DEFAULT_HIGH_CACHE_USAGE) -> None:
        self.high_cache_usage = high_cache_usage

    @classmethod
    def from_config(cls, config: "RulesConfig") -> "KVCachePressureRule":
        return cls(high_cache_usage=config.kv_cache_pressure.high_cache_usage)

    def _run(self, current: Metrics, previous: Metrics | None) -> FindingData | None:
        if current.kv_cache_usage_perc is None or current.kv_cache_usage_perc < self.high_cache_usage:
            return None

        signals: list[str] = []
        evidence = [f"GPU KV cache usage: {current.kv_cache_usage_perc:.0%} (threshold: {self.high_cache_usage:.0%})"]

        waiting_high = current.num_requests_waiting is not None and current.num_requests_waiting > 0
        if waiting_high:
            signals.append("Cache saturation blocking new request admission")
            evidence.append(f"Waiting requests: {current.num_requests_waiting:.0f} (blocked by full cache)")

        if previous is not None and rising(current.kv_cache_usage_perc, previous.kv_cache_usage_perc):
            prev_c, curr_c = previous.kv_cache_usage_perc, current.kv_cache_usage_perc
            signals.append(f"KV cache usage rising ({prev_c:.0%} → {curr_c:.0%})")

        return FindingData(
            confidence=Confidence.high if waiting_high else Confidence.medium,
            summary=(
                f"GPU KV cache at {current.kv_cache_usage_perc:.0%} — "
                "new requests cannot be admitted until sequences complete."
            ),
            signals=signals,
            evidence=evidence,
        )
