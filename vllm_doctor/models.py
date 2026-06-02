from enum import Enum

from pydantic import BaseModel

from vllm_doctor.metrics import Metrics, MetricSeriesSnapshot


class ClientMode(str, Enum):
    """Which collection backend is in use: a Prometheus API or a vLLM /metrics scrape."""

    prometheus = "prometheus"
    scrape = "scrape"


class Severity(str, Enum):
    """How urgent a finding is, independent of how confident we are."""

    critical = "critical"
    warning = "warning"
    info = "info"


class Health(str, Enum):
    """Overall health rollup: the worst severity across all fired findings, or `ok`."""

    ok = "ok"
    info = "info"
    warning = "warning"
    critical = "critical"


class Confidence(str, Enum):
    """How sure a rule is about its finding given the observed signals."""

    high = "high"
    medium = "medium"
    low = "low"


class DiagnosisContext(BaseModel):
    """The query context for a single diagnosis run: time window, model filter, client mode."""

    since: str
    model_name: str | None = None
    client_mode: ClientMode = ClientMode.prometheus


class FindingData(BaseModel):
    """What a rule returns when it fires. The orchestration layer turns this into a `Finding`."""

    confidence: Confidence
    summary: str
    signals: list[str] = []
    evidence: list[str] = []
    severity: Severity | None = None


class Finding(BaseModel):
    """A diagnosis result for one fired rule: severity, confidence, summary, and supporting detail."""

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
    """One rule's outcome: its identity and the finding (or None if the rule did not fire)."""

    id: str
    name: str
    finding: Finding | None = None


class DiagnosisResult(BaseModel):
    """The full output of one diagnosis run: context, observed metrics, and per-rule results."""

    context: DiagnosisContext
    metric_series: MetricSeriesSnapshot
    checks: list[RuleResult] = []

    @property
    def metrics(self) -> Metrics:
        return self.metric_series.to_metrics()

    @property
    def health(self) -> Health:
        fired = [c.finding for c in self.checks if c.finding is not None]
        if not fired:
            return Health.ok
        worst = min(fired, key=lambda f: list(Severity).index(f.severity))
        return Health(worst.severity.value)
