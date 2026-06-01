from enum import Enum
from typing import Annotated, TypeAlias

from pydantic import BaseModel, Field

_Metric: TypeAlias = float | None


class ClientMode(str, Enum):
    prometheus = "prometheus"
    scrape = "scrape"


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
    high = "high"
    medium = "medium"
    low = "low"


class Metrics(BaseModel):
    num_requests_running: Annotated[_Metric, Field(title="Requests Running")] = None
    num_requests_waiting: Annotated[_Metric, Field(title="Requests Waiting")] = None
    kv_cache_usage_perc: Annotated[
        _Metric,
        Field(title="GPU Cache Usage", json_schema_extra={"fmt": ".0%", "bar": True}),
    ] = None
    prompt_tokens_per_second: Annotated[_Metric, Field(title="Prefill Tokens/s", json_schema_extra={"fmt": ".1f"})] = (
        None
    )
    generation_tokens_per_second: Annotated[
        _Metric, Field(title="Decode Tokens/s", json_schema_extra={"fmt": ".1f"})
    ] = None
    request_success_total: Annotated[_Metric, Field(title="Requests Success")] = None
    request_error_total: Annotated[_Metric, Field(title="Requests Error")] = None
    request_abort_total: Annotated[_Metric, Field(title="Requests Aborted")] = None
    ttft_p95_seconds: Annotated[_Metric, Field(title="TTFT p95 (s)", json_schema_extra={"fmt": ".3f"})] = None
    tpot_p95_seconds: Annotated[_Metric, Field(title="TPOT p95 (s)", json_schema_extra={"fmt": ".3f"})] = None
    prefix_cache_hit_rate: Annotated[
        _Metric, Field(title="Prefix Cache Hit Rate", json_schema_extra={"fmt": ".0%"})
    ] = None
    queue_time_p95_seconds: Annotated[_Metric, Field(title="Queue Time p95 (s)", json_schema_extra={"fmt": ".3f"})] = (
        None
    )
    num_preemptions_total: Annotated[_Metric, Field(title="Preemptions Total")] = None


class DiagnosisContext(BaseModel):
    since: str
    model_name: str | None = None
    client_mode: ClientMode = ClientMode.prometheus


class FindingData(BaseModel):
    confidence: Confidence
    summary: str
    signals: list[str] = []
    evidence: list[str] = []
    severity: Severity | None = None


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


class RuleResult(BaseModel):
    id: str
    name: str
    finding: Finding | None = None


class DiagnosisResult(BaseModel):
    context: DiagnosisContext
    metrics: Metrics
    checks: list[RuleResult] = []

    @property
    def health(self) -> Health:
        fired = [c.finding for c in self.checks if c.finding is not None]
        if not fired:
            return Health.ok
        worst = min(fired, key=lambda f: list(Severity).index(f.severity))
        return Health(worst.severity.value)
