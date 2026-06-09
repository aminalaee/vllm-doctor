import io

import pytest
from rich.console import Console

from vllm_doctor.clients.models import MetricSample
from vllm_doctor.metrics import MetricSeries, MetricSeriesSnapshot
from vllm_doctor.models import (
    Confidence,
    DiagnosisContext,
    DiagnosisResult,
    Finding,
    RuleResult,
    Severity,
)
from vllm_doctor.reports.text import render

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


@pytest.fixture
def queue_check(queue_finding: Finding) -> RuleResult:
    return RuleResult(id="queue_pressure", name="Queue Pressure", finding=queue_finding)


class TestRenderText:
    def test_shows_header(self) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(context=_CTX, metric_series=MetricSeriesSnapshot(), checks=[]),
            console=Console(file=buf, highlight=False),
        )
        assert "vLLM Doctor" in buf.getvalue()

    def test_shows_multi_model_notice(self) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(
                context=DiagnosisContext(since="1h", model_name=None),
                metric_series=MetricSeriesSnapshot(
                    num_requests_running=MetricSeries(
                        samples=[
                            MetricSample(labels={"model_name": "a"}, value=1.0),
                            MetricSample(labels={"model_name": "b"}, value=2.0),
                        ]
                    )
                ),
                checks=[],
            ),
            console=Console(file=buf, highlight=False),
        )
        assert "Pass --model" in buf.getvalue()

    def test_ok_health_when_no_findings(self) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(context=_CTX, metric_series=MetricSeriesSnapshot(), checks=[]),
            console=Console(file=buf, highlight=False),
        )
        assert "OK" in buf.getvalue()

    def test_shows_matrix_rule_name(self, queue_check: RuleResult) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(context=_CTX, metric_series=MetricSeriesSnapshot(), checks=[queue_check]),
            console=Console(file=buf, highlight=False),
        )
        assert "Queue Pressure" in buf.getvalue()

    def test_shows_severity_in_health(self, queue_check: RuleResult) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(context=_CTX, metric_series=MetricSeriesSnapshot(), checks=[queue_check]),
            console=Console(file=buf, highlight=False),
        )
        assert "WARNING" in buf.getvalue()

    def test_matrix_ok_row_when_no_finding(self) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(
                context=_CTX, metric_series=MetricSeriesSnapshot(), checks=[RuleResult(id="my_rule", name="My Rule")]
            ),
            console=Console(file=buf, highlight=False),
        )
        assert "My Rule" in buf.getvalue()
        assert "ok" in buf.getvalue()

    def test_shows_finding_title(self, queue_check: RuleResult) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(context=_CTX, metric_series=MetricSeriesSnapshot(), checks=[queue_check]),
            console=Console(file=buf, highlight=False),
        )
        assert "Queue pressure" in buf.getvalue()

    def test_shows_evidence(self, queue_check: RuleResult) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(context=_CTX, metric_series=MetricSeriesSnapshot(), checks=[queue_check]),
            console=Console(file=buf, highlight=False),
        )
        assert "Waiting requests: 20" in buf.getvalue()

    def test_shows_recommendation(self, queue_check: RuleResult) -> None:
        buf = io.StringIO()
        render(
            DiagnosisResult(context=_CTX, metric_series=MetricSeriesSnapshot(), checks=[queue_check]),
            console=Console(file=buf, highlight=False),
        )
        assert "Add replicas" in buf.getvalue()

    def test_verbose_shows_metrics(self) -> None:
        ctx = DiagnosisContext(since="1h")
        metrics = MetricSeriesSnapshot(num_requests_running=5, kv_cache_usage_perc=0.5)
        buf = io.StringIO()
        render(
            DiagnosisResult(context=ctx, metric_series=metrics, checks=[]),
            console=Console(file=buf, highlight=False),
            verbose=True,
        )
        assert "Observed Metrics" in buf.getvalue()
        assert "Summary" in buf.getvalue()
        assert "Requests Running" in buf.getvalue()

    def test_verbose_shows_cache_bar(self) -> None:
        ctx = DiagnosisContext(since="1h")
        metrics = MetricSeriesSnapshot(kv_cache_usage_perc=0.94)
        buf = io.StringIO()
        render(
            DiagnosisResult(context=ctx, metric_series=metrics, checks=[]),
            console=Console(file=buf, highlight=False),
            verbose=True,
        )
        assert "█" in buf.getvalue()

    def test_verbose_nan_cache_shows_na(self) -> None:
        ctx = DiagnosisContext(since="1h")
        metrics = MetricSeriesSnapshot(kv_cache_usage_perc=float("nan"))
        buf = io.StringIO()
        render(
            DiagnosisResult(context=ctx, metric_series=metrics, checks=[]),
            console=Console(file=buf, highlight=False),
            verbose=True,
        )
        assert "n/a" in buf.getvalue()

    def test_non_verbose_hides_metrics(self) -> None:
        ctx = DiagnosisContext(since="1h")
        metrics = MetricSeriesSnapshot(num_requests_running=5)
        buf = io.StringIO()
        render(
            DiagnosisResult(context=ctx, metric_series=metrics, checks=[]),
            console=Console(file=buf, highlight=False),
            verbose=False,
        )
        assert "Observed Metrics" not in buf.getvalue()

    def test_non_verbose_hides_dimensional_metrics_when_multiple_entities(self) -> None:
        ctx = DiagnosisContext(since="1h")
        metric_series = MetricSeriesSnapshot(
            num_requests_running=MetricSeries(
                samples=[
                    MetricSample(labels={"pod": "vllm-0"}, value=2.0),
                    MetricSample(labels={"pod": "vllm-1"}, value=7.0),
                ]
            ),
            kv_cache_usage_perc=MetricSeries(
                samples=[
                    MetricSample(labels={"pod": "vllm-0"}, value=0.40),
                    MetricSample(labels={"pod": "vllm-1"}, value=0.92),
                ]
            ),
        )
        buf = io.StringIO()
        render(
            DiagnosisResult(context=ctx, metric_series=metric_series, checks=[]),
            console=Console(file=buf, highlight=False),
            verbose=False,
        )
        output = buf.getvalue()

        assert "Observed Metrics" not in output
        assert "vllm-0" not in output
        assert "vllm-1" not in output

    def test_verbose_shows_dimensional_metrics_when_multiple_entities(self) -> None:
        ctx = DiagnosisContext(since="1h")
        metric_series = MetricSeriesSnapshot(
            num_requests_running=MetricSeries(
                samples=[
                    MetricSample(labels={"pod": "vllm-0"}, value=2.0),
                    MetricSample(labels={"pod": "vllm-1"}, value=7.0),
                ]
            ),
            kv_cache_usage_perc=MetricSeries(
                samples=[
                    MetricSample(labels={"pod": "vllm-0"}, value=0.40),
                    MetricSample(labels={"pod": "vllm-1"}, value=0.92),
                ]
            ),
        )
        buf = io.StringIO()
        render(
            DiagnosisResult(context=ctx, metric_series=metric_series, checks=[]),
            console=Console(file=buf, highlight=False, width=160),
            verbose=True,
        )
        output = buf.getvalue()

        assert "Summary" in output
        assert "Observed Metrics per pod" in output
        assert "vllm-0" in output
        assert "vllm-1" in output
        assert "GPU Cache Usage" in output

    def test_dimensional_metrics_fallback_to_instance_label(self) -> None:
        ctx = DiagnosisContext(since="1h")
        metric_series = MetricSeriesSnapshot(
            num_requests_waiting=MetricSeries(
                samples=[
                    MetricSample(labels={"instance": "10.0.0.1:8000"}, value=0.0),
                    MetricSample(labels={"instance": "10.0.0.2:8000"}, value=4.0),
                ]
            )
        )
        buf = io.StringIO()
        render(
            DiagnosisResult(context=ctx, metric_series=metric_series, checks=[]),
            console=Console(file=buf, highlight=False),
            verbose=True,
        )

        assert "Observed Metrics per instance" in buf.getvalue()

    def test_single_replica_hides_dimensional_metrics(self) -> None:
        ctx = DiagnosisContext(since="1h")
        metric_series = MetricSeriesSnapshot(
            num_requests_running=MetricSeries(samples=[MetricSample(labels={"pod": "vllm-0"}, value=2.0)])
        )
        buf = io.StringIO()
        render(
            DiagnosisResult(context=ctx, metric_series=metric_series, checks=[]),
            console=Console(file=buf, highlight=False),
            verbose=True,
        )

        assert "vllm-0" not in buf.getvalue()

    def test_dimensional_metrics_limit_replica_columns(self) -> None:
        ctx = DiagnosisContext(since="1h")
        metric_series = MetricSeriesSnapshot(
            num_requests_waiting=MetricSeries(
                samples=[MetricSample(labels={"pod": f"vllm-{index}"}, value=float(index)) for index in range(8)]
            )
        )
        buf = io.StringIO()
        render(
            DiagnosisResult(context=ctx, metric_series=metric_series, checks=[]),
            console=Console(file=buf, highlight=False, width=200),
            verbose=True,
        )

        assert "+2 more" in buf.getvalue()
