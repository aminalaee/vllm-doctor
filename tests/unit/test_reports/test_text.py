import io

import pytest
from rich.console import Console

from vllm_doctor.models import (
    Confidence,
    DiagnosisResult,
    Finding,
    MetricSnapshot,
    Metrics,
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

    def test_verbose_shows_metrics(self, snapshot: MetricSnapshot) -> None:
        snapshot = MetricSnapshot(
            window="1h",
            metrics=Metrics(num_requests_running=5, gpu_cache_usage_perc=0.5),
        )
        buf = io.StringIO()
        render(
            DiagnosisResult(snapshot=snapshot, findings=[]),
            console=Console(file=buf, highlight=False),
            verbose=True,
        )
        assert "Observed Metrics" in buf.getvalue()
        assert "Requests Running" in buf.getvalue()

    def test_verbose_shows_cache_bar(self, snapshot: MetricSnapshot) -> None:
        snapshot = MetricSnapshot(
            window="1h",
            metrics=Metrics(gpu_cache_usage_perc=0.94),
        )
        buf = io.StringIO()
        render(
            DiagnosisResult(snapshot=snapshot, findings=[]),
            console=Console(file=buf, highlight=False),
            verbose=True,
        )
        assert "█" in buf.getvalue()

    def test_verbose_nan_cache_shows_na(self) -> None:
        snapshot = MetricSnapshot(
            window="1h",
            metrics=Metrics(gpu_cache_usage_perc=float("nan")),
        )
        buf = io.StringIO()
        render(
            DiagnosisResult(snapshot=snapshot, findings=[]),
            console=Console(file=buf, highlight=False),
            verbose=True,
        )
        assert "n/a" in buf.getvalue()

    def test_non_verbose_hides_metrics(self, snapshot: MetricSnapshot) -> None:
        snapshot = MetricSnapshot(
            window="1h",
            metrics=Metrics(num_requests_running=5),
        )
        buf = io.StringIO()
        render(
            DiagnosisResult(snapshot=snapshot, findings=[]),
            console=Console(file=buf, highlight=False),
            verbose=False,
        )
        assert "Observed Metrics" not in buf.getvalue()
