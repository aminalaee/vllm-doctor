from pydantic import BaseModel


class MetricSample(BaseModel):
    """One labeled value from a Prometheus query.

    A single time series sample: a set of labels (dimensions) and the numeric
    value at the queried time. The label set distinguishes pods, models,
    finished_reason values, etc.
    """

    labels: dict[str, str]
    value: float
    timestamp: float | None = None


def label_selector(model: str | None = None, **labels: str) -> str:
    """Builds a Prometheus label selector, e.g. {model_name="foo",finished_reason="stop"}."""
    parts = [f'model_name="{model}"'] if model else []
    parts += [f'{k}="{v}"' for k, v in labels.items()]
    return "{" + ",".join(parts) + "}" if parts else ""
