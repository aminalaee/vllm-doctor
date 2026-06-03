from abc import ABC, abstractmethod
from dataclasses import dataclass
from enum import Enum
from typing import Annotated, Any, TypeAlias

from pydantic import BaseModel, Field, model_validator

from vllm_doctor.clients.models import MetricSample

_Metric: TypeAlias = float | None


class Metrics(BaseModel):
    """Scalar metric values exposed in reports."""

    num_requests_running: _Metric = None
    num_requests_waiting: _Metric = None
    kv_cache_usage_perc: _Metric = None
    prompt_tokens_per_second: _Metric = None
    generation_tokens_per_second: _Metric = None
    request_success_total: _Metric = None
    request_error_total: _Metric = None
    request_abort_total: _Metric = None
    ttft_p95_seconds: _Metric = None
    tpot_p95_seconds: _Metric = None
    prefix_cache_hit_rate: _Metric = None
    queue_time_p95_seconds: _Metric = None
    num_preemptions_total: _Metric = None


class Aggregate(str, Enum):
    """How a labeled metric series becomes a scalar metric value."""

    sum = "sum"
    max = "max"
    avg = "avg"


class MetricSeries(BaseModel):
    """A collection of samples for one metric, preserving labels."""

    samples: list[MetricSample] = Field(default_factory=list)
    aggregate_by: Aggregate = Field(default=Aggregate.sum, exclude=True)

    @model_validator(mode="before")
    @classmethod
    def _coerce_scalar(cls, value: Any) -> Any:
        if value is None:
            return {"samples": []}
        if isinstance(value, (int, float)) and not isinstance(value, bool):
            return {"samples": [{"labels": {}, "value": float(value)}]}
        return value

    def sum(self) -> float | None:
        if not self.samples:
            return None
        return sum(s.value for s in self.samples)

    def max(self) -> float | None:
        if not self.samples:
            return None
        return max(s.value for s in self.samples)

    def avg(self) -> float | None:
        if not self.samples:
            return None
        total = self.sum()
        assert total is not None
        return total / len(self.samples)

    def value(self) -> float | None:
        return aggregate(self, self.aggregate_by)

    def by(self, dim: str) -> dict[str, float | None]:
        """Group samples by the given dimension, aggregating within each group via `aggregate_by`."""
        groups: dict[str, list[MetricSample]] = {}
        for sample in self.samples:
            key = sample.labels.get(dim)
            if key is None:
                continue
            groups.setdefault(key, []).append(sample)
        return {
            key: MetricSeries(samples=samples, aggregate_by=self.aggregate_by).value()
            for key, samples in groups.items()
        }

    def filter(self, **labels: str) -> "MetricSeries":
        """Return a new series containing only samples matching all given labels."""
        matching = [sample for sample in self.samples if all(sample.labels.get(k) == v for k, v in labels.items())]
        return MetricSeries(samples=matching, aggregate_by=self.aggregate_by)


_SeriesMetric = Annotated[MetricSeries, Field(default_factory=MetricSeries)]


class MetricSeriesSnapshot(BaseModel):
    """Raw labeled metric series used internally to derive scalar Metrics.

    Each field's `aggregate_by` is set from the matching `MetricSpec` after
    construction, so `series.value()` and `series.by(label)` produce the
    metric's intended aggregation regardless of how the snapshot was built
    (collector output, test fixture, deserialized JSON).
    """

    num_requests_running: _SeriesMetric
    num_requests_waiting: _SeriesMetric
    kv_cache_usage_perc: _SeriesMetric
    prompt_tokens_per_second: _SeriesMetric
    generation_tokens_per_second: _SeriesMetric
    request_success_total: _SeriesMetric
    request_error_total: _SeriesMetric
    request_abort_total: _SeriesMetric
    ttft_p95_seconds: _SeriesMetric
    tpot_p95_seconds: _SeriesMetric
    prefix_cache_hit_rate: _SeriesMetric
    queue_time_p95_seconds: _SeriesMetric
    num_preemptions_total: _SeriesMetric

    @model_validator(mode="after")
    def _apply_spec_aggregation(self) -> "MetricSeriesSnapshot":
        for spec in METRIC_SPECS:
            if isinstance(spec, Direct):
                getattr(self, spec.output).aggregate_by = spec.aggregate_by
        return self

    def to_metrics(self) -> Metrics:
        return Metrics(**{name: getattr(self, name).value() for name in type(self).model_fields})


def aggregate(series: MetricSeries, aggregate: Aggregate) -> float | None:
    if aggregate == Aggregate.sum:
        return series.sum()
    if aggregate == Aggregate.max:
        return series.max()
    if aggregate == Aggregate.avg:
        return series.avg()
    raise ValueError(f"Unsupported aggregate: {aggregate}")


