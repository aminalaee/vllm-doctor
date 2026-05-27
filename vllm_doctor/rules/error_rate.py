"""
Error rate rule.

Detects elevated server-side errors or client aborts relative to total requests.

vLLM tracks finished requests by reason via vllm:request_success_total:
  - stop      — completed normally
  - error     — server-side failure (OOM, internal error)
  - abort     — client disconnected or request cancelled (often due to latency)
  - length    — hit max_tokens limit (not an error)
  - repetition — stopped by repetition penalty (not an error)

Signals (each matching signal increases confidence):
  - error rate high: server is failing requests internally
  - abort rate high: clients are giving up, often due to slow responses

Confidence:
  error high only, or abort high only  → low
  both high                            → high
"""

from vllm_doctor.models import Confidence, Finding, MetricSnapshot, Severity
from vllm_doctor.rules.base import Rule

DEFAULT_HIGH_ERROR_RATE = 0.05
DEFAULT_HIGH_ABORT_RATE = 0.10


class ErrorRateRule(Rule):
    @property
    def name(self) -> str:
        return "Error Rate"

    def __init__(
        self,
        high_error_rate: float = DEFAULT_HIGH_ERROR_RATE,
        high_abort_rate: float = DEFAULT_HIGH_ABORT_RATE,
    ) -> None:
        self.high_error_rate = high_error_rate
        self.high_abort_rate = high_abort_rate

    def evaluate(self, snapshot: MetricSnapshot) -> list[Finding]:
        errors = snapshot.metrics.request_error_total
        aborts = snapshot.metrics.request_abort_total
        success = snapshot.metrics.request_success_total

        if errors is None and aborts is None:
            return []

        total = (success or 0.0) + (errors or 0.0) + (aborts or 0.0)
        if total == 0:
            return []

        error_rate = (errors or 0.0) / total
        abort_rate = (aborts or 0.0) / total

        errors_high = error_rate >= self.high_error_rate
        aborts_high = abort_rate >= self.high_abort_rate

        if not errors_high and not aborts_high:
            return []

        signals: list[str] = []
        evidence: list[str] = []

        if errors_high:
            signals.append("Elevated server-side error rate")
            evidence.append(
                f"Error rate: {error_rate:.1%} ({errors:.0f} errors out of {total:.0f} requests, "
                f"threshold: {self.high_error_rate:.1%})"
            )
        if aborts_high:
            signals.append(
                "Elevated client abort rate — clients disconnecting before response"
            )
            evidence.append(
                f"Abort rate: {abort_rate:.1%} ({aborts:.0f} aborts out of {total:.0f} requests, "
                f"threshold: {self.high_abort_rate:.1%})"
            )

        confidence = (
            Confidence.high if (errors_high and aborts_high) else Confidence.low
        )

        return [
            Finding(
                severity=Severity.critical if errors_high else Severity.warning,
                confidence=confidence,
                title="Elevated error rate",
                summary=(
                    "Server is returning errors or clients are aborting at an elevated rate."
                ),
                signals=signals,
                evidence=evidence,
                likely_causes=[
                    "Server-side OOM or internal errors under high load",
                    "Requests exceeding timeout limits causing client aborts",
                    "High latency causing clients to disconnect before completion",
                    "Resource exhaustion correlating with KV cache pressure",
                ],
                recommendations=[
                    "Inspect vLLM server logs for error details",
                    "Correlate with KV cache pressure and queue pressure findings",
                    "Check client timeout settings relative to observed TTFT and TPOT",
                    "Reduce load or add replicas if errors correlate with traffic spikes",
                ],
                related_metrics=[
                    "vllm:request_success_total",
                ],
            )
        ]
