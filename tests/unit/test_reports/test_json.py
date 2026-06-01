import json

import pytest
from freezegun import freeze_time

from vllm_doctor.models import (
    ClientMode,
    Confidence,
    DiagnosisContext,
    DiagnosisResult,
    Finding,
    Metrics,
    RuleResult,
    Severity,
)
from vllm_doctor.reports import json as json_report

_CTX = DiagnosisContext(since="1h", model_name="meta-llama/Llama-3.1-8B")


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
    @freeze_time("2026-06-01 13:44:39 UTC")
    def test_default_report_shape(self, queue_finding: Finding) -> None:
        result = DiagnosisResult(
            context=_CTX,
            metrics=Metrics(),
            checks=[
                RuleResult(id="queue_pressure", name="Queue Pressure", finding=queue_finding),
                RuleResult(id="kv_cache_pressure", name="KV Cache Pressure"),
            ],
        )
        output = json.loads(json_report.render(result))
        assert output == {
            "schema_version": "1",
            "metadata": {
                "generated_at": "2026-06-01T13:44:39+00:00",
                "target": {
                    "model_name": "meta-llama/Llama-3.1-8B",
                    "since": "1h",
                    "client_mode": "prometheus",
                },
            },
            "health": "warning",
            "notice": None,
            "checks": [
                {
                    "id": "queue_pressure",
                    "name": "Queue Pressure",
                    "finding": {
                        "severity": "warning",
                        "confidence": "low",
                        "title": "Queue pressure",
                        "summary": "Requests are queuing faster than the server can process them.",
                        "evidence": ["Waiting requests: 20"],
                        "likely_causes": ["Insufficient capacity"],
                        "recommendations": ["Add replicas"],
                        "related_metrics": [],
                    },
                },
                {"id": "kv_cache_pressure", "name": "KV Cache Pressure", "finding": None},
            ],
        }

    @freeze_time("2026-06-01 13:44:39 UTC")
    def test_scrape_notice_shape(self) -> None:
        result = DiagnosisResult(
            context=DiagnosisContext(since="now", client_mode=ClientMode.scrape),
            metrics=Metrics(),
            checks=[],
        )
        output = json.loads(json_report.render(result))
        assert output == {
            "schema_version": "1",
            "metadata": {
                "generated_at": "2026-06-01T13:44:39+00:00",
                "target": {
                    "model_name": None,
                    "since": "now",
                    "client_mode": "scrape",
                },
            },
            "health": "ok",
            "notice": (
                "TTFT, TPOT and Queue Latency rules require Prometheus — connect to Prometheus for full analysis."
            ),
            "checks": [],
        }

    @freeze_time("2026-06-01 13:44:39 UTC")
    def test_verbose_metrics_shape(self) -> None:
        result = DiagnosisResult(
            context=_CTX,
            metrics=Metrics(num_requests_running=2, prefix_cache_hit_rate=0.5),
            checks=[],
        )
        output = json.loads(json_report.render(result, verbose=True))
        assert output["metadata"]["generated_at"] == "2026-06-01T13:44:39+00:00"
        assert output["metrics"] == {
            "num_requests_running": 2.0,
            "num_requests_waiting": None,
            "kv_cache_usage_perc": None,
            "prompt_tokens_per_second": None,
            "generation_tokens_per_second": None,
            "request_success_total": None,
            "request_error_total": None,
            "request_abort_total": None,
            "ttft_p95_seconds": None,
            "tpot_p95_seconds": None,
            "prefix_cache_hit_rate": 0.5,
            "queue_time_p95_seconds": None,
            "num_preemptions_total": None,
        }

    @freeze_time("2026-06-01 13:44:39 UTC")
    def test_health_reflects_worst_severity(self, queue_finding: Finding) -> None:
        critical_finding = queue_finding.model_copy(update={"severity": Severity.critical})
        result = DiagnosisResult(
            context=_CTX,
            metrics=Metrics(),
            checks=[
                RuleResult(id="queue_pressure", name="Queue Pressure", finding=queue_finding),
                RuleResult(id="kv_cache_pressure", name="KV Cache Pressure", finding=critical_finding),
            ],
        )
        output = json.loads(json_report.render(result))
        assert output["health"] == "critical"

    @freeze_time("2026-06-01 13:44:39 UTC")
    def test_empty_report(self) -> None:
        result = DiagnosisResult(context=_CTX, metrics=Metrics(), checks=[])
        output = json.loads(json_report.render(result))
        assert output["health"] == "ok"
        assert output["checks"] == []

    @pytest.mark.parametrize(
        ("verbose", "expected"),
        [(False, False), (True, True)],
    )
    @freeze_time("2026-06-01 13:44:39 UTC")
    def test_metrics_visibility(self, verbose: bool, expected: bool) -> None:
        result = DiagnosisResult(context=_CTX, metrics=Metrics(), checks=[])
        output = json.loads(json_report.render(result, verbose=verbose))
        assert ("metrics" in output) is expected
