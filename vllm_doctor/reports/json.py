from datetime import datetime, timezone

from pydantic import BaseModel

from vllm_doctor.models import ClientMode, DiagnosisResult, Health, Metrics, RuleResult

_SCRAPE_MODE_NOTICE = "TTFT, TPOT and Queue Latency rules require Prometheus — connect to Prometheus for full analysis."
_SCHEMA_VERSION = "1"


class Target(BaseModel):
    model_name: str | None
    window: str
    client_mode: ClientMode


class Metadata(BaseModel):
    generated_at: str
    target: Target


class DiagnosisReport(BaseModel):
    schema_version: str
    metadata: Metadata
    health: Health
    notice: str | None
    checks: list[RuleResult]
    metrics: Metrics


def render(result: DiagnosisResult, verbose: bool = False, compact: bool = False) -> str:
    report = DiagnosisReport(
        schema_version=_SCHEMA_VERSION,
        metadata=Metadata(
            generated_at=datetime.now(timezone.utc).isoformat(),
            target=Target(
                model_name=result.context.model_name,
                window=result.context.window,
                client_mode=result.context.client_mode,
            ),
        ),
        health=result.health,
        notice=_SCRAPE_MODE_NOTICE if result.context.client_mode == ClientMode.scrape else None,
        checks=result.checks,
        metrics=result.metrics,
    )
    exclude: dict = {"checks": {"__all__": {"finding": {"signals"}}}}
    if not verbose:
        exclude["metrics"] = True
    indent = None if compact else 2
    return report.model_dump_json(indent=indent, exclude=exclude)
