"""
TPOT bottleneck rule.

Detects when time per output token (p95) exceeds the configured threshold.
Confidence rises when generation throughput is also low — corroborating decode
pressure — and when TTFT is not elevated, isolating the bottleneck to decode
rather than prefill or queue saturation.
"""

import math
from typing import TYPE_CHECKING

from vllm_doctor.models import Confidence, FindingData, Metrics, Severity
from vllm_doctor.rules.base import Rule

if TYPE_CHECKING:
    from vllm_doctor.config import RulesConfig
_DEFAULT_HIGH_TPOT_P95 = 0.2
_DEFAULT_LOW_GEN_TOKENS_PER_SEC = 50.0


class TPOTBottleneckRule(Rule):
    name = "High TPOT"
    title = "High time per output token (TPOT)"
    severity = Severity.warning
    likely_causes = [
        "GPU memory bandwidth saturated during decode",
        "Too many concurrent sequences reducing per-request throughput",
        "Large model size relative to available GPU memory",
        "Insufficient tensor parallelism for current load",
    ]
    recommendations = [
        "Reduce max concurrent requests (--max-num-seqs)",
        "Increase tensor parallelism to distribute decode across GPUs",
        "Enable speculative decoding to amortize decode cost",
        "Profile GPU memory bandwidth utilization",
    ]
    related_metrics = ["tpot_p95_seconds", "generation_tokens_per_second", "ttft_p95_seconds"]

    def __init__(
        self,
        high_tpot_p95: float = _DEFAULT_HIGH_TPOT_P95,
        low_gen_tokens_per_sec: float = _DEFAULT_LOW_GEN_TOKENS_PER_SEC,
    ) -> None:
        self.high_tpot_p95 = high_tpot_p95
        self.low_gen_tokens_per_sec = low_gen_tokens_per_sec

    @classmethod
    def from_config(cls, config: "RulesConfig") -> "TPOTBottleneckRule":
        return cls(
            high_tpot_p95=config.tpot_bottleneck.high_tpot_p95,
            low_gen_tokens_per_sec=config.tpot_bottleneck.low_gen_tokens_per_sec,
        )

    def run(self, metrics: Metrics) -> FindingData | None:
        tpot = metrics.tpot_p95_seconds
        if tpot is None or not math.isfinite(tpot) or tpot < self.high_tpot_p95:
            return None

        ttft = metrics.ttft_p95_seconds
        gen = metrics.generation_tokens_per_second

        signals = [f"TPOT p95 ({tpot:.2f}s) exceeds threshold ({self.high_tpot_p95}s)"]
        evidence = [f"TPOT p95: {tpot:.3f}s"]

        gen_low = gen is not None and math.isfinite(gen) and gen < self.low_gen_tokens_per_sec
        ttft_normal = ttft is not None and math.isfinite(ttft) and ttft < 2.0

        if gen is not None and math.isfinite(gen):
            evidence.append(f"Generation throughput: {gen:.1f} tok/s")
        if gen_low:
            signals.append(f"Generation throughput ({gen:.1f} tok/s) is low — decode is the bottleneck")
        if ttft is not None and math.isfinite(ttft):
            evidence.append(f"TTFT p95: {ttft:.3f}s")
        if ttft_normal:
            signals.append("TTFT p95 is normal — bottleneck is in decode, not prefill")

        signals_count = sum([True, gen_low, ttft_normal])
        if signals_count >= 3:
            confidence = Confidence.high
        elif signals_count == 2:
            confidence = Confidence.medium
        else:
            confidence = Confidence.low

        return FindingData(
            confidence=confidence,
            summary=(
                "Each output token is taking too long to generate. "
                "This typically indicates GPU decode saturation or memory bandwidth pressure."
            ),
            signals=signals,
            evidence=evidence,
        )
