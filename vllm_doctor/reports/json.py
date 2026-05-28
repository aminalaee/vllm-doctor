from pydantic import BaseModel

from vllm_doctor.models import DiagnosisResult, Health, Metrics, RuleResult


class DiagnosisReport(BaseModel):
    health: Health
    model_name: str | None
    window: str
    checks: list[RuleResult]
    metrics: Metrics


def render(result: DiagnosisResult, verbose: bool = False) -> str:
    report = DiagnosisReport(
        health=result.health,
        model_name=result.context.model_name,
        window=result.context.window,
        checks=result.checks,
        metrics=result.current,
    )
    exclude: dict = {"checks": {"__all__": {"finding": {"signals"}}}}
    if not verbose:
        exclude["metrics"] = True
    return report.model_dump_json(indent=2, exclude=exclude)
