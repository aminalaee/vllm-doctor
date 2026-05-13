from pydantic import BaseModel


class MetricSample(BaseModel):
    labels: dict[str, str]
    value: float
    timestamp: float | None = None
