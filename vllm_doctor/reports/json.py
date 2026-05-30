from pydantic import BaseModel

from vllm_doctor.models import ClientMode, DiagnosisResult, Health, Metrics, RuleResult

_SCRAPE_MODE_NOTICE = "TTFT, TPOT and Queue Latency rules require Prometheus — connect to Prometheus for full analysis."


class DiagnosisReport(BaseModel):
    health: Health
    model_name: str | None
    window: str
    client_mode: ClientMode
    notice: str | None
    checks: list[RuleResult]
    metrics: Metrics


def render(result: DiagnosisResult, verbose: bool = False) -> str:
    report = DiagnosisReport(
        health=result.health,
        model_name=result.context.model_name,
        window=result.context.window,
        client_mode=result.context.client_mode,
        notice=_SCRAPE_MODE_NOTICE if result.context.client_mode == ClientMode.scrape else None,
        checks=result.checks,
        metrics=result.current,
    )
    exclude: dict = {"checks": {"__all__": {"finding": {"signals"}}}}
    if not verbose:
        exclude["metrics"] = True
    return report.model_dump_json(indent=2, exclude=exclude)
