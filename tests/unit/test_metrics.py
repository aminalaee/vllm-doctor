import pytest

from vllm_doctor.clients.models import MetricSample
from vllm_doctor.metrics import Aggregate, MetricSeries, aggregate


def test_metric_series_accepts_scalar_value() -> None:
    series = MetricSeries.model_validate(2.5)

    assert series.samples == [MetricSample(labels={}, value=2.5)]


def test_metric_series_accepts_missing_value() -> None:
    series = MetricSeries.model_validate(None)

    assert series.samples == []


def test_metric_series_aggregates_samples() -> None:
    series = MetricSeries(
        samples=[
            MetricSample(labels={"pod": "a"}, value=2.0),
            MetricSample(labels={"pod": "b"}, value=4.0),
        ]
    )

    assert series.sum() == 6.0
    assert series.max() == 4.0
    assert series.avg() == 3.0


def test_empty_metric_series_has_no_scalar_value() -> None:
    series = MetricSeries()

    assert series.sum() is None
    assert series.max() is None
    assert series.avg() is None


def test_metric_series_groups_by_label() -> None:
    series = MetricSeries(
        samples=[
            MetricSample(labels={"pod": "a", "model_name": "llama"}, value=2.0),
            MetricSample(labels={"pod": "a", "model_name": "llama"}, value=3.0),
            MetricSample(labels={"pod": "b", "model_name": "llama"}, value=4.0),
            MetricSample(labels={"model_name": "llama"}, value=5.0),
        ]
    )

    assert series.by("pod") == {"a": 5.0, "b": 4.0}


def test_metric_series_filters_by_labels() -> None:
    series = MetricSeries(
        samples=[
            MetricSample(labels={"pod": "a", "model_name": "llama"}, value=2.0),
            MetricSample(labels={"pod": "b", "model_name": "llama"}, value=3.0),
            MetricSample(labels={"pod": "a", "model_name": "mistral"}, value=4.0),
        ]
    )

    filtered = series.filter(pod="a", model_name="llama")

    assert filtered.samples == [MetricSample(labels={"pod": "a", "model_name": "llama"}, value=2.0)]


def test_aggregate_avg() -> None:
    series = MetricSeries(
        samples=[
            MetricSample(labels={}, value=2.0),
            MetricSample(labels={}, value=4.0),
        ]
    )

    assert aggregate(series, Aggregate.avg) == 3.0


def test_aggregate_rejects_unknown_strategy() -> None:
    with pytest.raises(ValueError, match="Unsupported aggregate"):
        aggregate(MetricSeries(), object())  # type: ignore[arg-type]
