from collections.abc import Sequence

from vllm_doctor.models import Confidence, DiagnosisContext, Metrics, RuleResult, Severity
from vllm_doctor.rules.base import Rule

_SEVERITY_ORDER = {Severity.critical: 0, Severity.warning: 1, Severity.info: 2}
_CONFIDENCE_ORDER = {Confidence.high: 0, Confidence.medium: 1, Confidence.low: 2}


def run(
    context: DiagnosisContext,
    current: Metrics,
    rules: Sequence[Rule],
    previous: Metrics | None = None,
) -> list[RuleResult]:
    results = []
    for rule in rules:
        findings = rule.run(context=context, current=current, previous=previous)
        if findings:
            finding = min(
                findings,
                key=lambda f: (
                    _SEVERITY_ORDER[f.severity],
                    _CONFIDENCE_ORDER[f.confidence],
                ),
            )
        else:
            finding = None
        results.append(RuleResult(name=rule.name, finding=finding))
    return sorted(
        results,
        key=lambda r: (
            _SEVERITY_ORDER.get(r.finding.severity, 99) if r.finding else 99,
            _CONFIDENCE_ORDER.get(r.finding.confidence, 99) if r.finding else 99,
        ),
    )
