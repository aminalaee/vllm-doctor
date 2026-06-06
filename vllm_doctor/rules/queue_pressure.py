"""
Queue pressure rule.

Detects when requests are accumulating faster than the server can process them.

Signals (each matching signal increases confidence):
  - num_requests_waiting > threshold: requests are queued, server is backlogged
  - num_requests_running at high concurrency: server is saturated, not idle

Confidence:
  1 signal → low
  2 signals → high
"""

from vllm_doctor.config import QueuePressureConfig
from vllm_doctor.metrics import MetricSeriesSnapshot
from vllm_doctor.models import Confidence, FindingData, Severity
from vllm_doctor.rules.base import Rule


class QueuePressureRule(Rule[QueuePressureConfig]):
    id = "queue_pressure"
    name = "Queue Pressure"
    title = "Queue pressure"
    severity = Severity.warning
    config_attr = "queue_pressure"
    config_cls = QueuePressureConfig
    likely_causes = [
        "Insufficient replica capacity for current traffic",
        "Autoscaling has not reacted yet",
        "Long-context requests consuming disproportionate compute",
    ]
    recommendations = [
        "Add replicas or increase concurrency limits",
        "Inspect autoscaling thresholds",
        "Separate long-context traffic to a dedicated replica",
        "Reduce incoming request rate",
    ]
    related_metrics = ["vllm:num_requests_waiting", "vllm:num_requests_running"]

    def run(self, metrics: MetricSeriesSnapshot) -> FindingData | None:
        waiting = metrics.num_requests_waiting.value()
        if waiting is None or waiting <= self.cfg.high_waiting:
            return None

        running = metrics.num_requests_running.value()
        running_high = running is not None and running > self.cfg.high_running

        signals: list[str] = []
        evidence = [f"Waiting requests: {waiting:.0f} (threshold: {self.cfg.high_waiting})"]
        if running_high:
            signals.append("Queue pressure compounding with server saturation")
            evidence.append(f"Running requests: {running:.0f} (threshold: {self.cfg.high_running})")

        return FindingData(
            confidence=Confidence.high if running_high else Confidence.low,
            summary="Requests are queuing faster than the server can process them.",
            signals=signals,
            evidence=evidence,
        )
