import json

import pytest

from vllm_doctor.models import Confidence, Finding, MetricSnapshot, Severity
from vllm_doctor.reports import json as json_report


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


class TestRenderJson:
    def test_ok_health_no_findings(self, snapshot: MetricSnapshot) -> None:
        output = json.loads(json_report.render([], snapshot))
        assert output["health"] == "ok"

    def test_health_reflects_worst_severity(
        self, snapshot: MetricSnapshot, queue_finding: Finding
    ) -> None:
        output = json.loads(json_report.render([queue_finding], snapshot))
        assert output["health"] == "warning"

    def test_findings_in_output(
        self, snapshot: MetricSnapshot, queue_finding: Finding
    ) -> None:
        output = json.loads(json_report.render([queue_finding], snapshot))
        assert len(output["findings"]) == 1
        assert output["findings"][0]["title"] == "Queue pressure"

    def test_empty_findings(self, snapshot: MetricSnapshot) -> None:
        output = json.loads(json_report.render([], snapshot))
        assert output["findings"] == []

    def test_metrics_in_output(self, snapshot: MetricSnapshot) -> None:
        output = json.loads(json_report.render([], snapshot))
        assert "metrics" in output

    def test_window_in_output(self, snapshot: MetricSnapshot) -> None:
        output = json.loads(json_report.render([], snapshot))
        assert output["window"] == "1h"
