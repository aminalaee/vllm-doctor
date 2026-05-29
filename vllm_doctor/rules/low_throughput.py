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

from vllm_doctor.models import Confidence, FindingData, Metrics, Severity
from vllm_doctor.rules.base import Rule
from vllm_doctor.rules.trend import falling

DEFAULT_LOW_PROMPT_TPS = 10.0
DEFAULT_LOW_GEN_TPS = 50.0
DEFAULT_LOW_RUNNING = 2


class LowThroughputRule(Rule):
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

    def _run(self, current: Metrics, previous: Metrics | None) -> FindingData | None:
        if current.prompt_tokens_per_second is None and current.generation_tokens_per_second is None:
            return None

        prompt_low = (
            current.prompt_tokens_per_second is not None and current.prompt_tokens_per_second < self.low_prompt_tps
        )
        gen_low = (
            current.generation_tokens_per_second is not None and current.generation_tokens_per_second < self.low_gen_tps
        )

        if not prompt_low and not gen_low:
            return None

        # Suppress when requests are waiting — that's queue pressure, not underutilization
        if current.num_requests_waiting is not None and current.num_requests_waiting > 0:
            return None

        signals: list[str] = []
        evidence: list[str] = []

        if prompt_low and gen_low:
            signals.append("Both prefill and decode throughput below threshold — server underutilized")
        elif prompt_low:
            signals.append("Prefill throughput below threshold")
        else:
            signals.append("Decode throughput below threshold")

        if current.prompt_tokens_per_second is not None:
            evidence.append(
                f"Prompt tokens/s: {current.prompt_tokens_per_second:.1f} (threshold: {self.low_prompt_tps:.1f})"
            )
        if current.generation_tokens_per_second is not None:
            evidence.append(
                f"Generation tokens/s: {current.generation_tokens_per_second:.1f} (threshold: {self.low_gen_tps:.1f})"
            )

        running_low = current.num_requests_running is not None and current.num_requests_running < self.low_running
        if running_low:
            signals.append("Very few active requests — no batching benefit")
            evidence.append(f"Requests running: {current.num_requests_running:.0f}")

        if previous is not None:
            if gen_low and falling(current.generation_tokens_per_second, previous.generation_tokens_per_second):
                prev_g, curr_g = previous.generation_tokens_per_second, current.generation_tokens_per_second
                signals.append(f"Generation throughput declining ({prev_g:.1f} → {curr_g:.1f} tok/s)")
            elif prompt_low and falling(current.prompt_tokens_per_second, previous.prompt_tokens_per_second):
                prev_p, curr_p = previous.prompt_tokens_per_second, current.prompt_tokens_per_second
                signals.append(f"Prompt throughput declining ({prev_p:.1f} → {curr_p:.1f} tok/s)")

        return FindingData(
            confidence=Confidence.medium if (prompt_low and gen_low) or running_low else Confidence.low,
            summary="Server is processing requests below expected throughput with no queue pressure.",
            signals=signals,
            evidence=evidence,
        )
