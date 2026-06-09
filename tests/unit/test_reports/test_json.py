import json

import pytest
from freezegun import freeze_time

from vllm_doctor.clients.models import MetricSample
from vllm_doctor.metrics import MetricSeries, MetricSeriesSnapshot
from vllm_doctor.models import (
    ClientMode,
    Confidence,
    DiagnosisContext,
    DiagnosisResult,
    Finding,
    RuleResult,
    Severity,
)
from vllm_doctor.reports import json as json_report
from vllm_doctor.reports.notices import MULTI_MODEL_NOTICE

_CTX = DiagnosisContext(since="1h", model_name="meta-llama/Llama-3.1-8B")


@pytest.fixture
def two_model_series() -> MetricSeries:
    return MetricSeries(
        samples=[
            MetricSample(labels={"model_name": "a"}, value=1.0),
            MetricSample(labels={"model_name": "b"}, value=2.0),
        ]
    )


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
            metric_series=MetricSeriesSnapshot(),
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
            "notices": [],
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
            metric_series=MetricSeriesSnapshot(),
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
            "notices": [
                "TTFT, TPOT and Queue Latency rules require Prometheus — connect to Prometheus for full analysis."
            ],
            "checks": [],
        }

    def test_multi_model_notice_when_unfiltered(self, two_model_series: MetricSeries) -> None:
        result = DiagnosisResult(
            context=DiagnosisContext(since="1h", model_name=None),
            metric_series=MetricSeriesSnapshot(num_requests_running=two_model_series),
            checks=[],
        )
        assert json.loads(json_report.render(result))["notices"] == [MULTI_MODEL_NOTICE]

    def test_no_multi_model_notice_when_filtered(self, two_model_series: MetricSeries) -> None:
        result = DiagnosisResult(
            context=DiagnosisContext(since="1h", model_name="a"),
            metric_series=MetricSeriesSnapshot(num_requests_running=two_model_series),
            checks=[],
        )
        assert json.loads(json_report.render(result))["notices"] == []

    @freeze_time("2026-06-01 13:44:39 UTC")
    def test_verbose_metrics_shape(self) -> None:
        result = DiagnosisResult(
            context=_CTX,
            metric_series=MetricSeriesSnapshot(num_requests_running=2, prefix_cache_hit_rate=0.5),
            checks=[],
        )
        output = json.loads(json_report.render(result, verbose=True))
        assert output["metadata"]["generated_at"] == "2026-06-01T13:44:39+00:00"
        assert output["metrics"] == {
            "num_requests_running": {"value": 2.0},
            "num_requests_waiting": {"value": None},
            "kv_cache_usage_perc": {"value": None},
            "prompt_tokens_per_second": {"value": None},
            "generation_tokens_per_second": {"value": None},
            "request_success_total": {"value": None},
            "request_error_total": {"value": None},
            "request_abort_total": {"value": None},
            "ttft_p95_seconds": {"value": None},
            "tpot_p95_seconds": {"value": None},
            "prefix_cache_hit_rate": {"value": 0.5},
            "queue_time_p95_seconds": {"value": None},
            "num_preemptions_total": {"value": None},
        }

    @freeze_time("2026-06-01 13:44:39 UTC")
    def test_verbose_metrics_include_replica_breakdown(self) -> None:
        result = DiagnosisResult(
            context=_CTX,
            metric_series=MetricSeriesSnapshot(
                num_requests_running=MetricSeries(
                    samples=[
                        MetricSample(labels={"pod": "vllm-0"}, value=2.0),
                        MetricSample(labels={"pod": "vllm-1"}, value=10.0),
                    ]
                ),
                kv_cache_usage_perc=MetricSeries(
                    samples=[
                        MetricSample(labels={"pod": "vllm-0"}, value=0.41),
                        MetricSample(labels={"pod": "vllm-1"}, value=0.94),
                    ]
                ),
            ),
            checks=[],
        )

        output = json.loads(json_report.render(result, verbose=True))

        assert output["metrics"]["num_requests_running"] == {
            "value": 12.0,
            "by": {"pod": {"vllm-0": 2.0, "vllm-1": 10.0}},
        }
        assert output["metrics"]["kv_cache_usage_perc"] == {
            "value": 0.94,
            "by": {"pod": {"vllm-0": 0.41, "vllm-1": 0.94}},
        }

    @freeze_time("2026-06-01 13:44:39 UTC")
    def test_verbose_metrics_omit_replica_breakdown_for_single_replica(self) -> None:
        result = DiagnosisResult(
            context=_CTX,
            metric_series=MetricSeriesSnapshot(
                num_requests_running=MetricSeries(samples=[MetricSample(labels={"pod": "vllm-0"}, value=2.0)]),
            ),
            checks=[],
        )

        output = json.loads(json_report.render(result, verbose=True))

        assert output["metrics"]["num_requests_running"] == {"value": 2.0}

    @freeze_time("2026-06-01 13:44:39 UTC")
    def test_health_reflects_worst_severity(self, queue_finding: Finding) -> None:
        critical_finding = queue_finding.model_copy(update={"severity": Severity.critical})
        result = DiagnosisResult(
            context=_CTX,
            metric_series=MetricSeriesSnapshot(),
            checks=[
                RuleResult(id="queue_pressure", name="Queue Pressure", finding=queue_finding),
                RuleResult(id="kv_cache_pressure", name="KV Cache Pressure", finding=critical_finding),
            ],
        )
        output = json.loads(json_report.render(result))
        assert output["health"] == "critical"

    @freeze_time("2026-06-01 13:44:39 UTC")
    def test_empty_report(self) -> None:
        result = DiagnosisResult(context=_CTX, metric_series=MetricSeriesSnapshot(), checks=[])
        output = json.loads(json_report.render(result))
        assert output["health"] == "ok"
        assert output["checks"] == []

    @pytest.mark.parametrize(
        ("verbose", "expected"),
        [(False, False), (True, True)],
    )
    @freeze_time("2026-06-01 13:44:39 UTC")
    def test_metrics_visibility(self, verbose: bool, expected: bool) -> None:
        result = DiagnosisResult(context=_CTX, metric_series=MetricSeriesSnapshot(), checks=[])
        output = json.loads(json_report.render(result, verbose=verbose))
        assert ("metrics" in output) is expected
