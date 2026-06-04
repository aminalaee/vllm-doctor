from datetime import datetime, timezone

from pydantic import BaseModel

from vllm_doctor.metrics import METRIC_SPECS, MetricSeriesSnapshot, detect_replica_label
from vllm_doctor.models import ClientMode, DiagnosisResult, Health, RuleResult

_SCRAPE_MODE_NOTICE = "TTFT, TPOT and Queue Latency rules require Prometheus — connect to Prometheus for full analysis."
_SCHEMA_VERSION = "1"


class ReportTarget(BaseModel):
    model_name: str | None
    since: str
    client_mode: ClientMode


class Metadata(BaseModel):
    generated_at: str
    target: ReportTarget


class DiagnosisReport(BaseModel):
    schema_version: str
    metadata: Metadata
    health: Health
    notice: str | None
    checks: list[RuleResult]
    metrics: dict[str, dict]


def _structured_metrics(metric_series: MetricSeriesSnapshot) -> dict[str, dict]:
    """Build the verbose `metrics` block: aggregate `value` plus `by[<label>]` when multi-replica."""
    label = detect_replica_label(metric_series)
    scalar_metrics = metric_series.to_metrics()
    result: dict[str, dict] = {}
    for spec in METRIC_SPECS:
        entry: dict = {"value": getattr(scalar_metrics, spec.output)}
        if label is not None:
            breakdown = getattr(metric_series, spec.output).by(label)
            if len(breakdown) > 1:
                entry["by"] = {label: breakdown}
        result[spec.output] = entry
    return result


def render(result: DiagnosisResult, verbose: bool = False, compact: bool = False) -> str:
    report = DiagnosisReport(
        schema_version=_SCHEMA_VERSION,
        metadata=Metadata(
            generated_at=datetime.now(timezone.utc).isoformat(),
            target=ReportTarget(
                model_name=result.context.model_name,
                since=result.context.since,
                client_mode=result.context.client_mode,
            ),
        ),
        health=result.health,
        notice=_SCRAPE_MODE_NOTICE if result.context.client_mode == ClientMode.scrape else None,
        checks=result.checks,
        metrics=_structured_metrics(result.metric_series),
    )
    exclude: dict = {"checks": {"__all__": {"finding": {"signals"}}}}
    if not verbose:
        exclude["metrics"] = True
    indent = None if compact else 2
    return report.model_dump_json(indent=indent, exclude=exclude)
