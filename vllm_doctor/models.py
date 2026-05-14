from enum import Enum

from pydantic import BaseModel


class Severity(str, Enum):
    critical = "critical"
    warning = "warning"
    info = "info"


class Confidence(str, Enum):
    low = "low"
    medium = "medium"
    high = "high"


class MetricSnapshot(BaseModel):
    model_name: str | None = None
    window: str
    num_requests_running: float | None = None
    num_requests_waiting: float | None = None
    gpu_cache_usage_perc: float | None = None


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
