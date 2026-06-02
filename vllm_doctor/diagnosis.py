from collections.abc import Sequence

from vllm_doctor.metrics import MetricSeriesSnapshot
from vllm_doctor.models import Confidence, Finding, FindingData, RuleResult, Severity
from vllm_doctor.rules.base import Rule

_SEVERITY = list(Severity)
_CONFIDENCE = list(Confidence)


def _sort_key(result: RuleResult) -> tuple[int, int]:
    if result.finding is None:
        return len(_SEVERITY), len(_CONFIDENCE)
    return _SEVERITY.index(result.finding.severity), _CONFIDENCE.index(result.finding.confidence)


def _assemble(rule: Rule, data: FindingData) -> Finding:
    return Finding(
        severity=data.severity if data.severity is not None else rule.severity,
        confidence=data.confidence,
        title=rule.title,
        summary=data.summary,
        signals=data.signals,
        evidence=data.evidence,
        likely_causes=rule.likely_causes,
        recommendations=rule.recommendations,
        related_metrics=rule.related_metrics,
    )


def run(
    metrics: MetricSeriesSnapshot,
    rules: Sequence[Rule],
) -> list[RuleResult]:
    results = []
    for rule in rules:
        data = rule.run(metrics)
        finding = _assemble(rule, data) if data is not None else None
        results.append(RuleResult(id=rule.id, name=rule.name, finding=finding))
    return sorted(results, key=_sort_key)
