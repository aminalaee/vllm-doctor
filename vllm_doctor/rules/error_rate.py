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

from typing import TYPE_CHECKING

from vllm_doctor.models import Confidence, FindingData, Metrics, Severity
from vllm_doctor.rules.base import Rule

if TYPE_CHECKING:
    from vllm_doctor.config import RulesConfig

DEFAULT_HIGH_ERROR_RATE = 0.05
DEFAULT_HIGH_ABORT_RATE = 0.10


class ErrorRateRule(Rule):
    name = "Error Rate"
    title = "Elevated error rate"
    severity = Severity.warning  # overridden to critical when errors_high
    likely_causes = [
        "Server-side OOM or internal errors under high load",
        "Requests exceeding timeout limits causing client aborts",
        "High latency causing clients to disconnect before completion",
        "Resource exhaustion correlating with KV cache pressure",
    ]
    recommendations = [
        "Inspect vLLM server logs for error details",
        "Correlate with KV cache pressure and queue pressure findings",
        "Check client timeout settings relative to observed TTFT and TPOT",
        "Reduce load or add replicas if errors correlate with traffic spikes",
    ]
    related_metrics = ["vllm:request_success_total"]

    def __init__(
        self,
        high_error_rate: float = DEFAULT_HIGH_ERROR_RATE,
        high_abort_rate: float = DEFAULT_HIGH_ABORT_RATE,
    ) -> None:
        self.high_error_rate = high_error_rate
        self.high_abort_rate = high_abort_rate

    @classmethod
    def from_config(cls, config: "RulesConfig") -> "ErrorRateRule":
        return cls(high_error_rate=config.error_rate.high_error_rate, high_abort_rate=config.error_rate.high_abort_rate)

    def _run(self, current: Metrics, previous: Metrics | None) -> FindingData | None:
        errors = current.request_error_total
        aborts = current.request_abort_total
        success = current.request_success_total

        if errors is None and aborts is None:
            return None

        total = (success or 0.0) + (errors or 0.0) + (aborts or 0.0)
        if total == 0:
            return None

        error_rate = (errors or 0.0) / total
        abort_rate = (aborts or 0.0) / total

        errors_high = error_rate >= self.high_error_rate
        aborts_high = abort_rate >= self.high_abort_rate

        if not errors_high and not aborts_high:
            return None

        signals: list[str] = []
        evidence: list[str] = []

        if errors_high:
            signals.append("Elevated server-side error rate")
            evidence.append(
                f"Error rate: {error_rate:.1%} ({errors:.0f} errors out of {total:.0f} requests, "
                f"threshold: {self.high_error_rate:.1%})"
            )
        if aborts_high:
            signals.append("Elevated client abort rate — clients disconnecting before response")
            evidence.append(
                f"Abort rate: {abort_rate:.1%} ({aborts:.0f} aborts out of {total:.0f} requests, "
                f"threshold: {self.high_abort_rate:.1%})"
            )

        return FindingData(
            confidence=Confidence.high if (errors_high and aborts_high) else Confidence.low,
            severity=Severity.critical if errors_high else Severity.warning,
            summary="Server is returning errors or clients are aborting at an elevated rate.",
            signals=signals,
            evidence=evidence,
        )
