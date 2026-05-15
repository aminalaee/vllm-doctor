from pydantic import BaseModel

from vllm_doctor.models import Finding, MetricSnapshot, Severity


class DiagnosisReport(BaseModel):
    health: str
    window: str
    findings: list[Finding]
    metrics: MetricSnapshot

    model_config = {"json_encoders": {}}


def render(findings: list[Finding], snapshot: MetricSnapshot) -> str:
    health = (
        "ok"
        if not findings
        else min(
            findings, key=lambda f: list(Severity).index(f.severity)
        ).severity.value
    )
    report = DiagnosisReport(
        health=health,
        window=snapshot.window,
        findings=findings,
        metrics=snapshot,
    )
    return report.model_dump_json(
        indent=2, exclude={"metrics": {"window", "model_name"}}
    )
