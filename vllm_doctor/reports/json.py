from pydantic import BaseModel

from vllm_doctor.models import DiagnosisResult, Finding, Health, Metrics


class DiagnosisReport(BaseModel):
    health: Health
    model_name: str | None
    window: str
    findings: list[Finding]
    metrics: Metrics


def render(result: DiagnosisResult) -> str:
    report = DiagnosisReport(
        health=result.health,
        model_name=result.snapshot.model_name,
        window=result.snapshot.window,
        findings=result.findings,
        metrics=result.snapshot.metrics,
    )
    return report.model_dump_json(indent=2)