class MetricSpec(ABC):
    """How to derive one output metric from one or more probes."""

    output: str
    display: "MetricDisplay"

    @abstractmethod
    def probe_names(self) -> set[str]: ...

    @abstractmethod
    def compute(self, raw: dict[str, MetricSeries]) -> MetricSeries: ...


@dataclass(frozen=True)
class MetricDisplay:
    """How a metric is rendered in reports."""

    title: str
    fmt: str = ".0f"
    bar: bool = False


REPLICA_LABELS = ("pod", "pod_name", "kubernetes_pod_name", "instance", "host", "hostname", "server", "endpoint")


def detect_replica_label(snapshot: "MetricSeriesSnapshot") -> str | None:
    """Pick the first known label that has >1 distinct values across the metric series.

    Returns None when the snapshot looks like a single-replica deployment.
    """
    for label in REPLICA_LABELS:
        values: set[str] = set()
        for spec in METRIC_SPECS:
            series = getattr(snapshot, spec.output)
            values.update(s.labels[label] for s in series.samples if label in s.labels)
        if len(values) > 1:
            return label
    return None


@dataclass(frozen=True)
class Direct(MetricSpec):
    """Pass-through: the output is the probe's metric series, unchanged."""

    output: str
    probe: str
    display: MetricDisplay
    aggregate_by: Aggregate = Aggregate.sum

    def probe_names(self) -> set[str]:
        return {self.probe}

    def compute(self, raw: dict[str, MetricSeries]) -> MetricSeries:
        return raw[self.probe].model_copy(update={"aggregate_by": self.aggregate_by})


@dataclass(frozen=True)
class Ratio(MetricSpec):
    """The aggregate ratio of two probes: `numerator.sum() / denominator.sum()`."""

    output: str
    numerator: str
    denominator: str
    display: MetricDisplay

    def probe_names(self) -> set[str]:
        return {self.numerator, self.denominator}

    def compute(self, raw: dict[str, MetricSeries]) -> MetricSeries:
        numerator = raw[self.numerator].sum()
        denominator = raw[self.denominator].sum()
        if numerator is None or denominator is None or denominator <= 0:
            return MetricSeries()
        return MetricSeries(samples=[MetricSample(labels={}, value=numerator / denominator)])


METRIC_SPECS: list[MetricSpec] = [
    Direct("num_requests_running", probe="num_requests_running", display=MetricDisplay("Requests Running")),
    Direct("num_requests_waiting", probe="num_requests_waiting", display=MetricDisplay("Requests Waiting")),
    Direct(
        "kv_cache_usage_perc",
        probe="kv_cache_usage_perc",
        display=MetricDisplay("GPU Cache Usage", fmt=".0%", bar=True),
        aggregate_by=Aggregate.max,
    ),
    Direct(
        "prompt_tokens_per_second",
        probe="prompt_tokens_per_second",
        display=MetricDisplay("Prefill Tokens/s", fmt=".1f"),
    ),
    Direct(
        "generation_tokens_per_second",
        probe="generation_tokens_per_second",
        display=MetricDisplay("Decode Tokens/s", fmt=".1f"),
    ),
    Direct("request_success_total", probe="request_success_total", display=MetricDisplay("Requests Success")),
    Direct("request_error_total", probe="request_error_total", display=MetricDisplay("Requests Error")),
    Direct("request_abort_total", probe="request_abort_total", display=MetricDisplay("Requests Aborted")),
    Direct(
        "ttft_p95_seconds",
        probe="ttft_p95_seconds",
        display=MetricDisplay("TTFT p95 (s)", fmt=".3f"),
        aggregate_by=Aggregate.max,
    ),
    Direct(
        "tpot_p95_seconds",
        probe="tpot_p95_seconds",
        display=MetricDisplay("TPOT p95 (s)", fmt=".3f"),
        aggregate_by=Aggregate.max,
    ),
    Direct(
        "queue_time_p95_seconds",
        probe="queue_time_p95_seconds",
        display=MetricDisplay("Queue Time p95 (s)", fmt=".3f"),
        aggregate_by=Aggregate.max,
    ),
    Direct("num_preemptions_total", probe="num_preemptions_total", display=MetricDisplay("Preemptions Total")),
    Ratio(
        "prefix_cache_hit_rate",
        numerator="prefix_hits",
        denominator="prefix_queries",
        display=MetricDisplay("Prefix Cache Hit Rate", fmt=".0%"),
    ),
]

METRIC_SPECS_BY_OUTPUT = {spec.output: spec for spec in METRIC_SPECS}
