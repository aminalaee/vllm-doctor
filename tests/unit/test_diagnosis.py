from vllm_doctor.diagnosis import run
from vllm_doctor.models import Confidence, FindingData, Metrics, Severity
from vllm_doctor.rules.base import Rule


class _EmptyRule(Rule):
    id = "empty"
    name = "Empty"
    title = "Empty"
    severity = Severity.info

    def run(self, _metrics: Metrics) -> FindingData | None:
        return None


class _CriticalRule(Rule):
    id = "critical"
    name = "Critical"
    title = "Critical"
    severity = Severity.critical

    def run(self, _metrics: Metrics) -> FindingData | None:
        return FindingData(confidence=Confidence.high, summary="test")


class _WarningRule(Rule):
    id = "warning"
    name = "Warning"
    title = "Warning"
    severity = Severity.warning

    def run(self, _metrics: Metrics) -> FindingData | None:
        return FindingData(confidence=Confidence.high, summary="test")


class _InfoRule(Rule):
    id = "info"
    name = "Info"
    title = "Info"
    severity = Severity.info

    def run(self, _metrics: Metrics) -> FindingData | None:
        return FindingData(confidence=Confidence.high, summary="test")


class _LowConfRule(Rule):
    id = "low"
    name = "Low"
    title = "Low"
    severity = Severity.warning

    def run(self, _metrics: Metrics) -> FindingData | None:
        return FindingData(confidence=Confidence.low, summary="test")


class _MediumConfRule(Rule):
    id = "medium"
    name = "Medium"
    title = "Medium"
    severity = Severity.warning

    def run(self, _metrics: Metrics) -> FindingData | None:
        return FindingData(confidence=Confidence.medium, summary="test")


class _HighConfRule(Rule):
    id = "high"
    name = "High"
    title = "High"
    severity = Severity.warning

    def run(self, _metrics: Metrics) -> FindingData | None:
        return FindingData(confidence=Confidence.high, summary="test")


class TestRun:
    def test_returns_empty_when_no_rules(self) -> None:
        assert run(metrics=Metrics(), rules=[]) == []

    def test_ok_results_when_no_findings(self) -> None:
        results = run(metrics=Metrics(), rules=[_EmptyRule(), _EmptyRule()])
        assert len(results) == 2
        assert all(r.finding is None for r in results)

    def test_aggregates_findings_from_multiple_rules(self) -> None:
        results = run(metrics=Metrics(), rules=[_WarningRule(), _InfoRule()])
        assert len(results) == 2

    def test_sorts_by_confidence_within_same_severity(self) -> None:
        results = run(
            metrics=Metrics(),
            rules=[_LowConfRule(), _MediumConfRule(), _HighConfRule()],
        )
        confidences = [r.finding.confidence for r in results if r.finding]
        assert confidences == [Confidence.high, Confidence.medium, Confidence.low]

    def test_sorts_by_severity(self) -> None:
        results = run(
            metrics=Metrics(),
            rules=[_InfoRule(), _WarningRule(), _CriticalRule()],
        )
        severities = [r.finding.severity for r in results if r.finding]
        assert severities == [Severity.critical, Severity.warning, Severity.info]

    def test_ok_rules_sorted_last(self) -> None:
        results = run(metrics=Metrics(), rules=[_EmptyRule(), _WarningRule()])
        assert results[0].finding is not None
        assert results[-1].finding is None

    def test_result_preserves_rule_name(self) -> None:
        results = run(metrics=Metrics(), rules=[_EmptyRule()])
        assert results[0].id == "empty"
        assert results[0].name == "Empty"
