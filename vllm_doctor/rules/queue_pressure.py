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

from vllm_doctor.models import Confidence, Finding, MetricSnapshot, Severity
from vllm_doctor.rules.base import Rule

DEFAULT_HIGH_WAITING = 5
DEFAULT_HIGH_RUNNING = 50


class QueuePressureRule(Rule):
    def __init__(
        self,
        high_waiting: int = DEFAULT_HIGH_WAITING,
        high_running: int = DEFAULT_HIGH_RUNNING,
    ) -> None:
        self.high_waiting = high_waiting
        self.high_running = high_running

    def evaluate(self, snapshot: MetricSnapshot) -> list[Finding]:
        waiting_high = (
            snapshot.metrics.num_requests_waiting is not None
            and snapshot.metrics.num_requests_waiting > self.high_waiting
        )

        if not waiting_high:
            return []

        signals: list[str] = []
        evidence = [
            f"Waiting requests: {snapshot.metrics.num_requests_waiting:.0f} (threshold: {self.high_waiting})"
        ]

        running_high = (
            snapshot.metrics.num_requests_running is not None
            and snapshot.metrics.num_requests_running > self.high_running
        )
        if running_high:
            signals.append("Queue pressure compounding with server saturation")
            evidence.append(
                f"Running requests: {snapshot.metrics.num_requests_running:.0f} (threshold: {self.high_running})"
            )

        confidence = Confidence.high if running_high else Confidence.low

        return [
            Finding(
                severity=Severity.warning,
                confidence=confidence,
                title="Queue pressure",
                summary="Requests are queuing faster than the server can process them.",
                signals=signals,
                evidence=evidence,
                likely_causes=[
                    "Insufficient replica capacity for current traffic",
                    "Autoscaling has not reacted yet",
                    "Long-context requests consuming disproportionate compute",
                ],
                recommendations=[
                    "Add replicas or increase concurrency limits",
                    "Inspect autoscaling thresholds",
                    "Separate long-context traffic to a dedicated replica",
                    "Reduce incoming request rate",
                ],
                related_metrics=[
                    "vllm:num_requests_waiting",
                    "vllm:num_requests_running",
                ],
            )
        ]
