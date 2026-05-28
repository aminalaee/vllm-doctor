import json

import pytest

from vllm_doctor.models import (
    Confidence,
    DiagnosisContext,
    DiagnosisResult,
    Finding,
    Metrics,
    RuleResult,
    Severity,
)
from vllm_doctor.reports import json as json_report

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


class TestRenderJson:
    def test_ok_health_no_findings(self) -> None:
        result = DiagnosisResult(context=_CTX, current=Metrics(), checks=[])
        output = json.loads(json_report.render(result))
        assert output["health"] == "ok"

    def test_health_reflects_worst_severity(self, queue_finding: Finding) -> None:
        result = DiagnosisResult(
            context=_CTX,
            current=Metrics(),
            checks=[RuleResult(name="Queue Pressure", finding=queue_finding)],
        )
        output = json.loads(json_report.render(result))
        assert output["health"] == "warning"

    def test_checks_in_output(self, queue_finding: Finding) -> None:
        result = DiagnosisResult(
            context=_CTX,
            current=Metrics(),
            checks=[
                RuleResult(name="Queue Pressure", finding=queue_finding),
                RuleResult(name="KV Cache Pressure"),
            ],
        )
        output = json.loads(json_report.render(result))
        assert len(output["checks"]) == 2
        assert output["checks"][0]["name"] == "Queue Pressure"
        assert output["checks"][0]["finding"]["title"] == "Queue pressure"
        assert output["checks"][1]["name"] == "KV Cache Pressure"
        assert output["checks"][1]["finding"] is None

    def test_empty_checks(self) -> None:
        result = DiagnosisResult(context=_CTX, current=Metrics(), checks=[])
        output = json.loads(json_report.render(result))
        assert output["checks"] == []

    def test_metrics_not_in_default_output(self) -> None:
        result = DiagnosisResult(context=_CTX, current=Metrics(), checks=[])
        output = json.loads(json_report.render(result))
        assert "metrics" not in output

    def test_metrics_in_verbose_output(self) -> None:
        result = DiagnosisResult(context=_CTX, current=Metrics(), checks=[])
        output = json.loads(json_report.render(result, verbose=True))
        assert "metrics" in output

    def test_window_in_output(self) -> None:
        result = DiagnosisResult(context=_CTX, current=Metrics(), checks=[])
        output = json.loads(json_report.render(result))
        assert output["window"] == "1h"

    def test_signals_excluded_from_finding(self, queue_finding: Finding) -> None:
        result = DiagnosisResult(
            context=_CTX,
            current=Metrics(),
            checks=[RuleResult(name="Queue Pressure", finding=queue_finding)],
        )
        output = json.loads(json_report.render(result))
        assert "signals" not in output["checks"][0]["finding"]
