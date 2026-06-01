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

from typing import TYPE_CHECKING

from vllm_doctor.models import Confidence, FindingData, Metrics, Severity
from vllm_doctor.rules.base import Rule

if TYPE_CHECKING:
    from vllm_doctor.config import RulesConfig
DEFAULT_HIGH_WAITING = 5
DEFAULT_HIGH_RUNNING = 50


class QueuePressureRule(Rule):
    name = "Queue Pressure"
    title = "Queue pressure"
    severity = Severity.warning
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

    def __init__(
        self,
        high_waiting: int = DEFAULT_HIGH_WAITING,
        high_running: int = DEFAULT_HIGH_RUNNING,
    ) -> None:
        self.high_waiting = high_waiting
        self.high_running = high_running

    @classmethod
    def from_config(cls, config: "RulesConfig") -> "QueuePressureRule":
        return cls(high_waiting=config.queue_pressure.high_waiting, high_running=config.queue_pressure.high_running)

    def run(self, metrics: Metrics) -> FindingData | None:
        if metrics.num_requests_waiting is None or metrics.num_requests_waiting <= self.high_waiting:
            return None

        signals: list[str] = []
        evidence = [f"Waiting requests: {metrics.num_requests_waiting:.0f} (threshold: {self.high_waiting})"]

        running_high = metrics.num_requests_running is not None and metrics.num_requests_running > self.high_running
        if running_high:
            signals.append("Queue pressure compounding with server saturation")
            evidence.append(f"Running requests: {metrics.num_requests_running:.0f} (threshold: {self.high_running})")

        return FindingData(
            confidence=Confidence.high if running_high else Confidence.low,
            summary="Requests are queuing faster than the server can process them.",
            signals=signals,
            evidence=evidence,
        )
