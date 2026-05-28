import io

import pytest
from rich.console import Console

from vllm_doctor.models import (
    Confidence,
    DiagnosisContext,
    DiagnosisResult,
    Finding,
    Metrics,
    RuleResult,
    Severity,
)
from vllm_doctor.reports.text import render

_CTX = DiagnosisContext(window="1h", model_name="meta-llama/Llama-3.1-8B")


@pytest.fixture
def queue_finding() -> Finding:
    return Finding(
        severity=Severity.warning,
        confidence=Confidence.low,
        title="Queue pressure",
        summary="Requests are queuing faster than the server can process them.",
        evidence=["Waiting requests: 20"],
        likely_causes=["Insufficient capacity"],
        recommendations=["Add replicas"],
    )


@pytest.fixture
def queue_check(queue_finding: Finding) -> RuleResult:
    return RuleResult(name="Queue Pressure", finding=queue_finding)


class TestRenderText:
    def test_shows_header(self) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(context=_CTX, current=Metrics(), checks=[]),
            console=Console(file=buf, highlight=False),
        )
        assert "vLLM Doctor" in buf.getvalue()

    def test_ok_health_when_no_findings(self) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(context=_CTX, current=Metrics(), checks=[]),
            console=Console(file=buf, highlight=False),
        )
        assert "OK" in buf.getvalue()

    def test_shows_matrix_rule_name(self, queue_check: RuleResult) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(context=_CTX, current=Metrics(), checks=[queue_check]),
            console=Console(file=buf, highlight=False),
        )
        assert "Queue Pressure" in buf.getvalue()

    def test_shows_severity_in_health(self, queue_check: RuleResult) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(context=_CTX, current=Metrics(), checks=[queue_check]),
            console=Console(file=buf, highlight=False),
        )
        assert "WARNING" in buf.getvalue()

    def test_matrix_ok_row_when_no_finding(self) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(context=_CTX, current=Metrics(), checks=[RuleResult(name="My Rule")]),
            console=Console(file=buf, highlight=False),
        )
        assert "My Rule" in buf.getvalue()
        assert "ok" in buf.getvalue()

    def test_shows_finding_title(self, queue_check: RuleResult) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(context=_CTX, current=Metrics(), checks=[queue_check]),
            console=Console(file=buf, highlight=False),
        )
        assert "Queue pressure" in buf.getvalue()

    def test_shows_evidence(self, queue_check: RuleResult) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(context=_CTX, current=Metrics(), checks=[queue_check]),
            console=Console(file=buf, highlight=False),
        )
        assert "Waiting requests: 20" in buf.getvalue()

    def test_shows_recommendation(self, queue_check: RuleResult) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(context=_CTX, current=Metrics(), checks=[queue_check]),
            console=Console(file=buf, highlight=False),
        )
        assert "Add replicas" in buf.getvalue()

    def test_verbose_shows_metrics(self) -> None:
        ctx = DiagnosisContext(window="1h")
        current = Metrics(num_requests_running=5, kv_cache_usage_perc=0.5)
        buf = io.StringIO()
        render(
            DiagnosisResult(context=ctx, current=current, checks=[]),
            console=Console(file=buf, highlight=False),
            verbose=True,
        )
        assert "Observed Metrics" in buf.getvalue()
        assert "Requests Running" in buf.getvalue()

    def test_verbose_shows_cache_bar(self) -> None:
        ctx = DiagnosisContext(window="1h")
        current = Metrics(kv_cache_usage_perc=0.94)
        buf = io.StringIO()
        render(
            DiagnosisResult(context=ctx, current=current, checks=[]),
            console=Console(file=buf, highlight=False),
            verbose=True,
        )
        assert "█" in buf.getvalue()

    def test_verbose_nan_cache_shows_na(self) -> None:
        ctx = DiagnosisContext(window="1h")
        current = Metrics(kv_cache_usage_perc=float("nan"))
        buf = io.StringIO()
        render(
            DiagnosisResult(context=ctx, current=current, checks=[]),
            console=Console(file=buf, highlight=False),
            verbose=True,
        )
        assert "n/a" in buf.getvalue()

    def test_non_verbose_hides_metrics(self) -> None:
        ctx = DiagnosisContext(window="1h")
        current = Metrics(num_requests_running=5)
        buf = io.StringIO()
        render(
            DiagnosisResult(context=ctx, current=current, checks=[]),
            console=Console(file=buf, highlight=False),
            verbose=False,
        )
        assert "Observed Metrics" not in buf.getvalue()
