"""
TTFT bottleneck rule.

Detects when time to first token (p95) exceeds the configured threshold.
Confidence rises when TPOT is healthy — ruling out a general decode bottleneck
— and when requests are queuing, confirming prefill pressure.
"""

import math

from vllm_doctor.config import TTFTBottleneckConfig
from vllm_doctor.metrics import MetricSeriesSnapshot
from vllm_doctor.models import Confidence, FindingData, Severity
from vllm_doctor.rules.base import Rule


class TTFTBottleneckRule(Rule[TTFTBottleneckConfig]):
    id = "ttft_bottleneck"
    name = "High TTFT"
    title = "High time to first token (TTFT)"
    severity = Severity.warning
    config_attr = "ttft_bottleneck"
    config_cls = TTFTBottleneckConfig
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

    def run(self, metrics: MetricSeriesSnapshot) -> FindingData | None:
        ttft = metrics.ttft_p95_seconds.value()
        if ttft is None or not math.isfinite(ttft) or ttft < self.cfg.high_ttft_p95:
            return None

        tpot = metrics.tpot_p95_seconds.value()
        waiting = metrics.num_requests_waiting.value()

        signals = [f"TTFT p95 ({ttft:.2f}s) exceeds threshold ({self.cfg.high_ttft_p95}s)"]
        evidence = [f"TTFT p95: {ttft:.3f}s"]
        tpot_stable = tpot is not None and math.isfinite(tpot) and tpot < self.cfg.high_tpot_p95

        if tpot is not None and math.isfinite(tpot):
            evidence.append(f"TPOT p95: {tpot:.3f}s")
        if tpot_stable:
            signals.append(f"TPOT p95 ({tpot:.2f}s) is stable — decode is not the bottleneck")
        if waiting is not None and waiting > 0:
            signals.append(f"{int(waiting)} requests queued — prefill pressure confirmed")
            evidence.append(f"Waiting requests: {int(waiting)}")

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
