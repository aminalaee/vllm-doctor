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

from vllm_doctor.config import LowThroughputConfig
from vllm_doctor.metrics import MetricSeriesSnapshot
from vllm_doctor.models import Confidence, FindingData, Severity
from vllm_doctor.rules.base import Rule


class LowThroughputRule(Rule[LowThroughputConfig]):
    id = "low_throughput"
    name = "Low Throughput"
    title = "Low throughput"
    severity = Severity.warning
    config_attr = "low_throughput"
    config_cls = LowThroughputConfig
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

    def run(self, metrics: MetricSeriesSnapshot) -> FindingData | None:
        prompt = metrics.prompt_tokens_per_second.value()
        gen = metrics.generation_tokens_per_second.value()
        if prompt is None and gen is None:
            return None

        prompt_low = prompt is not None and prompt < self.cfg.low_prompt_tps
        gen_low = gen is not None and gen < self.cfg.low_gen_tps

        if not prompt_low and not gen_low:
            return None

        waiting = metrics.num_requests_waiting.value()
        if waiting is not None and waiting > 0:
            return None

        signals: list[str] = []
        evidence: list[str] = []

        if prompt_low and gen_low:
            signals.append("Both prefill and decode throughput below threshold — server underutilized")
        elif prompt_low:
            signals.append("Prefill throughput below threshold")
        else:
            signals.append("Decode throughput below threshold")

        if prompt is not None:
            evidence.append(f"Prompt tokens/s: {prompt:.1f} (threshold: {self.cfg.low_prompt_tps:.1f})")
        if gen is not None:
            evidence.append(f"Generation tokens/s: {gen:.1f} (threshold: {self.cfg.low_gen_tps:.1f})")

        running = metrics.num_requests_running.value()
        running_low = running is not None and running < self.cfg.low_running
        if running_low:
            signals.append("Very few active requests — no batching benefit")
            evidence.append(f"Requests running: {running:.0f}")

        return FindingData(
            confidence=Confidence.medium if (prompt_low and gen_low) or running_low else Confidence.low,
            summary="Server is processing requests below expected throughput with no queue pressure.",
            signals=signals,
            evidence=evidence,
        )
