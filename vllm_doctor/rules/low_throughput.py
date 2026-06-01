"""
Low throughput rule.

Detects when the server is processing requests below expected throughput with
no queue pressure. This indicates the server is underutilized — not saturated —
which points to low incoming load, poor batching, or misconfigured concurrency.

Signals (each matching signal increases confidence):
  - prompt_tokens_per_second below threshold: prefill throughput is low
  - generation_tokens_per_second below threshold: decode throughput is low
  - num_requests_running very low: few active requests, no batching benefit

Suppressed when requests are waiting — low throughput with a queue is a
capacity problem (queue pressure), not an underutilization problem.

Confidence:
  both prompt and gen low, or running very low  → medium
  only one metric low                           → low
"""

from typing import TYPE_CHECKING

from vllm_doctor.models import Confidence, FindingData, Metrics, Severity
from vllm_doctor.rules.base import Rule

if TYPE_CHECKING:
    from vllm_doctor.config import RulesConfig
DEFAULT_LOW_PROMPT_TPS = 10.0
DEFAULT_LOW_GEN_TPS = 50.0
DEFAULT_LOW_RUNNING = 2


class LowThroughputRule(Rule):
    id = "low_throughput"
    name = "Low Throughput"
    title = "Low throughput"
    severity = Severity.warning
    likely_causes = [
        "Low incoming request rate — server is idle",
        "Poor batching due to few concurrent requests",
        "Suboptimal max_num_seqs or max_num_batched_tokens for current load",
    ]
    recommendations = [
        "Increase concurrent requests to improve batching efficiency",
        "Review max_num_seqs and max_num_batched_tokens settings",
        "Compare against benchmark baseline to confirm underperformance",
        "Consider consolidating replicas if load is consistently low",
    ]
    related_metrics = [
        "vllm:prompt_tokens_per_second",
        "vllm:generation_tokens_per_second",
        "vllm:num_requests_running",
    ]

    def __init__(
        self,
        low_prompt_tps: float = DEFAULT_LOW_PROMPT_TPS,
        low_gen_tps: float = DEFAULT_LOW_GEN_TPS,
        low_running: int = DEFAULT_LOW_RUNNING,
    ) -> None:
        self.low_prompt_tps = low_prompt_tps
        self.low_gen_tps = low_gen_tps
        self.low_running = low_running

    @classmethod
    def from_config(cls, config: "RulesConfig") -> "LowThroughputRule":
        return cls(
            low_prompt_tps=config.low_throughput.low_prompt_tps,
            low_gen_tps=config.low_throughput.low_gen_tps,
            low_running=config.low_throughput.low_running,
        )

    def run(self, metrics: Metrics) -> FindingData | None:
        if metrics.prompt_tokens_per_second is None and metrics.generation_tokens_per_second is None:
            return None

        prompt_low = (
            metrics.prompt_tokens_per_second is not None and metrics.prompt_tokens_per_second < self.low_prompt_tps
        )
        gen_low = (
            metrics.generation_tokens_per_second is not None and metrics.generation_tokens_per_second < self.low_gen_tps
        )

        if not prompt_low and not gen_low:
            return None

        # Suppress when requests are waiting — that's queue pressure, not underutilization
        if metrics.num_requests_waiting is not None and metrics.num_requests_waiting > 0:
            return None

        signals: list[str] = []
        evidence: list[str] = []

        if prompt_low and gen_low:
            signals.append("Both prefill and decode throughput below threshold — server underutilized")
        elif prompt_low:
            signals.append("Prefill throughput below threshold")
        else:
            signals.append("Decode throughput below threshold")

        if metrics.prompt_tokens_per_second is not None:
            evidence.append(
                f"Prompt tokens/s: {metrics.prompt_tokens_per_second:.1f} (threshold: {self.low_prompt_tps:.1f})"
            )
        if metrics.generation_tokens_per_second is not None:
            evidence.append(
                f"Generation tokens/s: {metrics.generation_tokens_per_second:.1f} (threshold: {self.low_gen_tps:.1f})"
            )

        running_low = metrics.num_requests_running is not None and metrics.num_requests_running < self.low_running
        if running_low:
            signals.append("Very few active requests — no batching benefit")
            evidence.append(f"Requests running: {metrics.num_requests_running:.0f}")

        return FindingData(
            confidence=Confidence.medium if (prompt_low and gen_low) or running_low else Confidence.low,
            summary="Server is processing requests below expected throughput with no queue pressure.",
            signals=signals,
            evidence=evidence,
        )
