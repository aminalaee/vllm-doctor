"""
TTFT bottleneck rule.

Detects when time to first token (p95) exceeds the configured threshold.
Confidence rises when TPOT is healthy — ruling out a general decode bottleneck
— and when requests are queuing, confirming prefill pressure.
"""

import math
from typing import TYPE_CHECKING

from vllm_doctor.models import Confidence, FindingData, Metrics, Severity
from vllm_doctor.rules.base import Rule

if TYPE_CHECKING:
    from vllm_doctor.config import RulesConfig
from vllm_doctor.rules.trend import rising

_DEFAULT_HIGH_TTFT_P95 = 2.0
_DEFAULT_HIGH_TPOT_P95 = 0.2


class TTFTBottleneckRule(Rule):
    name = "High TTFT"
    title = "High time to first token (TTFT)"
    severity = Severity.warning
    likely_causes = [
        "Long input prompts increasing prefill time",
        "Queue pressure delaying prefill start",
        "Chunked prefill not enabled or misconfigured",
        "Insufficient capacity for current prompt load",
    ]
    recommendations = [
        "Enable or tune chunked prefill (--enable-chunked-prefill)",
        "Reduce max prompt length or filter long requests",
        "Inspect queue depth — consider adding replicas",
        "Separate long-context traffic to dedicated instances",
    ]
    related_metrics = ["ttft_p95_seconds", "num_requests_waiting", "tpot_p95_seconds"]

    def __init__(
        self,
        high_ttft_p95: float = _DEFAULT_HIGH_TTFT_P95,
        high_tpot_p95: float = _DEFAULT_HIGH_TPOT_P95,
    ) -> None:
        self.high_ttft_p95 = high_ttft_p95
        self.high_tpot_p95 = high_tpot_p95

    @classmethod
    def from_config(cls, config: "RulesConfig") -> "TTFTBottleneckRule":
        return cls(
            high_ttft_p95=config.ttft_bottleneck.high_ttft_p95,
            high_tpot_p95=config.ttft_bottleneck.high_tpot_p95,
        )

    def _run(self, current: Metrics, previous: Metrics | None) -> FindingData | None:
        ttft = current.ttft_p95_seconds
        if ttft is None or not math.isfinite(ttft) or ttft < self.high_ttft_p95:
            return None

        tpot = current.tpot_p95_seconds
        waiting = current.num_requests_waiting

        signals = [f"TTFT p95 ({ttft:.2f}s) exceeds threshold ({self.high_ttft_p95}s)"]
        evidence = [f"TTFT p95: {ttft:.3f}s"]
        tpot_stable = tpot is not None and math.isfinite(tpot) and tpot < self.high_tpot_p95

        if tpot is not None and math.isfinite(tpot):
            evidence.append(f"TPOT p95: {tpot:.3f}s")
        if tpot_stable:
            signals.append(f"TPOT p95 ({tpot:.2f}s) is stable — decode is not the bottleneck")
        if waiting is not None and waiting > 0:
            signals.append(f"{int(waiting)} requests queued — prefill pressure confirmed")
            evidence.append(f"Waiting requests: {int(waiting)}")

        if previous is not None and rising(ttft, previous.ttft_p95_seconds):
            signals.append(f"TTFT p95 rising ({previous.ttft_p95_seconds:.2f}s → {ttft:.2f}s)")

        signals_count = sum([True, tpot_stable, waiting is not None and waiting > 0])
        if signals_count >= 3:
            confidence = Confidence.high
        elif signals_count == 2:
            confidence = Confidence.medium
        else:
            confidence = Confidence.low

        return FindingData(
            confidence=confidence,
            summary=(
                "Requests are waiting too long before receiving the first token. "
                "This typically indicates prefill or queue pressure."
            ),
            signals=signals,
            evidence=evidence,
        )
