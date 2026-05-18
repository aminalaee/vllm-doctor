import pytest

from vllm_doctor.diagnosis import run
from vllm_doctor.models import Confidence, Finding, MetricSnapshot, Severity
from vllm_doctor.rules.base import Rule


class FixedRule(Rule):
    def __init__(self, findings: list[Finding]) -> None:
        self._findings = findings

    def evaluate(self, snapshot: MetricSnapshot) -> list[Finding]:
        return self._findings


class EmptyRule(Rule):
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

    def test_returns_empty_when_no_findings(self, snapshot: MetricSnapshot) -> None:
        assert run(snapshot, [EmptyRule(), EmptyRule()]) == []

    def test_aggregates_findings_from_multiple_rules(
        self, snapshot: MetricSnapshot, warning_finding: Finding, info_finding: Finding
    ) -> None:
        findings = run(
            snapshot, [FixedRule([warning_finding]), FixedRule([info_finding])]
        )
        assert len(findings) == 2

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
        findings = run(snapshot, [FixedRule([low, medium, high])])
        assert [f.confidence for f in findings] == [
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
        findings = run(
            snapshot, [FixedRule([info_finding, warning_finding, critical_finding])]
        )
        assert [f.severity for f in findings] == [
            Severity.critical,
            Severity.warning,
            Severity.info,
        ]
