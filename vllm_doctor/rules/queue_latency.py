"""
Queue latency rule.

Detects when requests are spending too long in the waiting queue before prefill begins.

Unlike QueuePressureRule (which counts waiting requests), this rule measures actual
queue latency from the histogram — a direct signal of how long clients wait before
their request even starts processing.

Signals (each matching signal increases confidence):
  - queue_time_p95_seconds >= threshold: requests are spending too long queued
  - num_requests_waiting > 0: active backlog confirmed

Confidence:
  high queue time only      → low  (spike may be transient)
  high queue time + waiting → high (active backlog confirmed)
"""

import math

from vllm_doctor.config import QueueLatencyConfig
from vllm_doctor.metrics import MetricSeriesSnapshot
from vllm_doctor.models import Confidence, FindingData, Severity
from vllm_doctor.rules.base import Rule


class QueueLatencyRule(Rule[QueueLatencyConfig]):
    id = "queue_latency"
    name = "Queue Latency"
    title = "High queue latency"
    severity = Severity.warning
    config_attr = "queue_latency"
    config_cls = QueueLatencyConfig
    likely_causes = [
        "Insufficient replica capacity for current request rate",
        "Long-context requests blocking admission of new sequences",
        "Autoscaling has not reacted to traffic increase",
        "KV cache exhaustion limiting sequence admission",
    ]
    recommendations = [
        "Add replicas or increase concurrency limits",
        "Inspect autoscaling thresholds and reaction time",
        "Correlate with KV cache pressure — reduce max_num_seqs if cache is full",
        "Separate long-context traffic to a dedicated replica",
    ]
    related_metrics = ["vllm:request_queue_time_seconds", "vllm:num_requests_waiting"]

    def run(self, metrics: MetricSeriesSnapshot) -> FindingData | None:
        queue_time = metrics.queue_time_p95_seconds.value()
        if queue_time is None or not math.isfinite(queue_time) or queue_time < self.cfg.high_queue_time_p95:
            return None

        evidence = [f"Queue time p95: {queue_time:.3f}s (threshold: {self.cfg.high_queue_time_p95}s)"]
        signals: list[str] = []

        waiting = metrics.num_requests_waiting.value()
        waiting_confirmed = waiting is not None and waiting > 0
        if waiting_confirmed:
            signals.append(f"{int(waiting)} requests queued — active backlog confirmed")
            evidence.append(f"Waiting requests: {int(waiting)}")

        return FindingData(
            confidence=Confidence.high if waiting_confirmed else Confidence.low,
            summary=(
                f"Requests are waiting {queue_time:.2f}s (p95) in the queue before "
                "prefill begins — the server cannot admit requests fast enough."
            ),
            signals=signals,
            evidence=evidence,
        )
