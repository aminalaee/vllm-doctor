from enum import Enum
from typing import Annotated, TypeAlias

from pydantic import BaseModel, Field

_Metric: TypeAlias = float | None


class Severity(str, Enum):
    critical = "critical"
    warning = "warning"
    info = "info"


class Health(str, Enum):
    ok = "ok"
    info = "info"
    warning = "warning"
    critical = "critical"


class Confidence(str, Enum):
    low = "low"
    medium = "medium"
    high = "high"


class Metrics(BaseModel):
    num_requests_running: Annotated[_Metric, Field(title="Requests Running")] = None
    num_requests_waiting: Annotated[_Metric, Field(title="Requests Waiting")] = None
    kv_cache_usage_perc: Annotated[
        _Metric,
        Field(title="GPU Cache Usage", json_schema_extra={"fmt": ".0%", "bar": True}),
    ] = None
    prompt_tokens_per_second: Annotated[
        _Metric, Field(title="Prompt Tokens/s", json_schema_extra={"fmt": ".1f"})
    ] = None
    generation_tokens_per_second: Annotated[
        _Metric, Field(title="Generation Tokens/s", json_schema_extra={"fmt": ".1f"})
    ] = None
    request_success_total: Annotated[_Metric, Field(title="Requests Success")] = None
    request_error_total: Annotated[_Metric, Field(title="Requests Error")] = None
    request_abort_total: Annotated[_Metric, Field(title="Requests Aborted")] = None
    ttft_p95_seconds: Annotated[
        _Metric, Field(title="TTFT p95 (s)", json_schema_extra={"fmt": ".3f"})
    ] = None
    tpot_p95_seconds: Annotated[
        _Metric, Field(title="TPOT p95 (s)", json_schema_extra={"fmt": ".3f"})
    ] = None
    prefix_cache_hit_rate: Annotated[
        _Metric, Field(title="Prefix Cache Hit Rate", json_schema_extra={"fmt": ".0%"})
    ] = None


class MetricSnapshot(BaseModel):
    model_name: str | None = None
    window: str
    metrics: Metrics = Field(default_factory=Metrics)


class Finding(BaseModel):
    severity: Severity
    confidence: Confidence
    title: str
    summary: str
    signals: list[str] = []
    evidence: list[str] = []
    likely_causes: list[str] = []
    recommendations: list[str] = []
    related_metrics: list[str] = []


class DiagnosisResult(BaseModel):
    snapshot: MetricSnapshot
    findings: list[Finding]

    @property
    def health(self) -> Health:
        if not self.findings:
            return Health.ok
        worst = min(self.findings, key=lambda f: list(Severity).index(f.severity))
        return Health(worst.severity.value)
