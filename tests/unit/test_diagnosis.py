import pytest

from vllm_doctor.diagnosis import run
from vllm_doctor.models import Confidence, Finding, MetricSnapshot, Severity
from vllm_doctor.rules.base import Rule


class FixedRule(Rule):
    def __init__(self, findings: list[Finding], rule_name: str = "Fixed") -> None:
        self._findings = findings
        self._name = rule_name

    @property
    def name(self) -> str:
        return self._name

    def evaluate(self, snapshot: MetricSnapshot) -> list[Finding]:
        return self._findings


class EmptyRule(Rule):
    @property
    def name(self) -> str:
        return "Empty"

    def evaluate(self, snapshot: MetricSnapshot) -> list[Finding]:
        return []


@pytest.fixture
def snapshot() -> MetricSnapshot:
    return MetricSnapshot(window="1h")


@pytest.fixture
def critical_finding() -> Finding:
    return Finding(
        severity=Severity.critical,
        confidence=Confidence.high,
        title="critical",
        summary="test",
    )


@pytest.fixture
def warning_finding() -> Finding:
    return Finding(
        severity=Severity.warning,
        confidence=Confidence.high,
        title="warning",
        summary="test",
    )


@pytest.fixture
def info_finding() -> Finding:
    return Finding(
        severity=Severity.info, confidence=Confidence.high, title="info", summary="test"
    )


class TestRun:
    def test_returns_empty_when_no_rules(self, snapshot: MetricSnapshot) -> None:
        assert run(snapshot, []) == []

    def test_ok_results_when_no_findings(self, snapshot: MetricSnapshot) -> None:
        results = run(snapshot, [EmptyRule(), EmptyRule()])
        assert len(results) == 2
        assert all(r.finding is None for r in results)

    def test_aggregates_findings_from_multiple_rules(
        self, snapshot: MetricSnapshot, warning_finding: Finding, info_finding: Finding
    ) -> None:
        results = run(
            snapshot, [FixedRule([warning_finding]), FixedRule([info_finding])]
        )
        assert len(results) == 2

    def test_sorts_by_confidence_within_same_severity(
        self, snapshot: MetricSnapshot
    ) -> None:
        low = Finding(
            severity=Severity.warning,
            confidence=Confidence.low,
            title="low",
            summary="test",
        )
        medium = Finding(
            severity=Severity.warning,
            confidence=Confidence.medium,
            title="medium",
            summary="test",
        )
        high = Finding(
            severity=Severity.warning,
            confidence=Confidence.high,
            title="high",
            summary="test",
        )
        results = run(
            snapshot,
            [
                FixedRule([low], "Low"),
                FixedRule([medium], "Medium"),
                FixedRule([high], "High"),
            ],
        )
        assert [r.finding.confidence for r in results] == [
            Confidence.high,
            Confidence.medium,
            Confidence.low,
        ]

    def test_sorts_by_severity(
        self,
        snapshot: MetricSnapshot,
        critical_finding: Finding,
        warning_finding: Finding,
        info_finding: Finding,
    ) -> None:
        results = run(
            snapshot,
            [
                FixedRule([info_finding], "Info"),
                FixedRule([warning_finding], "Warning"),
                FixedRule([critical_finding], "Critical"),
            ],
        )
        assert [r.finding.severity for r in results] == [
            Severity.critical,
            Severity.warning,
            Severity.info,
        ]

    def test_ok_rules_sorted_last(
        self, snapshot: MetricSnapshot, warning_finding: Finding
    ) -> None:
        results = run(snapshot, [EmptyRule(), FixedRule([warning_finding], "Warn")])
        assert results[0].finding is not None
        assert results[-1].finding is None

    def test_result_preserves_rule_name(self, snapshot: MetricSnapshot) -> None:
        results = run(snapshot, [EmptyRule()])
        assert results[0].name == "Empty"
