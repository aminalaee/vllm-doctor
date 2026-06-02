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

from vllm_doctor.metrics import MetricSeriesSnapshot
from vllm_doctor.models import Confidence, FindingData, Severity
from vllm_doctor.rules.base import Rule

if TYPE_CHECKING:
    from vllm_doctor.config import RulesConfig
DEFAULT_HIGH_WAITING = 5
DEFAULT_HIGH_RUNNING = 50


class QueuePressureRule(Rule):
    id = "queue_pressure"
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

    def run(self, metrics: MetricSeriesSnapshot) -> FindingData | None:
        waiting = metrics.num_requests_waiting.value()
        if waiting is None or waiting <= self.high_waiting:
            return None

        running = metrics.num_requests_running.value()
        running_high = running is not None and running > self.high_running

        signals: list[str] = []
        evidence = [f"Waiting requests: {waiting:.0f} (threshold: {self.high_waiting})"]
        if running_high:
            signals.append("Queue pressure compounding with server saturation")
            evidence.append(f"Running requests: {running:.0f} (threshold: {self.high_running})")

        return FindingData(
            confidence=Confidence.high if running_high else Confidence.low,
            summary="Requests are queuing faster than the server can process them.",
            signals=signals,
            evidence=evidence,
        )
