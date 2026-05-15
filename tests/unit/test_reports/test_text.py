import io

import pytest
from rich.console import Console

from vllm_doctor.models import (
    Confidence,
    DiagnosisResult,
    Finding,
    MetricSnapshot,
    Severity,
)
from vllm_doctor.reports.text import render


@pytest.fixture
def snapshot() -> MetricSnapshot:
    return MetricSnapshot(window="1h", model_name="meta-llama/Llama-3.1-8B")


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


class TestRenderText:
    def test_shows_header(self, snapshot: MetricSnapshot) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(snapshot=snapshot, findings=[]),
            console=Console(file=buf, highlight=False),
        )
        assert "vLLM Doctor" in buf.getvalue()

    def test_no_issues_message(self, snapshot: MetricSnapshot) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(snapshot=snapshot, findings=[]),
            console=Console(file=buf, highlight=False),
        )
        assert "No issues detected" in buf.getvalue()

    def test_shows_finding_title(
        self, snapshot: MetricSnapshot, queue_finding: Finding
    ) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(snapshot=snapshot, findings=[queue_finding]),
            console=Console(file=buf, highlight=False),
        )
        assert "Queue pressure" in buf.getvalue()

    def test_shows_severity(
        self, snapshot: MetricSnapshot, queue_finding: Finding
    ) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(snapshot=snapshot, findings=[queue_finding]),
            console=Console(file=buf, highlight=False),
        )
        assert "WARNING" in buf.getvalue()

    def test_shows_evidence(
        self, snapshot: MetricSnapshot, queue_finding: Finding
    ) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(snapshot=snapshot, findings=[queue_finding]),
            console=Console(file=buf, highlight=False),
        )
        assert "Waiting requests: 20" in buf.getvalue()

    def test_shows_recommendation(
        self, snapshot: MetricSnapshot, queue_finding: Finding
    ) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(snapshot=snapshot, findings=[queue_finding]),
            console=Console(file=buf, highlight=False),
        )
        assert "Add replicas" in buf.getvalue()
