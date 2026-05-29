import pytest

from vllm_doctor.diagnosis import run
from vllm_doctor.models import Confidence, DiagnosisContext, Finding, FindingData, Metrics, Severity
from vllm_doctor.rules.base import Rule


class FixedRule(Rule):
    title = "Fixed"
    severity = Severity.warning

    def __init__(self, findings: list[Finding], rule_name: str = "Fixed") -> None:
        self._findings = findings
        self.name = rule_name

    def _run(self, current: Metrics, previous: Metrics | None) -> FindingData | None:
        return None

    def run(self, context: DiagnosisContext, current: Metrics, previous: Metrics | None = None) -> list[Finding]:
        return self._findings


class EmptyRule(Rule):
    name = "Empty"
    title = "Empty"
    severity = Severity.info

    def _run(self, current: Metrics, previous: Metrics | None) -> FindingData | None:
        return None


@pytest.fixture
def ctx() -> DiagnosisContext:
    return DiagnosisContext(window="1h")


@pytest.fixture
def critical_finding() -> Finding:
    return Finding(severity=Severity.critical, confidence=Confidence.high, title="critical", summary="test")


@pytest.fixture
def warning_finding() -> Finding:
    return Finding(severity=Severity.warning, confidence=Confidence.high, title="warning", summary="test")


@pytest.fixture
def info_finding() -> Finding:
    return Finding(severity=Severity.info, confidence=Confidence.high, title="info", summary="test")


class TestRun:
    def test_returns_empty_when_no_rules(self, ctx: DiagnosisContext) -> None:
        assert run(context=ctx, current=Metrics(), rules=[]) == []

    def test_ok_results_when_no_findings(self, ctx: DiagnosisContext) -> None:
        results = run(context=ctx, current=Metrics(), rules=[EmptyRule(), EmptyRule()])
        assert len(results) == 2
        assert all(r.finding is None for r in results)

    def test_aggregates_findings_from_multiple_rules(
        self, ctx: DiagnosisContext, warning_finding: Finding, info_finding: Finding
    ) -> None:
        results = run(context=ctx, current=Metrics(), rules=[FixedRule([warning_finding]), FixedRule([info_finding])])
        assert len(results) == 2

    def test_sorts_by_confidence_within_same_severity(self, ctx: DiagnosisContext) -> None:
        low = Finding(severity=Severity.warning, confidence=Confidence.low, title="low", summary="test")
        medium = Finding(severity=Severity.warning, confidence=Confidence.medium, title="medium", summary="test")
        high = Finding(severity=Severity.warning, confidence=Confidence.high, title="high", summary="test")
        results = run(
            context=ctx,
            current=Metrics(),
            rules=[FixedRule([low], "Low"), FixedRule([medium], "Medium"), FixedRule([high], "High")],
        )
        assert [r.finding.confidence for r in results] == [Confidence.high, Confidence.medium, Confidence.low]

    def test_sorts_by_severity(
        self,
        ctx: DiagnosisContext,
        critical_finding: Finding,
        warning_finding: Finding,
        info_finding: Finding,
    ) -> None:
        results = run(
            context=ctx,
            current=Metrics(),
            rules=[
                FixedRule([info_finding], "Info"),
                FixedRule([warning_finding], "Warning"),
                FixedRule([critical_finding], "Critical"),
            ],
        )
        assert [r.finding.severity for r in results] == [Severity.critical, Severity.warning, Severity.info]

    def test_ok_rules_sorted_last(self, ctx: DiagnosisContext, warning_finding: Finding) -> None:
        results = run(context=ctx, current=Metrics(), rules=[EmptyRule(), FixedRule([warning_finding], "Warn")])
        assert results[0].finding is not None
        assert results[-1].finding is None

    def test_result_preserves_rule_name(self, ctx: DiagnosisContext) -> None:
        results = run(context=ctx, current=Metrics(), rules=[EmptyRule()])
        assert results[0].name == "Empty"

    def test_passes_previous_to_rules(self, ctx: DiagnosisContext) -> None:
        received: list[Metrics | None] = []

        class CapturingRule(Rule):
            name = "Capturing"
            title = "Capturing"
            severity = Severity.info

            def _run(self, current: Metrics, previous: Metrics | None) -> FindingData | None:
                received.append(previous)
                return None

        prev = Metrics(num_requests_running=5.0)
        run(context=ctx, current=Metrics(), rules=[CapturingRule()], previous=prev)
        assert received[0] is prev
